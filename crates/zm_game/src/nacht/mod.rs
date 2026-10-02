//! Nacht der Untoten, loaded from the user's install: a background task reads
//! the fastfiles and builds meshes/textures (see [`build`]), then the scene is
//! uploaded and spawned when a session starts.

pub mod build;
pub mod level;
pub mod world_material;

use crate::menu::{CurrentMap, LoadingStatus, MapKind};
use crate::waw::Waw;
use crate::world::{Debris, Mats};
use crate::{Dynamic, GameState, SessionEntity};
use bevy::pbr::NotShadowCaster;
use bevy::render::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use build::{angles_to_quat, to_bevy, Blend, NachtScene};
use std::collections::HashMap;
use bevy::render::render_resource::PipelineCache;
use bevy::render::{Render, RenderApp, RenderSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use zm_core::level::Level;
use zm_core::navgraph::NavGraph;
use zm_core::trimesh::TriMesh;

#[derive(Resource)]
struct NachtTask(Task<Result<NachtScene, String>>);

/// Why loading Nacht failed (shown on the loading screen).
#[derive(Resource, Default)]
pub struct NachtError(pub Option<String>);

/// Marker: the current session is on Nacht.
#[derive(Resource)]
pub struct NachtActive;

/// A model's surfaces as uploaded meshes and materials.
#[derive(Clone)]
pub struct ModelParts {
    pub parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
}

/// Everything uploaded from the scene, kept for the rest of the run so
/// restarting is instant.
#[derive(Resource)]
pub struct NachtAssets {
    pub world: Vec<(Handle<Mesh>, WorldMat)>,
    pub submodels: HashMap<usize, Vec<(Handle<Mesh>, Handle<StandardMaterial>)>>,
    pub models: HashMap<String, ModelParts>,
    pub static_models: Vec<(String, Transform)>,
    pub sky_model: Option<String>,
    pub sky_scale: f32,
    pub collision: Arc<TriMesh>,
    pub nav: Arc<NavGraph>,
    pub level: Level,
    pub scene_entities: Vec<waw_assets::mapents::Entity>,
    /// Brush submodels already used by gameplay (boards, doors) by number.
    pub gameplay_submodels: Vec<usize>,
    pub chest: Option<build::SceneChest>,
    /// The light grid as an irradiance volume for models.
    pub irradiance: Option<(Handle<Image>, Transform)>,
    /// Fog and film grade from the map's own data.
    pub fog: Option<waw_assets::look::Fog>,
    pub film: Option<waw_assets::look::Film>,
    /// Our weapon id -> world model name (in `models`).
    pub weapon_world_models: HashMap<String, String>,
}

/// How a world surface group is drawn.
#[derive(Clone)]
pub enum WorldMat {
    /// The game's lit world shader (lightmap + primary light).
    Lit(Handle<world_material::WawWorldMaterial>),
    /// Unlit or otherwise plain surfaces.
    Plain(Handle<StandardMaterial>),
}

/// A model skinned to its own skeleton, uploaded.
#[derive(Clone)]
pub struct PartAssets {
    /// (bone name, parent index, bind pose relative to the parent)
    pub bones: Vec<(String, Option<usize>, Transform)>,
    pub inverse_bindposes: Handle<SkinnedMeshInverseBindposes>,
    pub meshes: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
}

/// A skinned zombie model ready to instantiate.
pub struct CharAssets {
    pub body: PartAssets,
    pub heads: Vec<PartAssets>,
}

/// The game's display names of our weapons ("Colt M1911", "Kar98k").
#[derive(Resource, Default)]
pub struct WeaponNames(pub HashMap<String, String>);

/// First-person arms, guns and weapon animations.
#[derive(Resource, Default)]
pub struct ViewRig {
    pub arms: Option<PartAssets>,
    pub guns: HashMap<String, PartAssets>,
    /// weapon id -> slot -> clip
    pub anims: HashMap<String, HashMap<&'static str, Arc<build::AnimClip>>>,
}

/// A joint spawned for a skeleton: (bone name, entity, bind local pose).
pub type Joint = (String, Entity, Transform);

/// Spawns `part`'s skeleton under `root` (sharing joints already in
/// `joints` by name) and its skinned meshes under `mesh_parent`.
pub fn spawn_part(commands: &mut Commands, part: &PartAssets, joints: &mut Vec<Joint>, root: Entity, mesh_parent: Entity) {
    let mut mine: Vec<Entity> = Vec::with_capacity(part.bones.len());
    for (name, parent, local) in &part.bones {
        if let Some((_, e, _)) = joints.iter().find(|j| j.0 == *name) {
            mine.push(*e);
            continue;
        }
        let parent_e = parent.and_then(|p| mine.get(p).copied()).unwrap_or(root);
        let e = commands.spawn((*local, ChildOf(parent_e), Name::new(name.clone()))).id();
        joints.push((name.clone(), e, *local));
        mine.push(e);
    }
    for (mesh, mat) in &part.meshes {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            bevy::render::mesh::skinning::SkinnedMesh { inverse_bindposes: part.inverse_bindposes.clone(), joints: mine.clone() },
            NotShadowCaster,
            // Bind-pose bounds don't follow the skeleton (heads are modelled
            // around their own origin), so don't frustum-cull skinned parts.
            bevy::render::view::NoFrustumCulling,
            ChildOf(mesh_parent),
        ));
    }
}

/// For each track of `clip`, the index of the joint it drives.
pub fn track_map(clip: &build::AnimClip, joints: &[Joint]) -> Vec<Option<usize>> {
    clip.tracks.iter().map(|t| joints.iter().position(|j| j.0 == t.bone)).collect()
}

/// Poses joints from `clip` at `frame`: rotations replace the bind
/// rotation, translations are offsets from the bind position. Bones the
/// clip does not mention keep their pose. `only` limits it to one bone.
pub fn pose_mapped(
    clip: &build::AnimClip,
    frame: f32,
    joints: &[Joint],
    map: &[Option<usize>],
    tq: &mut Query<&mut Transform>,
    only: Option<&str>,
) {
    for (t, j) in clip.tracks.iter().zip(map) {
        let Some(j) = j else { continue };
        if only.is_some_and(|o| o != t.bone) {
            continue;
        }
        let (_, e, bind) = &joints[*j];
        if let Ok(mut tr) = tq.get_mut(*e) {
            tr.rotation = t.rotation(frame);
            tr.translation = bind.translation + t.offset(frame);
        }
    }
}

/// First-person gun models by weapon id, once the install has been read.
#[derive(Resource, Default)]
pub struct ViewModels(pub HashMap<String, Vec<(Handle<Mesh>, Handle<StandardMaterial>)>>);

/// The real zombie models and their animations, once the install has been read.
#[derive(Resource, Default)]
pub struct ZombieModels {
    pub chars: Vec<CharAssets>,
    pub clips: Vec<Arc<build::AnimClip>>,
}

impl ZombieModels {
    pub fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name == name)
    }
}

/// Follows the camera so the skybox model always surrounds the player.
#[derive(Component)]
pub struct SkyBox;

/// Number of render pipelines still compiling (mirrored from the render
/// world), so loading screens can wait until a new scene can be drawn.
#[derive(Resource, Clone, Default)]
pub struct PendingPipelines(pub Arc<AtomicUsize>);

impl PendingPipelines {
    pub fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

fn count_pending_pipelines(cache: Res<PipelineCache>, pending: Res<PendingPipelines>) {
    pending.0.store(cache.waiting_pipelines().count(), Ordering::Relaxed);
}

pub struct NachtPlugin;

impl Plugin for NachtPlugin {
    fn build(&self, app: &mut App) {
        let pending = PendingPipelines::default();
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(pending.clone()).add_systems(Render, count_pending_pipelines.in_set(RenderSet::Cleanup));
        }
        app.insert_resource(pending)
            .init_resource::<ZombieModels>()
            .init_resource::<ViewModels>()
            .init_resource::<ViewRig>()
            .init_resource::<NachtError>()
            .add_plugins(world_material::WorldMaterialPlugin)
            .add_systems(Startup, start_load)
            .add_systems(OnEnter(GameState::Loading), start_load)
            .add_systems(Update, poll_load)
            .add_systems(Update, follow_sky)
            .add_systems(Update, chest_visuals.run_if(resource_exists::<NachtActive>))
            .add_systems(Update, ambience.run_if(in_state(GameState::Playing).and(resource_exists::<NachtActive>)));
    }
}

/// Starts reading the install in the background: at startup (so the menu
/// is instant and zone sounds are available everywhere) and on demand.
#[allow(clippy::too_many_arguments)]
fn start_load(
    mut commands: Commands,
    state: Res<State<GameState>>,
    current: Res<CurrentMap>,
    waw: Res<Waw>,
    task: Option<Res<NachtTask>>,
    assets: Option<Res<NachtAssets>>,
    device: Option<Res<RenderDevice>>,
    mut error: ResMut<NachtError>,
    (bank, defs): (Option<Res<crate::audio::SoundBank>>, Res<crate::Defs>),
) {
    let needed = current.0 == MapKind::Nacht || *state.get() == GameState::MainMenu;
    if !needed || task.is_some() || assets.is_some() || (error.0.is_some() && *state.get() == GameState::MainMenu) {
        return;
    }
    error.0 = None;
    let (Some(install), Some(iwd)) = (waw.install.clone(), waw.iwd.clone()) else {
        error.0 = Some(waw.error.clone().unwrap_or_else(|| "World at War install not found".into()));
        return;
    };
    let bc = crate::waw::bc_supported(device.as_deref());
    let mut aliases = bank.map(|b| b.wanted_aliases()).unwrap_or_default();
    aliases.extend(crate::grenades::ALIASES.iter().map(|a| a.to_string()));
    let wanted = build::Wanted {
        aliases,
        // The offhand grenade is loaded like a weapon (viewmodel, anims, sounds).
        weapon_ids: defs.0.iter().map(|d| d.id).chain([crate::grenades::GRENADE_ID]).collect(),
    };
    let t = AsyncComputeTaskPool::get().spawn(async move { build::build(&install, &iwd, bc, wanted) });
    commands.insert_resource(NachtTask(t));
}

#[allow(clippy::too_many_arguments)]
fn poll_load(
    mut commands: Commands,
    task: Option<ResMut<NachtTask>>,
    mut error: ResMut<NachtError>,
    mut status: Query<&mut Text, With<LoadingStatus>>,
    time: Res<Time>,
    (mut meshes, mut images, mut materials, mut audio, mut bindposes): (
        ResMut<Assets<Mesh>>,
        ResMut<Assets<Image>>,
        ResMut<Assets<StandardMaterial>>,
        ResMut<Assets<AudioSource>>,
        ResMut<Assets<SkinnedMeshInverseBindposes>>,
    ),
    (mut zombie_models, mut view_models, mut view_rig): (ResMut<ZombieModels>, ResMut<ViewModels>, ResMut<ViewRig>),
    mut mats: ResMut<Mats>,
    mut zone_sounds: ResMut<crate::audio::ZoneSounds>,
    mut world_mats: ResMut<Assets<world_material::WawWorldMaterial>>,
    mut defs: ResMut<crate::Defs>,
) {
    if let Some(e) = &error.0 {
        if let Ok(mut t) = status.single_mut() {
            t.0 = format!("COULD NOT LOAD THE MAP: {e}\nPress Esc to return to the menu");
        }
        return;
    }
    let Some(mut task) = task else { return };
    if let Ok(mut t) = status.single_mut() {
        let dots = ".".repeat(1 + (time.elapsed_secs() * 2.0) as usize % 3);
        t.0 = format!("READING YOUR WORLD AT WAR INSTALL{dots}");
    }
    let Some(result) = block_on(future::poll_once(&mut task.0)) else { return };
    commands.remove_resource::<NachtTask>();
    let scene = match result {
        Ok(s) => s,
        Err(e) => {
            error!("Nacht failed to load: {e}");
            error.0 = Some(e);
            return;
        }
    };
    info!(
        "Nacht built in {:.1}s: {} world meshes, {} materials, {} textures, {} models, {} static models, {} collision triangles",
        scene.load_secs,
        scene.world.len(),
        scene.materials.len(),
        scene.images.len(),
        scene.models.len(),
        scene.static_models.len(),
        scene.collision.tris.len()
    );
    let level = level::build_level(&scene);
    let nav = level::build_nav(&scene, &level, &scene.collision);
    info!("Nacht nav graph: {} nodes, {} links", nav.nodes.len(), nav.edges.iter().map(Vec::len).sum::<usize>() / 2);

    commands.insert_resource(crate::MapRules(scene.rules.clone()));
    let NachtScene {
        images: imgs, lightmap_pages, irradiance, fog, film, lights, materials: mdefs, world, submodels, models, static_models, sky_model, sky_scale, collision, entities, sounds, weapon_sounds, weapon_names, weapon_world_models, chest, map, characters, view_models: vms, zombie_anims, view_rig: vr, weapon_stats, flesh_penetration, fx, ..
    } = scene;
    crate::audio::apply_zone_weapon_stats(&mut defs.0, &weapon_stats, flesh_penetration);
    commands.insert_resource(crate::fx::PendingFx(fx));
    let image_handles: HashMap<String, Handle<Image>> = imgs.into_iter().map(|(k, v)| (k, images.add(v))).collect();
    let mat_handles: Vec<Handle<StandardMaterial>> = mdefs
        .iter()
        .map(|m| {
            let tex = m.color.as_ref().and_then(|c| image_handles.get(c).cloned());
            materials.add(StandardMaterial {
                base_color: if tex.is_some() { Color::WHITE } else { Color::srgb(0.5, 0.5, 0.5) },
                base_color_texture: tex,
                perceptual_roughness: 0.92,
                reflectance: 0.2,
                alpha_mode: match m.blend {
                    Blend::Opaque => AlphaMode::Opaque,
                    Blend::Mask => AlphaMode::Mask(0.5),
                    Blend::Blend => AlphaMode::Blend,
                    Blend::Add => AlphaMode::Add,
                },
                unlit: m.unlit,
                double_sided: m.two_sided,
                cull_mode: if m.two_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
                // Constant offset only (Bevy has no slope bias here): a few
                // depth-buffer steps per decal layer.
                depth_bias: 4.0 * m.depth_bias.max(if m.blend == Blend::Blend { 2.0 } else { 0.0 }),
                ..default()
            })
        })
        .collect();
    let mut upload = |p: build::SkinnedPart| -> PartAssets {
        let inv: Vec<Mat4> = p.bones.iter().map(|b| b.inv_bind).collect();
        PartAssets {
            bones: p.bones.iter().map(|b| (b.name.clone(), b.parent, b.local)).collect(),
            inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(inv)),
            meshes: p.meshes.into_iter().map(|s| (meshes.add(s.mesh), mat_handles[s.material].clone())).collect(),
        }
    };
    zombie_models.chars = characters.into_iter().map(|c| CharAssets { body: upload(c.body), heads: c.heads.into_iter().map(&mut upload).collect() }).collect();
    zombie_models.clips = zombie_anims.into_iter().map(Arc::new).collect();
    if let Some(vr) = vr {
        view_rig.arms = Some(upload(vr.arms));
        view_rig.guns = vr.guns.into_iter().map(|(k, v)| (k, upload(v))).collect();
        view_rig.anims = vr.anims.into_iter().map(|(k, v)| (k, v.into_iter().map(|(s, c)| (s, Arc::new(c))).collect())).collect();
    }
    view_models.0 = vms
        .into_iter()
        .map(|(id, parts)| (id, parts.into_iter().map(|s| (meshes.add(s.mesh), mat_handles[s.material].clone())).collect()))
        .collect();
    info!(
        "Zombie models: {} ({} animations), weapon view models: {}, first-person rig: {} guns",
        zombie_models.chars.len(),
        zombie_models.clips.len(),
        view_models.0.len(),
        view_rig.guns.len()
    );
    // Lit world surfaces use the game's shader, one material per (material,
    // primary light) pair.
    let pages: Vec<Option<(Handle<Image>, Handle<Image>)>> = lightmap_pages.into_iter().map(|p| p.map(|(sec, pri)| (images.add(sec), images.add(pri)))).collect();
    let mut lit_cache: HashMap<(usize, u8, u8, bool), Handle<world_material::WawWorldMaterial>> = HashMap::new();
    let world: Vec<_> = world
        .into_iter()
        .map(|s| {
            let m = &mdefs[s.material];
            let mat = match pages.get(s.lightmap as usize).cloned().flatten() {
                Some((sec, pri)) if m.lightmapped && !m.unlit => {
                    let h = lit_cache.entry((s.material, s.primary_light, s.lightmap, s.decal)).or_insert_with(|| {
                        use world_material::*;
                        let color = m.color.as_ref().and_then(|c| image_handles.get(c).cloned());
                        let normal = m.normal.as_ref().and_then(|c| image_handles.get(c).cloned());
                        let mut flags = 0;
                        if color.is_some() {
                            flags |= FLAG_COLOR;
                        }
                        if normal.is_some() {
                            flags |= FLAG_NORMAL;
                        }
                        if m.vertex_tint {
                            flags |= FLAG_VERTEX_COLOR;
                        }
                        if m.blend == Blend::Mask {
                            flags |= FLAG_ALPHA_TEST;
                        }
                        let l = lights.get(s.primary_light as usize).copied().unwrap_or_default();
                        world_mats.add(WawWorldMaterial {
                            params: WorldParams {
                                flags,
                                light_kind: l.kind as u32,
                                falloff: l.falloff as u32,
                                exponent: l.exponent,
                                light_color: l.color.extend(1.0),
                                light_pos: l.position.extend(1.0 / l.radius),
                                light_dir: l.dir.extend(0.0),
                                spot: Vec4::new(l.cos_outer, l.cos_inner, 0.0, 0.0),
                            },
                            color,
                            normal,
                            lightmap_secondary: sec.clone(),
                            lightmap_primary: pri.clone(),
                            alpha: match m.blend {
                                Blend::Opaque | Blend::Mask => AlphaMode::Opaque,
                                Blend::Blend | Blend::Add => AlphaMode::Blend,
                            },
                            two_sided: m.two_sided,
                            // Decal layers by sort key; anything else in the
                            // world's decal range gets the smallest offset.
                            depth_bias: m.depth_bias.max(if s.decal { 2.0 } else { 0.0 }) as u8,
                        })
                    });
                    WorldMat::Lit(h.clone())
                }
                _ => WorldMat::Plain(mat_handles[s.material].clone()),
            };
            (meshes.add(s.mesh), mat)
        })
        .collect();
    let submodels: HashMap<usize, Vec<_>> =
        submodels.into_iter().map(|(n, v)| (n, v.into_iter().map(|s| (meshes.add(s.mesh), mat_handles[s.material].clone())).collect())).collect();
    let models: HashMap<String, ModelParts> = models
        .into_iter()
        .map(|(n, m)| (n, ModelParts { parts: m.surfaces.into_iter().map(|s| (meshes.add(s.mesh), mat_handles[s.material].clone())).collect() }))
        .collect();
    zone_sounds.aliases = sounds
        .into_iter()
        .map(|(k, v)| {
            let variants = v
                .into_iter()
                .map(|s| crate::audio::AliasSound {
                    handle: audio.add(AudioSource { bytes: s.wav.into() }),
                    volume: s.volume,
                    pitch: s.pitch,
                    spatial: s.spatial,
                    distance: s.distance,
                    secondary: s.secondary,
                    chain: s.chain,
                    duration: s.duration,
                    start_delay: s.start_delay,
                })
                .collect();
            (k, variants)
        })
        .collect();
    zone_sounds.weapons = weapon_sounds;
    commands.insert_resource(WeaponNames(weapon_names));
    info!("Zone sounds: {} aliases, {} weapons", zone_sounds.aliases.len(), zone_sounds.weapons.len());
    let missing: Vec<&str> = build::GAME_ALIASES.iter().copied().filter(|a| !zone_sounds.has(a)).collect();
    if !missing.is_empty() {
        warn!("Sound aliases not found in the install: {missing:?}");
    }

    // Boards and door blockers are spawned per session (they can be removed).
    let mut gameplay_submodels: Vec<usize> = level.windows.iter().flat_map(|w| w.board_models.iter().map(|b| b.0)).collect();
    for d in &map.doors {
        gameplay_submodels.extend(d.blockers.iter().copied());
    }
    mats.submodels = submodels.clone();

    commands.insert_resource(NachtAssets {
        world,
        submodels,
        models,
        static_models,
        sky_model,
        sky_scale,
        collision: Arc::new(collision),
        nav: Arc::new(nav),
        level,
        scene_entities: entities,
        gameplay_submodels,
        chest,
        irradiance: irradiance.map(|(img, t)| (images.add(img), t)),
        fog,
        film,
        weapon_world_models,
    });
}

fn spawn_model(commands: &mut Commands, parts: &ModelParts, t: Transform, extra: impl Bundle) -> Entity {
    commands
        .spawn((t, Visibility::default(), extra))
        .with_children(|p| {
            for (mesh, mat) in &parts.parts {
                p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
            }
        })
        .id()
}

/// Spawns the static map: geometry, props, lights and sky.
pub fn spawn_scene(
    mut commands: Commands,
    assets: Res<NachtAssets>,
    mut ambient: ResMut<AmbientLight>,
    mut clear: ResMut<ClearColor>,
) {
    // All light comes from the map's data (lightmaps, grid, primary lights).
    ambient.brightness = 0.0;
    clear.0 = Color::srgb(0.01, 0.012, 0.02);
    for (mesh, mat) in &assets.world {
        match mat {
            WorldMat::Lit(m) => {
                commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(m.clone()), SessionEntity));
            }
            WorldMat::Plain(m) => {
                commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(m.clone()), SessionEntity));
            }
        }
    }
    for (name, t) in &assets.static_models {
        if let Some(parts) = assets.models.get(name) {
            spawn_model(&mut commands, parts, *t, SessionEntity);
        }
    }
    // Brush models that are part of the scenery (not boards or doors).
    for e in &assets.scene_entities {
        let Some(n) = e.submodel() else { continue };
        if assets.gameplay_submodels.contains(&n) || e.classname().starts_with("trigger") {
            continue;
        }
        if let Some(parts) = assets.submodels.get(&n) {
            let t = Transform::from_translation(to_bevy(e.origin())).with_rotation(angles_to_quat(e.angles()));
            commands.spawn((t, Visibility::default(), SessionEntity)).with_children(|p| {
                for (mesh, mat) in parts {
                    p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
                }
            });
        }
    }
    // Script models (barrels, props), except door blockers which are
    // dynamic and the box lid, which opens.
    let lid_name = assets.chest.as_ref().map(|c| c.lid_model.as_str());
    for e in &assets.scene_entities {
        if e.classname() != "script_model" || e.targetname().starts_with("upstairs_blocker") || e.get("model") == lid_name {
            continue;
        }
        let Some(model) = e.get("model") else { continue };
        if let Some(parts) = assets.models.get(model) {
            let t = Transform::from_translation(to_bevy(e.origin())).with_rotation(angles_to_quat(e.angles()));
            spawn_model(&mut commands, parts, t, SessionEntity);
        }
    }
    // Models: the light grid's baked light with each point's primary light
    // added (as the game lights models). The world has both in its own
    // shader. Scaled to the real maps' fixed camera exposure.
    if let Some((voxels, t)) = &assets.irradiance {
        commands.spawn((
            bevy::pbr::LightProbe,
            bevy::pbr::irradiance_volume::IrradianceVolume {
                voxels: voxels.clone(),
                intensity: 1.2 * 2f32.powf(REAL_MAP_EV),
                affects_lightmapped_meshes: false,
            },
            *t,
            SessionEntity,
        ));
    }
    if let Some(parts) = assets.sky_model.as_ref().and_then(|n| assets.models.get(n)) {
        let e = spawn_model(&mut commands, parts, Transform::from_scale(Vec3::splat(assets.sky_scale)), (SessionEntity, SkyBox, NotShadowCaster));
        commands.entity(e).insert(NotShadowCaster);
    }
    spawn_ambience(&mut commands, &assets);
}

/// Camera exposure on the real maps. The game's own lighting (world shader,
/// light grid) is scaled to it, so effects authored for Bevy's units
/// (muzzle flashes, explosions, glows) keep their brightness.
pub const REAL_MAP_EV: f32 = 7.5;

/// Spawns the parts of the map that change during a game: window boards,
/// door blockers and the mystery box.
pub fn spawn_dynamic(
    mut commands: Commands,
    assets: Res<NachtAssets>,
    level: Res<crate::LevelRes>,
    mats: Res<Mats>,
    boards: Res<crate::Boards>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let level = &level.0;
    for (wi, _) in level.windows.iter().enumerate() {
        for b in 0..boards.0[wi] {
            crate::world::spawn_board(&mut commands, level, &mats, wi, b);
        }
    }
    for di in 0..level.doors.len() {
        let Some(src) = assets.level_door_parts(di) else { continue };
        for (kind, t) in src {
            match kind {
                DoorPart::Submodel(n) => {
                    if let Some(parts) = assets.submodels.get(&n) {
                        commands.spawn((t, Visibility::default(), Debris { door: di }, Dynamic)).with_children(|p| {
                            for (mesh, mat) in parts {
                                p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
                            }
                        });
                    }
                }
                DoorPart::Model(name) => {
                    if let Some(parts) = assets.models.get(&name) {
                        spawn_model(&mut commands, parts, t, (Debris { door: di }, Dynamic));
                    }
                }
            }
        }
    }
    match &assets.chest {
        // The map's own box: animate its lid and float the weapons above it.
        Some(chest) => {
            if let Some(parts) = assets.models.get(&chest.lid_model) {
                spawn_model(&mut commands, parts, chest.lid, (ChestLid { base: chest.lid, open: 0.0 }, Dynamic));
            }
            commands.spawn((chest.weapon, Visibility::Hidden, ChestWeapon { base: chest.weapon, shown: None }, Dynamic)).with_children(|p| {
                p.spawn((
                    PointLight { color: Color::srgb(0.75, 0.85, 1.0), intensity: 25_000.0, range: 3.0, shadows_enabled: false, ..default() },
                    Transform::from_xyz(0.0, 0.3, 0.0),
                ));
            });
        }
        None => crate::world::spawn_crate(&mut commands, level, &mats, &mut meshes),
    }
}

enum DoorPart {
    Submodel(usize),
    Model(String),
}

impl NachtAssets {
    /// Visual parts of door `i` (in the same order as the level's doors).
    fn level_door_parts(&self, i: usize) -> Option<Vec<(DoorPart, Transform)>> {
        let map = waw_assets::zombiemap::ZombieMap::from_entities(&self.scene_entities);
        let d = map.doors.get(i)?;
        let mut out = Vec::new();
        for n in &d.blockers {
            if let Some(e) = self.scene_entities.iter().find(|e| e.submodel() == Some(*n)) {
                out.push((DoorPart::Submodel(*n), Transform::from_translation(to_bevy(e.origin())).with_rotation(angles_to_quat(e.angles()))));
            }
        }
        for (model, o, a) in &d.props {
            out.push((DoorPart::Model(model.clone()), Transform::from_translation(to_bevy(*o)).with_rotation(angles_to_quat(*a))));
        }
        Some(out)
    }
}

fn follow_sky(cam: Query<&GlobalTransform, With<crate::player::Player>>, mut sky: Query<&mut Transform, With<SkyBox>>) {
    let Ok(c) = cam.single() else { return };
    for mut t in &mut sky {
        t.translation = c.translation();
    }
}

/// An ambient one-shot that repeats at random intervals.
#[derive(Component)]
struct AmbientRandom {
    alias: String,
    min: f32,
    max: f32,
    wait: f32,
}

/// A looping sound kept at the point of a segment closest to the listener.
#[derive(Component)]
struct LineEmitter {
    a: Vec3,
    b: Vec3,
}

/// The map's ambient sound emitters (structs with `script_sound`), as the
/// game's client script plays them.
fn spawn_ambience(commands: &mut Commands, assets: &NachtAssets) {
    use waw_assets::zombiemap::AmbientKind;
    let map = waw_assets::zombiemap::ZombieMap::from_entities(&assets.scene_entities);
    for em in &map.ambient {
        let at = Transform::from_translation(to_bevy(em.origin));
        match &em.kind {
            AmbientKind::Random { min, max } => {
                let wait = fastrand::f32() * max;
                commands.spawn((at, AmbientRandom { alias: em.alias.clone(), min: *min, max: *max, wait }, SessionEntity));
            }
            AmbientKind::Looper => {
                commands.spawn((at, crate::audio::AliasLoop::new(em.alias.clone()), SessionEntity));
            }
            AmbientKind::Line { end } => {
                let line = LineEmitter { a: to_bevy(em.origin), b: to_bevy(*end) };
                commands.spawn((at, line, crate::audio::AliasLoop::new(em.alias.clone()), SessionEntity));
            }
        }
    }
}

fn ambience(
    time: Res<Time>,
    listener: Query<&GlobalTransform, With<SpatialListener>>,
    mut randoms: Query<(&Transform, &mut AmbientRandom)>,
    mut lines: Query<(&mut Transform, &LineEmitter), Without<AmbientRandom>>,
    mut alias: EventWriter<crate::audio::PlayAlias>,
    mut spooky: Local<f32>,
) {
    let dt = time.delta_secs();
    for (t, mut r) in &mut randoms {
        r.wait -= dt;
        if r.wait <= 0.0 {
            r.wait = r.min + fastrand::f32() * (r.max - r.min).max(0.0);
            alias.write(crate::audio::PlayAlias::at(r.alias.clone(), t.translation));
        }
    }
    // The level's ambient package: a spooky 2D one-shot every 5-8 s.
    *spooky -= dt;
    if *spooky <= 0.0 {
        *spooky = 5.0 + fastrand::f32() * 3.0;
        alias.write(crate::audio::PlayAlias::local("amb_spooky_2d"));
    }
    let Ok(l) = listener.single() else { return };
    let p = l.translation();
    for (mut t, line) in &mut lines {
        let ab = line.b - line.a;
        let k = ((p - line.a).dot(ab) / ab.length_squared().max(1e-4)).clamp(0.0, 1.0);
        t.translation = line.a + ab * k;
    }
}

/// The box lid script model (rolls open 105 degrees in 0.5 s).
#[derive(Component)]
struct ChestLid {
    base: Transform,
    open: f32,
}

/// The weapon floating out of the box while it cycles.
#[derive(Component)]
struct ChestWeapon {
    base: Transform,
    shown: Option<usize>,
}

/// Opens and closes the real box lid and shows the cycling weapon models,
/// rising out of the box as the box script moves them.
#[allow(clippy::type_complexity)]
fn chest_visuals(
    time: Res<Time>,
    mcrate: Res<crate::interact::MysteryCrate>,
    defs: Res<crate::Defs>,
    assets: Option<Res<NachtAssets>>,
    mut lid: Query<(&mut Transform, &mut ChestLid), Without<ChestWeapon>>,
    mut weapon: Query<(Entity, &mut Transform, &mut Visibility, &mut ChestWeapon, Option<&Children>), Without<ChestLid>>,
    meshes: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    use crate::interact::CrateState;
    let Some(assets) = assets else { return };
    let dt = time.delta_secs();
    let (open, shown, rise) = match mcrate.0 {
        CrateState::Idle => (false, None, 0.0),
        CrateState::Rolling { t, shown, .. } => {
            // MoveTo over 3 s, accelerating for 2 s, decelerating for 0.9.
            let k = (t / 3.0).clamp(0.0, 1.0);
            (true, Some(shown), k * k * (3.0 - 2.0 * k))
        }
        CrateState::Ready { def, .. } => (true, Some(def), 1.0),
    };
    for (mut t, mut l) in &mut lid {
        let target = if open { 1.0 } else { 0.0 };
        l.open += (target - l.open).clamp(-dt * 2.0, dt * 2.0);
        let k = l.open * l.open * (3.0 - 2.0 * l.open);
        *t = l.base;
        t.rotation = l.base.rotation * Quat::from_rotation_x((105f32).to_radians() * k);
    }
    for (e, mut t, mut vis, mut w, children) in &mut weapon {
        *vis = if shown.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        t.translation = w.base.translation + Vec3::Y * 40.0 * build::INCH * rise;
        if w.shown != shown {
            w.shown = shown;
            for c in children.into_iter().flatten() {
                if meshes.get(*c).is_ok() {
                    commands.entity(*c).despawn();
                }
            }
            if let Some(model) = shown.and_then(|d| assets.weapon_world_models.get(defs.0[d].id)).and_then(|n| assets.models.get(n)) {
                commands.entity(e).with_children(|p| {
                    for (mesh, mat) in &model.parts {
                        p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), NotShadowCaster));
                    }
                });
            }
        }
    }
}

