//! Level geometry, lighting and the parts of the map that change during play
//! (window boards, debris piles, wall-buy outlines, the mystery crate).

use crate::waw::{Waw, WawImages};
use crate::{v3, Boards, Defs, Dynamic, LevelRes, SessionEntity};
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;
use bevy::render::renderer::RenderDevice;
use std::collections::HashMap;
use zm_core::level::{LINTEL, SILL, WALL_H};
use zm_core::weapons;

/// Shared meshes/materials so spawning at runtime is cheap.
#[derive(Resource)]
pub struct Mats {
    pub board_mesh: Handle<Mesh>,
    pub board_mat: Handle<StandardMaterial>,
    pub cube: Handle<Mesh>,
    pub sphere: Handle<Mesh>,
    pub skin: Vec<Handle<StandardMaterial>>,
    pub cloth: Vec<Handle<StandardMaterial>>,
    pub eye: Handle<StandardMaterial>,
    pub blood: Handle<StandardMaterial>,
    pub spark: Handle<StandardMaterial>,
    pub chalk: Handle<StandardMaterial>,
    pub rubble: Handle<StandardMaterial>,
    pub wood: Handle<StandardMaterial>,
    pub gun_metal: Handle<StandardMaterial>,
    pub gun_wood: Handle<StandardMaterial>,
    pub glow_green: Handle<StandardMaterial>,
    pub glow_gold: Handle<StandardMaterial>,
    pub glow_red: Handle<StandardMaterial>,
    pub glow_blue: Handle<StandardMaterial>,
    pub beam: Handle<StandardMaterial>,
    // Bunker surfaces (textured from the install when available).
    pub wall: Handle<StandardMaterial>,
    pub trim: Handle<StandardMaterial>,
    pub floor: Handle<StandardMaterial>,
    pub ceiling: Handle<StandardMaterial>,
    pub ground: Handle<StandardMaterial>,
    pub bark: Handle<StandardMaterial>,
    pub crate_body: Handle<StandardMaterial>,
    /// Chalk outline material per weapon id.
    pub chalk_by_weapon: HashMap<String, Handle<StandardMaterial>>,
    pub chalk_quad: Handle<Mesh>,
    /// Real brush submodels (window boards...) when a map from the install is loaded.
    pub submodels: HashMap<usize, Vec<(Handle<Mesh>, Handle<crate::nacht::model_material::ModelMaterial>)>>,
}

/// Texture tile size in metres for world-space UVs.
const TILE: f32 = 2.5;

/// A box whose UVs follow world space, so textures tile evenly whatever its size.
pub fn tiled_box(size: Vec3, origin: Vec3, tile: f32) -> Mesh {
    let mut mesh = Mesh::from(Cuboid::new(size.x, size.y, size.z));
    let (Some(VertexAttributeValues::Float32x3(pos)), Some(VertexAttributeValues::Float32x3(nor))) =
        (mesh.attribute(Mesh::ATTRIBUTE_POSITION), mesh.attribute(Mesh::ATTRIBUTE_NORMAL))
    else {
        return mesh;
    };
    let uvs: Vec<[f32; 2]> = pos
        .iter()
        .zip(nor)
        .map(|(p, n)| {
            let w = Vec3::from(*p) + origin;
            let (u, v) = if n[0].abs() > 0.5 {
                (w.z * n[0].signum(), w.y)
            } else if n[1].abs() > 0.5 {
                (w.x, w.z)
            } else {
                (-w.x * n[2].signum(), w.y)
            };
            [u / tile, -v / tile]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

#[derive(Component)]
pub struct Board {
    pub window: usize,
    pub index: u8,
}

#[derive(Component)]
pub struct Debris {
    pub door: usize,
}

#[derive(Component)]
pub struct CrateLid;

/// The weapon model that floats out of the crate.
#[derive(Component)]
pub struct CrateDisplay;

#[derive(Component)]
pub struct Flicker {
    pub base: f32,
    pub phase: f32,
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, make_mats).add_systems(Update, flicker);
    }
}

fn mat(mats: &mut Assets<StandardMaterial>, c: Color, rough: f32) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial { base_color: c, perceptual_roughness: rough, ..default() })
}

fn glow(mats: &mut Assets<StandardMaterial>, c: Color, strength: f32) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial {
        base_color: c,
        emissive: LinearRgba::from(c) * strength,
        ..default()
    })
}

/// `surface = image` lines from `textures.cfg`.
fn texture_config() -> HashMap<String, String> {
    crate::settings::find_config("textures.cfg")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| crate::settings::parse_kv(&t))
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn make_mats(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    waw: Res<Waw>,
    mut wimg: ResMut<WawImages>,
    mut images: ResMut<Assets<Image>>,
    device: Option<Res<RenderDevice>>,
) {
    let cfg = texture_config();
    let mut tex = |key: &str| cfg.get(key).and_then(|name| wimg.get(&waw, &mut images, device.as_deref(), name, true, true));
    // Textured surface, or a flat colour without the install.
    let mut surface = |key: &str, c: Color, rough: f32| match tex(key) {
        Some(img) => mats.add(StandardMaterial { base_color_texture: Some(img), perceptual_roughness: rough, ..default() }),
        None => mats.add(StandardMaterial { base_color: c, perceptual_roughness: rough, ..default() }),
    };
    let wall = surface("wall", Color::srgb(0.42, 0.40, 0.36), 0.9);
    let trim = surface("trim", Color::srgb(0.25, 0.22, 0.18), 0.9);
    let floor = surface("floor", Color::srgb(0.30, 0.29, 0.27), 0.95);
    let ceiling = surface("ceiling", Color::srgb(0.20, 0.19, 0.18), 1.0);
    let ground = surface("ground", Color::srgb(0.13, 0.12, 0.09), 1.0);
    let bark = surface("bark", Color::srgb(0.10, 0.08, 0.07), 1.0);
    let board_mat = surface("board", Color::srgb(0.36, 0.25, 0.15), 0.95);
    let rubble = surface("rubble", Color::srgb(0.32, 0.30, 0.28), 1.0);
    let crate_body = surface("crate", Color::srgb(0.32, 0.20, 0.10), 0.9);
    let mut chalk_by_weapon = HashMap::new();
    for (k, name) in &cfg {
        if let Some(id) = k.strip_prefix("chalk.") {
            if let Some(img) = wimg.get(&waw, &mut images, device.as_deref(), name, true, false) {
                let h = mats.add(StandardMaterial {
                    base_color_texture: Some(img.clone()),
                    emissive: LinearRgba::rgb(0.5, 0.5, 0.48),
                    emissive_texture: Some(img),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                });
                chalk_by_weapon.insert(id.to_string(), h);
            }
        }
    }
    let m = &mut *mats;
    let res = Mats {
        board_mesh: meshes.add(tiled_box(Vec3::new(1.9, 0.17, 0.05), Vec3::ZERO, 1.9)),
        board_mat,
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        sphere: meshes.add(Sphere::new(0.5).mesh().ico(2).unwrap()),
        skin: vec![
            mat(m, Color::srgb(0.47, 0.52, 0.42), 0.8),
            mat(m, Color::srgb(0.55, 0.53, 0.45), 0.8),
            mat(m, Color::srgb(0.40, 0.44, 0.40), 0.8),
        ],
        cloth: vec![
            mat(m, Color::srgb(0.20, 0.21, 0.19), 0.95),
            mat(m, Color::srgb(0.27, 0.24, 0.18), 0.95),
            mat(m, Color::srgb(0.17, 0.18, 0.22), 0.95),
            mat(m, Color::srgb(0.30, 0.28, 0.25), 0.95),
        ],
        eye: glow(m, Color::srgb(1.0, 0.45, 0.05), 12.0),
        blood: mat(m, Color::srgb(0.35, 0.02, 0.02), 0.4),
        spark: glow(m, Color::srgb(1.0, 0.8, 0.4), 20.0),
        chalk: glow(m, Color::srgb(0.85, 0.85, 0.8), 0.6),
        rubble,
        wood: mat(m, Color::srgb(0.32, 0.20, 0.10), 0.9),
        gun_metal: mat(m, Color::srgb(0.12, 0.12, 0.13), 0.4),
        gun_wood: mat(m, Color::srgb(0.35, 0.18, 0.08), 0.7),
        glow_green: glow(m, Color::srgb(0.3, 1.0, 0.35), 6.0),
        glow_gold: glow(m, Color::srgb(1.0, 0.8, 0.2), 6.0),
        glow_red: glow(m, Color::srgb(1.0, 0.2, 0.15), 6.0),
        glow_blue: glow(m, Color::srgb(0.3, 0.6, 1.0), 6.0),
        beam: m.add(StandardMaterial {
            base_color: Color::srgba(0.5, 0.8, 1.0, 0.18),
            emissive: LinearRgba::rgb(0.6, 1.2, 2.0),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        }),
        wall,
        trim,
        floor,
        ceiling,
        ground,
        bark,
        crate_body,
        chalk_by_weapon,
        chalk_quad: meshes.add(Rectangle::new(1.0, 1.0)),
        submodels: HashMap::new(),
    };
    commands.insert_resource(res);
}

pub fn spawn_static(
    mut commands: Commands,
    level: Res<LevelRes>,
    surf: Res<Mats>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let level = &level.0;
    let wall_mat = surf.wall.clone();
    let trim_mat = surf.trim.clone();
    let floor_mat = surf.floor.clone();
    let ceil_mat = surf.ceiling.clone();
    let dirt_mat = surf.ground.clone();
    let bark_mat = surf.bark.clone();
    let bulb_mat = glow(&mut mats, Color::srgb(1.0, 0.8, 0.5), 8.0);
    let mut ground = Plane3d::default().mesh().size(140.0, 140.0).build();
    if let Some(VertexAttributeValues::Float32x2(uv)) = ground.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        uv.iter_mut().for_each(|t| *t = [t[0] * 140.0 / 4.0, t[1] * 140.0 / 4.0]);
    }

    // Outdoor ground and a ring of dead trees.
    commands.spawn((
        SessionEntity,
        Mesh3d(meshes.add(ground)),
        MeshMaterial3d(dirt_mat),
        Transform::from_xyz(5.0, -0.01, 4.0),
    ));
    let trunk = meshes.add(Cylinder::new(0.25, 7.0));
    let branch = meshes.add(Cylinder::new(0.08, 2.5));
    fastrand::seed(7);
    for i in 0..40 {
        let a = i as f32 / 40.0 * std::f32::consts::TAU + fastrand::f32() * 0.1;
        let r = 26.0 + fastrand::f32() * 14.0;
        let pos = Vec3::new(5.0 + a.cos() * r, 3.5, 4.0 + a.sin() * r);
        commands
            .spawn((
                SessionEntity,
                Mesh3d(trunk.clone()),
                MeshMaterial3d(bark_mat.clone()),
                Transform::from_translation(pos).with_rotation(Quat::from_rotation_z((fastrand::f32() - 0.5) * 0.2)),
            ))
            .with_children(|p| {
                for b in 0..3 {
                    p.spawn((
                        Mesh3d(branch.clone()),
                        MeshMaterial3d(bark_mat.clone()),
                        Transform::from_xyz(0.0, 1.0 + b as f32 * 0.9, 0.0)
                            .with_rotation(Quat::from_euler(EulerRot::YXZ, fastrand::f32() * std::f32::consts::TAU, 0.0, 0.9))
                            .with_translation(Vec3::new(0.0, 1.0 + b as f32 * 0.9, 0.0)),
                    ));
                }
            });
    }
    fastrand::seed(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1));

    // Interior floors and ceilings per area.
    for a in &level.areas {
        let size = a.size();
        let c = a.center();
        commands.spawn((
        SessionEntity,
            Mesh3d(meshes.add(tiled_box(Vec3::new(size.x, 0.1, size.z), Vec3::new(c.x, -0.05, c.z), TILE))),
            MeshMaterial3d(floor_mat.clone()),
            Transform::from_xyz(c.x, -0.05, c.z),
        ));
        commands.spawn((
        SessionEntity,
            Mesh3d(meshes.add(tiled_box(Vec3::new(size.x + 0.4, 0.3, size.z + 0.4), Vec3::new(c.x, WALL_H + 0.15, c.z), TILE))),
            MeshMaterial3d(ceil_mat.clone()),
            Transform::from_xyz(c.x, WALL_H + 0.15, c.z),
        ));
        // Ceiling beams.
        let n = (size.x / 3.0) as i32;
        for i in 1..n.max(1) {
            commands.spawn((
        SessionEntity,
                Mesh3d(meshes.add(tiled_box(Vec3::new(0.25, 0.3, size.z), Vec3::ZERO, 1.0))),
                MeshMaterial3d(trim_mat.clone()),
                Transform::from_xyz(a.min.x + i as f32 * size.x / n as f32, WALL_H - 0.15, c.z),
                NotShadowCaster,
            ));
        }
    }

    for w in &level.walls {
        let s = w.size();
        commands.spawn((
        SessionEntity,
            Mesh3d(meshes.add(tiled_box(Vec3::new(s.x, s.y, s.z), v3(w.center()), TILE))),
            MeshMaterial3d(wall_mat.clone()),
            Transform::from_translation(v3(w.center())),
        ));
    }

    // Window frames.
    for w in &level.windows {
        let t = w.tangent();
        let tan = Vec3::new(t.0, 0.0, t.1);
        let c = v3(w.center);
        let rot = Quat::from_rotation_y(t.1.atan2(t.0) * -1.0);
        for side in [-1.0f32, 1.0] {
            commands.spawn((
        SessionEntity,
                Mesh3d(meshes.add(Cuboid::new(0.12, LINTEL - SILL, 0.5))),
                MeshMaterial3d(trim_mat.clone()),
                Transform::from_translation(c + tan * side * 0.86).with_rotation(rot),
            ));
        }
    }

    // Lights.
    let bulb = meshes.add(Sphere::new(0.12));
    for (i, l) in level.lights.iter().enumerate() {
        commands
            .spawn((
                SessionEntity,
                PointLight {
                    color: Color::srgb(1.0, 0.78, 0.5),
                    intensity: 350_000.0,
                    range: 16.0,
                    shadows_enabled: i < 2,
                    ..default()
                },
                Transform::from_translation(v3(*l)),
                Flicker { base: 350_000.0, phase: i as f32 * 1.7 },
            ))
            .with_children(|p| {
                p.spawn((Mesh3d(bulb.clone()), MeshMaterial3d(bulb_mat.clone()), NotShadowCaster));
            });
    }
    commands.spawn((
        SessionEntity,
        DirectionalLight {
            color: Color::srgb(0.55, 0.65, 0.9),
            illuminance: 1200.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 30.0, -10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Spawn everything that resets on restart.
pub fn spawn_dynamic(
    mut commands: Commands,
    level: Res<LevelRes>,
    mats: Res<Mats>,
    boards: Res<Boards>,
    defs: Res<Defs>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let level = &level.0;
    for (wi, _) in level.windows.iter().enumerate() {
        for b in 0..boards.0[wi] {
            spawn_board(&mut commands, level, &mats, wi, b);
        }
    }

    // Debris piles in closed doorways.
    for (di, d) in level.doors.iter().enumerate() {
        let c = v3(d.blocker.center());
        let s = v3(d.blocker.size());
        let long_x = s.x > s.z;
        for k in 0..9 {
            let along = (k as f32 / 8.0 - 0.5) * if long_x { s.x } else { s.z } * 0.9;
            let size = Vec3::new(0.5 + fastrand::f32() * 0.7, 0.4 + fastrand::f32() * 0.9, 0.5 + fastrand::f32() * 0.6);
            let off = if long_x { Vec3::new(along, 0.0, (fastrand::f32() - 0.5) * 0.8) } else { Vec3::new((fastrand::f32() - 0.5) * 0.8, 0.0, along) };
            let y = size.y / 2.0 + if k % 3 == 0 { 1.0 } else { 0.0 };
            commands.spawn((
                Mesh3d(mats.cube.clone()),
                MeshMaterial3d(if k % 2 == 0 { mats.rubble.clone() } else { mats.wood.clone() }),
                Transform::from_translation(Vec3::new(c.x, 0.0, c.z) + off + Vec3::Y * y)
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, fastrand::f32() * 0.4, fastrand::f32() * 3.0, fastrand::f32() * 0.4))
                    .with_scale(size),
                Debris { door: di },
                Dynamic,
            ));
        }
        // A few planks nailed across the top of the pile.
        for k in 0..3 {
            let rot = if long_x { Quat::from_rotation_z(0.3 * (k as f32 - 1.0)) } else { Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_z(0.3 * (k as f32 - 1.0)) };
            commands.spawn((
                Mesh3d(mats.cube.clone()),
                MeshMaterial3d(mats.board_mat.clone()),
                Transform::from_translation(Vec3::new(c.x, 1.4 + k as f32 * 0.35, c.z)).with_rotation(rot).with_scale(Vec3::new(3.0, 0.18, 0.06)),
                Debris { door: di },
                Dynamic,
            ));
        }
    }

    // Wall-buy chalk outlines: a simple gun silhouette in emissive strokes.
    for wb in &level.wall_buys {
        let def_idx = weapons::find(&defs.0, &wb.weapon_id).unwrap_or(0);
        let len = match defs.0[def_idx].kind {
            weapons::Kind::Pistol => 0.45,
            weapons::Kind::Smg => 0.9,
            weapons::Kind::Shotgun => 1.1,
            _ => 1.3,
        };
        let yaw = (-wb.facing.0).atan2(-wb.facing.1);
        let base = Transform::from_translation(v3(wb.pos)).with_rotation(Quat::from_rotation_y(yaw));
        if let Some(chalk) = mats.chalk_by_weapon.get(wb.weapon_id.as_str()) {
            // The game's own chalk drawing on a quad flat against the wall.
            commands.spawn((
                Mesh3d(mats.chalk_quad.clone()),
                MeshMaterial3d(chalk.clone()),
                Transform { rotation: base.rotation * Quat::from_rotation_y(std::f32::consts::PI), ..base }.with_scale(Vec3::new(len * 1.3, len * 0.45, 1.0)),
                NotShadowCaster,
                Dynamic,
            ));
            continue;
        }
        commands
            .spawn((base, Visibility::default(), Dynamic))
            .with_children(|p| {
                let stroke = |p: &mut ChildSpawnerCommands, pos: Vec3, scale: Vec3, rz: f32| {
                    p.spawn((
                        Mesh3d(mats.cube.clone()),
                        MeshMaterial3d(mats.chalk.clone()),
                        Transform::from_translation(pos).with_rotation(Quat::from_rotation_z(rz)).with_scale(scale),
                        NotShadowCaster,
                    ));
                };
                stroke(p, Vec3::new(0.0, 0.05, 0.0), Vec3::new(len, 0.03, 0.01), 0.0);
                stroke(p, Vec3::new(0.0, -0.08, 0.0), Vec3::new(len * 0.6, 0.03, 0.01), 0.0);
                stroke(p, Vec3::new(-len * 0.45, -0.12, 0.0), Vec3::new(0.25, 0.03, 0.01), 1.0);
                stroke(p, Vec3::new(len * 0.05, -0.18, 0.0), Vec3::new(0.18, 0.03, 0.01), 1.4);
                stroke(p, Vec3::new(len * 0.5, 0.0, 0.0), Vec3::new(0.03, 0.12, 0.01), 0.0);
            });
    }

    spawn_crate(&mut commands, level, &mats, &mut meshes);
}

/// The mystery crate: body, glowing seam, lid, beam and the weapon display.
pub fn spawn_crate(commands: &mut Commands, level: &zm_core::level::Level, mats: &Mats, meshes: &mut Assets<Mesh>) {
    let cb = level.crate_box;
    let c = v3(cb.center());
    let s = v3(cb.size());
    commands
        .spawn((Transform::from_translation(Vec3::new(c.x, cb.min.y, c.z)), Visibility::default(), Dynamic))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(s.x, s.y * 0.85, s.z))),
                MeshMaterial3d(mats.crate_body.clone()),
                Transform::from_xyz(0.0, s.y * 0.425, 0.0),
            ));
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(s.x + 0.04, 0.03, s.z + 0.04))),
                MeshMaterial3d(mats.glow_blue.clone()),
                Transform::from_xyz(0.0, s.y * 0.85, 0.0),
                NotShadowCaster,
            ));
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(s.x, s.y * 0.15, s.z))),
                MeshMaterial3d(mats.crate_body.clone()),
                Transform::from_xyz(0.0, s.y * 0.925, 0.0),
                CrateLid,
            ));
            p.spawn((
                Mesh3d(meshes.add(Cylinder::new(0.35, 30.0))),
                MeshMaterial3d(mats.beam.clone()),
                Transform::from_xyz(0.0, 15.0, 0.0),
                NotShadowCaster,
            ));
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.9, 0.12, 0.08))),
                MeshMaterial3d(mats.glow_gold.clone()),
                Transform::from_xyz(0.0, 1.2, 0.0),
                Visibility::Hidden,
                CrateDisplay,
                NotShadowCaster,
            ));
            p.spawn((
                PointLight { color: Color::srgb(0.5, 0.7, 1.0), intensity: 60_000.0, range: 5.0, ..default() },
                Transform::from_xyz(0.0, 1.5, 0.0),
            ));
        });
}

pub fn board_transform(level: &zm_core::level::Level, window: usize, index: u8) -> Transform {
    let w = &level.windows[window];
    let t = w.tangent();
    let yaw = -(t.1.atan2(t.0));
    let y = SILL + 0.18 + index as f32 * ((LINTEL - SILL - 0.3) / 5.0);
    let tilt = if index % 2 == 0 { 0.12 } else { -0.1 } * (1.0 + (index as f32 * 1.3).sin() * 0.5);
    // Boards sit on the outside face of the wall.
    let pos = Vec3::new(w.center.x + w.outward.0 * 0.26, y, w.center.z + w.outward.1 * 0.26);
    Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_z(tilt))
}

pub fn spawn_board(commands: &mut Commands, level: &zm_core::level::Level, mats: &Mats, window: usize, index: u8) {
    // Real board model from the map, when there is one.
    if let Some((n, o)) = level.windows[window].board_models.get(index as usize) {
        if let Some(parts) = mats.submodels.get(n) {
            commands
                .spawn((Transform::from_translation(v3(*o)), Visibility::default(), Board { window, index }, Dynamic))
                .with_children(|p| {
                    for (mesh, mat) in parts {
                        p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
                    }
                });
            return;
        }
    }
    commands.spawn((
        Mesh3d(mats.board_mesh.clone()),
        MeshMaterial3d(mats.board_mat.clone()),
        board_transform(level, window, index),
        Board { window, index },
        Dynamic,
    ));
}

fn flicker(time: Res<Time>, mut q: Query<(&mut PointLight, &Flicker)>) {
    let t = time.elapsed_secs();
    for (mut l, f) in &mut q {
        let n = (t * 7.3 + f.phase).sin() * (t * 3.1 + f.phase * 2.0).sin();
        let drop = if n > 0.93 { 0.25 } else { 1.0 };
        l.intensity = f.base * (0.9 + 0.1 * (t * 13.0 + f.phase).sin()) * drop;
    }
}
