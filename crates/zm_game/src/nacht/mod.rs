//! Nacht der Untoten, loaded from the user's install: a background task reads
//! the fastfiles and builds meshes/textures (see [`build`]), then the scene is
//! uploaded and spawned when a session starts.

pub mod build;
pub mod level;

use crate::menu::{CurrentMap, LoadingStatus, MapKind};
use crate::waw::Waw;
use crate::world::{Debris, Flicker, Mats};
use crate::{Dynamic, GameState, SessionEntity};
use bevy::pbr::{Lightmap, NotShadowCaster};
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
    pub world: Vec<(Handle<Mesh>, Handle<StandardMaterial>, bool)>,
    pub submodels: HashMap<usize, Vec<(Handle<Mesh>, Handle<StandardMaterial>)>>,
    pub models: HashMap<String, ModelParts>,
    pub static_models: Vec<(String, Transform)>,
    pub sky_model: Option<String>,
    pub lightmap: Option<Handle<Image>>,
    pub collision: Arc<TriMesh>,
    pub nav: Arc<NavGraph>,
    pub level: Level,
    pub scene_entities: Vec<waw_assets::mapents::Entity>,
    /// Brush submodels already used by gameplay (boards, doors) by number.
    pub gameplay_submodels: Vec<usize>,
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
        let e = commands.spawn((*local, ChildOf(parent_e))).id();
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
            .add_systems(Startup, start_load)
            .add_systems(OnEnter(GameState::Loading), start_load)
            .add_systems(Update, poll_load)
            .add_systems(Update, follow_sky);
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
    let wanted = build::Wanted {
        aliases: bank.map(|b| b.wanted_aliases()).unwrap_or_default(),
        weapon_ids: defs.0.iter().map(|d| d.id).collect(),
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

    let NachtScene {
        images: imgs, lightmap, materials: mdefs, world, submodels, models, static_models, sky_model, collision, entities, sounds, weapon_fire, map, characters, view_models: vms, zombie_anims, view_rig: vr, ..
    } = scene;
    let image_handles: HashMap<String, Handle<Image>> = imgs.into_iter().map(|(k, v)| (k, images.add(v))).collect();
    let lightmap = lightmap.map(|l| images.add(l));
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
                },
                unlit: m.unlit,
                double_sided: m.blend != Blend::Opaque,
                cull_mode: if m.blend == Blend::Opaque { Some(bevy::render::render_resource::Face::Back) } else { None },
                lightmap_exposure: 250.0,
                depth_bias: if m.blend == Blend::Blend { 2.0 } else { 0.0 },
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
    let world: Vec<_> = world.into_iter().map(|s| (meshes.add(s.mesh), mat_handles[s.material].clone(), mdefs[s.material].lightmapped)).collect();
    let submodels: HashMap<usize, Vec<_>> =
        submodels.into_iter().map(|(n, v)| (n, v.into_iter().map(|s| (meshes.add(s.mesh), mat_handles[s.material].clone())).collect())).collect();
    let models: HashMap<String, ModelParts> = models
        .into_iter()
        .map(|(n, m)| (n, ModelParts { parts: m.surfaces.into_iter().map(|s| (meshes.add(s.mesh), mat_handles[s.material].clone())).collect() }))
        .collect();
    let mut to_handles = |m: HashMap<String, Vec<Vec<u8>>>| -> HashMap<String, Vec<Handle<AudioSource>>> {
        m.into_iter().map(|(k, v)| (k, v.into_iter().map(|b| audio.add(AudioSource { bytes: b.into() })).collect())).collect()
    };
    zone_sounds.aliases = to_handles(sounds);
    zone_sounds.weapon_fire = to_handles(weapon_fire);
    info!("Zone sounds: {} aliases, {} weapon fire sounds", zone_sounds.aliases.len(), zone_sounds.weapon_fire.len());

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
        lightmap,
        collision: Arc::new(collision),
        nav: Arc::new(nav),
        level,
        scene_entities: entities,
        gameplay_submodels,
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
    ambient.brightness = 60.0;
    ambient.color = Color::srgb(0.55, 0.62, 0.85);
    clear.0 = Color::srgb(0.01, 0.012, 0.02);
    for (mesh, mat, lit) in &assets.world {
        let mut e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), SessionEntity));
        if let (true, Some(lm)) = (*lit, &assets.lightmap) {
            e.insert(Lightmap { image: lm.clone(), uv_rect: Rect::new(0.0, 0.0, 1.0, 1.0), bicubic_sampling: false });
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
    // Script models (barrels, props), except door blockers which are dynamic.
    for e in &assets.scene_entities {
        if e.classname() != "script_model" || e.targetname().starts_with("upstairs_blocker") {
            continue;
        }
        let Some(model) = e.get("model") else { continue };
        if let Some(parts) = assets.models.get(model) {
            let t = Transform::from_translation(to_bevy(e.origin())).with_rotation(angles_to_quat(e.angles()));
            spawn_model(&mut commands, parts, t, SessionEntity);
        }
    }
    // The map's own lights, as flickering point lights.
    for (i, e) in assets.scene_entities.iter().filter(|e| e.classname() == "light").enumerate() {
        let c = e.vec3("_color").unwrap_or([1.0, 0.75, 0.45]);
        let radius = e.f32("radius").unwrap_or(400.0) * build::INCH;
        let base = 90_000.0;
        commands.spawn((
            PointLight { color: Color::srgb(c[0], c[1], c[2]), intensity: base, range: radius.clamp(4.0, 14.0), shadows_enabled: false, ..default() },
            Transform::from_translation(to_bevy(e.origin())),
            Flicker { base, phase: i as f32 * 1.3 },
            SessionEntity,
        ));
    }
    // Moonlight.
    commands.spawn((
        DirectionalLight { color: Color::srgb(0.55, 0.7, 1.0), illuminance: 400.0, shadows_enabled: false, ..default() },
        Transform::from_xyz(-10.0, 30.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        SessionEntity,
    ));
    if let Some(parts) = assets.sky_model.as_ref().and_then(|n| assets.models.get(n)) {
        let e = spawn_model(&mut commands, parts, Transform::default(), (SessionEntity, SkyBox, NotShadowCaster));
        commands.entity(e).insert(NotShadowCaster);
    }
}

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
    crate::world::spawn_crate(&mut commands, level, &mats, &mut meshes);
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
