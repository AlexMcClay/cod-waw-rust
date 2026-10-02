//! Zombie spawning, AI and procedural animation.
//!
//! Life cycle: spawn outside -> walk to an assigned window -> tear boards
//! off -> vault through -> chase the player via the shared flow field ->
//! die (fall, sink, despawn).

use crate::audio::{PlayAlias, PlaySfx, Sfx};
use crate::player::{self, Health, Player};
use crate::world::{Board, Mats};
use crate::{Boards, Dynamic, GameState, LevelRes, Round, World};
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use zm_core::geom::V3;
use zm_core::rules::{self, Gait};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZState {
    Approach,
    AtWindow,
    Climbing,
    Chase,
    Dying,
}

#[derive(Component)]
pub struct Zombie {
    pub hp: f32,
    pub state: ZState,
    pub gait: Gait,
    pub speed: f32,
    pub window: usize,
    pub timer: f32,
    pub attack_cd: f32,
    /// >0 while an attack swing is in progress (time left in its anim).
    pub swing: f32,
    /// The melee or board-tearing anim being played (index into the
    /// rules' `attacks` / `tears`) and the time into it.
    pub act: usize,
    pub act_t: f32,
    pub groan_cd: f32,
    pub anim: f32,
    pub scale: f32,
    /// Approach point jitter along the window so they don't stack.
    pub slot: f32,
    /// Path node currently walked to (real maps only).
    pub nav_target: Option<usize>,
    pub nav_timer: f32,
    /// Ground height where a window vault started.
    pub climb_from: f32,
    /// Progress check: time since the last check and where it was.
    pub stuck_timer: f32,
    pub stuck_at: Vec3,
}

impl Zombie {
    pub fn alive(&self) -> bool {
        self.state != ZState::Dying
    }
}

#[derive(Component)]
pub struct Limb {
    pub arm: bool,
    pub side: f32,
}

/// A real zombie model and its animation state.
#[derive(Component)]
pub struct ZombieRig {
    joints: Vec<crate::nacht::Joint>,
    /// Which walk/attack variant this zombie uses.
    variant: usize,
    clip: usize,
    time: f32,
    map: Vec<Option<usize>>,
}

/// Spawns a real zombie model (skinned, from the install) under `root`.
fn spawn_real_model(commands: &mut Commands, root: Entity, models: &crate::nacht::ZombieModels) -> bool {
    if models.chars.is_empty() {
        return false;
    }
    let ch = &models.chars[fastrand::usize(..models.chars.len())];
    // Models face +X; our zombies face -Z.
    let model = commands.spawn((Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)), Visibility::default(), ChildOf(root))).id();
    let mut joints = Vec::new();
    crate::nacht::spawn_part(commands, &ch.body, &mut joints, model, model);
    if !ch.heads.is_empty() {
        crate::nacht::spawn_part(commands, &ch.heads[fastrand::usize(..ch.heads.len())], &mut joints, model, model);
    }
    commands.entity(root).insert(ZombieRig { joints, variant: fastrand::usize(..12), clip: usize::MAX, time: 0.0, map: Vec::new() });
    true
}

/// Developer aid: `UNDEAD_PREVIEW=<anim>` shows a zombie in front of the
/// player playing one animation in place, without AI.
#[derive(Component)]
pub struct Preview(usize, f32);

pub fn spawn_preview(
    mut commands: Commands,
    models: Option<Res<crate::nacht::ZombieModels>>,
    world: Res<World>,
    player: Query<(&Transform, &player::PlayerCtl), With<Player>>,
) {
    let Ok(name) = std::env::var("UNDEAD_PREVIEW") else { return };
    let (Some(models), Ok((p, ctl))) = (models, player.single()) else {
        warn!("preview: models or player missing");
        return;
    };
    let Some(clip) = models.clip(&name) else {
        warn!("preview: no animation {name}");
        return;
    };
    let flat = Vec3::new(-ctl.yaw.sin(), 0.0, -ctl.yaw.cos());
    let dist: f32 = std::env::var("UNDEAD_PREVIEW_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(1.5);
    let mut pos = ctl.feet(p) + flat * dist;
    pos.y = ground_y(&world, pos.x, pos.z, pos.y + 0.5);
    let root = commands
        .spawn((
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y((flat.x).atan2(flat.z))),
            Visibility::default(),
            Dynamic,
            Preview(clip, 0.0),
        ))
        .id();
    let ok = spawn_real_model(&mut commands, root, &models);
    info!("preview: {name} at {pos:?} (model spawned: {ok})");
}

fn animate_previews(
    time: Res<Time>,
    models: Option<Res<crate::nacht::ZombieModels>>,
    mut pq: Query<(&mut Preview, &mut ZombieRig)>,
    mut tq: Query<&mut Transform>,
) {
    let Some(models) = models else { return };
    for (mut pv, mut rig) in &mut pq {
        pv.1 += time.delta_secs();
        let clip = &models.clips[pv.0];
        if rig.clip != pv.0 {
            rig.clip = pv.0;
            rig.map = crate::nacht::track_map(clip, &rig.joints);
        }
        let frame = clip.frame_at(pv.1);
        let ZombieRig { joints, map, .. } = &*rig;
        crate::nacht::pose_mapped(clip, frame, joints, map, &mut tq, None);
    }
}

/// Picks the animation for a zombie's state and plays it on its joints.
fn animate_rigs(
    time: Res<Time>,
    models: Option<Res<crate::nacht::ZombieModels>>,
    mut zq: Query<(Entity, &Zombie, &mut ZombieRig)>,
    mut tq: Query<&mut Transform>,
    mut alias: EventWriter<PlayAlias>,
) {
    let Some(models) = models else { return };
    if models.clips.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let find = |names: &[&str], v: usize| -> Option<usize> {
        let opts: Vec<usize> = names.iter().filter_map(|n| models.clip(n)).collect();
        (!opts.is_empty()).then(|| opts[v % opts.len()])
    };
    let rules = rules::ZombieRules::nacht();
    for (entity, z, mut rig) in &mut zq {
        let v = rig.variant;
        // Tearing and attacking play the anim whose notetracks the AI is
        // timing, in step with it.
        let timed = |list: &[rules::TimedAnim]| list.get(z.act % list.len().max(1)).and_then(|a| models.clip(a.name));
        let mut sync = None;
        let (want, rate) = match z.state {
            ZState::Dying => (find(&["ai_zombie_death_v1", "ai_zombie_death_v2"], v), 1.3),
            ZState::Climbing => (find(&["ai_zombie_traverse_v1", "ai_zombie_traverse_v2"], v), 2.0),
            _ if z.swing > 0.0 => {
                sync = Some(z.act_t.max(0.0));
                (timed(rules.attacks), 1.0)
            }
            ZState::AtWindow => {
                sync = Some(z.act_t.max(0.0));
                (timed(rules.tears), 1.0)
            }
            _ => {
                let names: &[&str] = match z.gait {
                    Gait::Walk => &["ai_zombie_walk_v1", "ai_zombie_walk_v2", "ai_zombie_walk_v3", "ai_zombie_walk_v4"],
                    Gait::Run => &["ai_zombie_walk_fast_v1", "ai_zombie_walk_fast_v2", "ai_zombie_walk_fast_v3"],
                    Gait::Sprint => &["ai_zombie_sprint_v1", "ai_zombie_sprint_v2"],
                };
                let c = find(names, v);
                // Match the playback to the actual walking speed so feet don't slide.
                let rs = c.map(|c| models.clips[c].root_speed).unwrap_or(1.0).max(0.2);
                (c, (z.speed * z.scale.recip() / rs).clamp(0.4, 2.5))
            }
        };
        let Some(want) = want else { continue };
        if rig.clip != want {
            rig.clip = want;
            rig.time = 0.0;
            rig.map = crate::nacht::track_map(&models.clips[want], &rig.joints);
        }
        let clip = &models.clips[want];
        let before = if rig.time == 0.0 { -1.0 } else { clip.frame_at(rig.time) / clip.numframes.max(1.0) };
        // Timed anims follow the AI's clock (restarting for the next board
        // or swing); the others just play.
        match sync {
            Some(t) => rig.time = t,
            None => rig.time += dt * rate,
        }
        let frame = clip.frame_at(rig.time);
        // `sndnt#alias` notetracks crossed this frame play on the zombie.
        let now = frame / clip.numframes.max(1.0);
        for (note, t) in &clip.notify {
            let Some(name) = note.strip_prefix("sndnt#") else { continue };
            let crossed = if now >= before { *t > before && *t <= now } else { *t > before || *t <= now };
            if crossed {
                alias.write(PlayAlias::on(name, entity));
            }
        }
        let ZombieRig { joints, map, .. } = &*rig;
        crate::nacht::pose_mapped(clip, frame, joints, map, &mut tq, None);
    }
}

pub struct ZombiesPlugin;

impl Plugin for ZombiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (update_field, ai, separate, animate, animate_rigs).chain().run_if(in_state(GameState::Playing)),
        )
        .add_systems(Update, (dying, animate_previews))
        .add_systems(OnEnter(GameState::Playing), spawn_preview.run_if(|| std::env::var_os("UNDEAD_PREVIEW").is_some()));
    }
}

/// Ray vs zombie (head sphere + body box). Returns distance and headshot flag.
#[allow(dead_code)]
pub fn hit_test(origin: Vec3, dir: Vec3, t: &Transform, z: &Zombie, max: f32) -> Option<(f32, bool)> {
    hit_location(origin, dir, t, z, max).map(|(d, loc)| (d, loc.is_head()))
}

/// Ray vs zombie: distance and the engine hit location (head, neck, upper
/// or lower torso, arms, hands, legs, feet) from where on the body the ray
/// lands, so weapons can apply their per-location damage multipliers.
pub fn hit_location(origin: Vec3, dir: Vec3, t: &Transform, z: &Zombie, max: f32) -> Option<(f32, zm_core::weapons::HitLoc)> {
    use zm_core::geom::{ray_sphere, Aabb, V3};
    use zm_core::weapons::HitLoc;
    let s = z.scale;
    let p = t.translation;
    let o = V3::new(origin.x, origin.y, origin.z);
    let d = V3::new(dir.x, dir.y, dir.z);
    let head_c = V3::new(p.x, p.y + 1.63 * s, p.z);
    let head = ray_sphere(o, d, head_c, 0.19 * s).filter(|t| *t <= max);
    let body = Aabb::new(V3::new(p.x - 0.3, p.y, p.z - 0.3), V3::new(p.x + 0.3, p.y + 1.47 * s, p.z + 0.3)).ray_hit(o, d, max);
    let dist = match (head, body) {
        (Some(h), Some(b)) if b < h - 0.05 => b,
        (Some(h), _) => return Some((h, HitLoc::Head)),
        (None, Some(b)) => b,
        _ => return None,
    };
    // Where on the body: height above the feet (in units of this zombie's
    // size) and the side, in the zombie's own frame (it faces -Z, so +X is
    // its right).
    let local = t.rotation.inverse() * (origin + dir * dist - p);
    let h = local.y / s.max(0.01);
    let right = local.x >= 0.0;
    let side = |r: HitLoc, l: HitLoc| if right { r } else { l };
    let out = local.x.abs() > 0.19 * s;
    let loc = if h >= 1.38 {
        HitLoc::Neck
    } else if h >= 1.08 {
        if out { side(HitLoc::RightArmUpper, HitLoc::LeftArmUpper) } else { HitLoc::TorsoUpper }
    } else if h >= 0.86 {
        if out { side(HitLoc::RightArmLower, HitLoc::LeftArmLower) } else { HitLoc::TorsoLower }
    } else if h >= 0.72 && out {
        side(HitLoc::RightHand, HitLoc::LeftHand)
    } else if h >= 0.45 {
        side(HitLoc::RightLegUpper, HitLoc::LeftLegUpper)
    } else if h >= 0.1 {
        side(HitLoc::RightLegLower, HitLoc::LeftLegLower)
    } else {
        side(HitLoc::RightFoot, HitLoc::LeftFoot)
    };
    Some((dist, loc))
}

/// Apply damage; returns true if this killed the zombie.
pub fn apply_damage(z: &mut Zombie, amount: f32, insta: bool) -> bool {
    if !z.alive() {
        return false;
    }
    z.hp -= if insta { f32::INFINITY } else { amount };
    if z.hp <= 0.0 {
        z.state = ZState::Dying;
        z.timer = 0.0;
        true
    } else {
        false
    }
}

/// Build a zombie entity (root at the feet) and return it.
pub fn spawn_zombie(
    commands: &mut Commands,
    mats: &Mats,
    level: &LevelRes,
    world: &World,
    spec: rules::SpawnSpec,
    models: Option<&crate::nacht::ZombieModels>,
) -> Option<Entity> {
    let level = &level.0;
    let windows: Vec<usize> = level
        .windows
        .iter()
        .enumerate()
        .filter(|(_, w)| world.area_open(level, w.area))
        .map(|(i, _)| i)
        .collect();
    if windows.is_empty() {
        return None;
    }
    let (wi, pos) = if level.spawners.is_empty() {
        let wi = windows[fastrand::usize(..windows.len())];
        let w = &level.windows[wi];
        let (sx, sz) = w.spawn_point();
        let (tx, tz) = w.tangent();
        let jitter = (fastrand::f32() - 0.5) * 8.0;
        let back = fastrand::f32() * 4.0;
        (wi, Vec3::new(sx + tx * jitter + w.outward.0 * back, 0.0, sz + tz * jitter + w.outward.1 * back))
    } else {
        // The map's own spawners; each zombie heads for the closest open window.
        let open: Vec<&(zm_core::geom::V3, usize)> = level.spawners.iter().filter(|s| world.area_open(level, s.1)).collect();
        let sp = open.get(fastrand::usize(..open.len().max(1)))?.0;
        let p = Vec3::new(sp.x, sp.y, sp.z);
        let wi = *windows.iter().min_by(|a, b| {
            let da = outside_of(level, **a).distance(p);
            let db = outside_of(level, **b).distance(p);
            da.total_cmp(&db)
        })?;
        let jitter = Vec3::new(fastrand::f32() - 0.5, 0.0, fastrand::f32() - 0.5) * 2.0;
        let q = p + jitter;
        (wi, Vec3::new(q.x, ground_y(world, q.x, q.z, p.y + 1.0), q.z))
    };

    // The round's health and the gait rolled from `zombie_move_speed`.
    let gait = spec.gait;
    let scale = 0.92 + fastrand::f32() * 0.16;
    let skin = mats.skin[fastrand::usize(..mats.skin.len())].clone();
    let cloth = mats.cloth[fastrand::usize(..mats.cloth.len())].clone();
    let pants = mats.cloth[fastrand::usize(..mats.cloth.len())].clone();
    let cube = mats.cube.clone();

    let id = commands
        .spawn((
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
            Visibility::default(),
            Zombie {
                hp: spec.health,
                state: ZState::Approach,
                gait,
                // The gait's anims run at slightly different speeds.
                speed: spec.speed * (0.9 + fastrand::f32() * 0.2),
                window: wi,
                timer: 0.0,
                attack_cd: 0.0,
                swing: 0.0,
                act: 0,
                act_t: 0.0,
                groan_cd: 1.0 + fastrand::f32() * 4.0,
                anim: fastrand::f32() * 10.0,
                scale,
                slot: (fastrand::f32() - 0.5) * 0.5,
                nav_target: None,
                nav_timer: 0.0,
                climb_from: 0.0,
                stuck_timer: 0.0,
                stuck_at: pos,
            },
            Dynamic,
        ))
        .id();
    if models.is_some_and(|m| spawn_real_model(commands, id, m)) {
        return Some(id);
    }
    commands
        .entity(id)
        .with_children(|p| {
            let part = |p: &mut ChildSpawnerCommands, m: &Handle<StandardMaterial>, pos: Vec3, size: Vec3| {
                p.spawn((Mesh3d(cube.clone()), MeshMaterial3d(m.clone()), Transform::from_translation(pos).with_scale(size)));
            };
            // Torso: hunched forward a little.
            p.spawn((Transform::from_xyz(0.0, 0.85, 0.0).with_rotation(Quat::from_rotation_x(-0.18)), Visibility::default()))
                .with_children(|t| {
                    part(t, &cloth, Vec3::new(0.0, 0.32, 0.0), Vec3::new(0.48, 0.62, 0.26));
                    part(t, &skin, Vec3::new(0.0, 0.67, 0.0), Vec3::new(0.12, 0.1, 0.12));
                    // Head, slightly tilted.
                    t.spawn((Transform::from_xyz(0.0, 0.8, -0.02).with_rotation(Quat::from_rotation_z((fastrand::f32() - 0.5) * 0.5)), Visibility::default()))
                        .with_children(|h| {
                            part(h, &skin, Vec3::ZERO, Vec3::new(0.23, 0.27, 0.24));
                            part(h, &mats.blood, Vec3::new(0.0, -0.09, -0.115), Vec3::new(0.14, 0.05, 0.02));
                            for side in [-1.0f32, 1.0] {
                                h.spawn((
                                    Mesh3d(mats.sphere.clone()),
                                    MeshMaterial3d(mats.eye.clone()),
                                    Transform::from_xyz(side * 0.055, 0.03, -0.12).with_scale(Vec3::splat(0.045)),
                                    NotShadowCaster,
                                ));
                            }
                        });
                    // Arms pivot at the shoulders.
                    for side in [-1.0f32, 1.0] {
                        t.spawn((
                            Transform::from_xyz(side * 0.31, 0.58, 0.0),
                            Visibility::default(),
                            Limb { arm: true, side },
                        ))
                        .with_children(|a| {
                            part(a, &cloth, Vec3::new(0.0, -0.17, 0.0), Vec3::new(0.13, 0.36, 0.13));
                            part(a, &skin, Vec3::new(0.0, -0.47, 0.0), Vec3::new(0.11, 0.28, 0.11));
                        });
                    }
                });
            // Legs pivot at the hips.
            for side in [-1.0f32, 1.0] {
                p.spawn((Transform::from_xyz(side * 0.13, 0.86, 0.0), Visibility::default(), Limb { arm: false, side }))
                    .with_children(|l| {
                        part(l, &pants, Vec3::new(0.0, -0.42, 0.0), Vec3::new(0.17, 0.84, 0.18));
                        part(l, &mats.gun_metal, Vec3::new(0.0, -0.84, -0.05), Vec3::new(0.17, 0.06, 0.28));
                    });
            }
        });
    Some(id)
}

fn update_field(time: Res<Time>, mut world: ResMut<World>, player: Query<(&Transform, &player::PlayerCtl), With<Player>>) {
    world.field_timer -= time.delta_secs();
    if world.field_timer > 0.0 && !world.field.is_empty() {
        return;
    }
    world.field_timer = 0.25;
    let Ok((p, ctl)) = player.single() else { return };
    if let (Some(graph), Some(mesh)) = (world.graph.clone(), world.mesh.clone()) {
        let feet = ctl.feet(p);
        let here = V3::new(feet.x, feet.y, feet.z);
        let node = graph.nearest(here, |n| mesh.line_clear(V3::new(here.x, here.y + 0.8, here.z), V3::new(n.x, n.y + 0.8, n.z)));
        if let Some(node) = node {
            let open = world.door_open.clone();
            world.field = graph.field(node, &open);
        }
    } else {
        let f = world.nav.flow_field((p.translation.x, p.translation.z));
        world.field = f;
    }
}

/// Where zombies stand outside a window.
pub fn outside_of(level: &zm_core::level::Level, wi: usize) -> Vec3 {
    let w = &level.windows[wi];
    let (ox, oz) = w.outside_point();
    Vec3::new(ox, w.center.y, oz)
}

/// Ground height under a point (0 on the flat bunker).
pub fn ground_y(world: &World, x: f32, z: f32, from: f32) -> f32 {
    match &world.mesh {
        Some(m) => m.ground(x, z, from + 0.6, from - 4.0, 0.6).unwrap_or(from - 0.6),
        None => 0.0,
    }
}

/// Clear path for walking straight between two feet positions.
fn clear_walk(world: &World, a: Vec3, b: Vec3) -> bool {
    let Some(mesh) = &world.mesh else { return true };
    let up = |v: Vec3, h: f32| V3::new(v.x, v.y + h, v.z);
    let solids_ok = world.player_solids.iter().all(|s| {
        let o = up(a, 0.9);
        let d = up(b, 0.9).sub(o);
        let l = d.len();
        l < 1e-3 || s.ray_hit(o, d.scale(1.0 / l), l).is_none()
    });
    solids_ok && (b.y - a.y).abs() < 1.6 && mesh.line_clear(up(a, 0.5), up(b, 0.5)) && mesh.line_clear(up(a, 1.3), up(b, 1.3))
}

/// Next point to walk to on a real map: straight at `goal` when clear,
/// otherwise along `field` over the path nodes.
fn graph_step(world: &World, z: &mut Zombie, pos: Vec3, goal: Vec3, field: &[f32], dt: f32) -> Option<Vec3> {
    if clear_walk(world, pos, goal) {
        z.nav_target = None;
        return Some(goal);
    }
    let (graph, mesh) = (world.graph.as_ref()?, world.mesh.as_ref()?);
    z.nav_timer -= dt;
    let here = V3::new(pos.x, pos.y, pos.z);
    let reached = z.nav_target.is_some_and(|t| {
        let n = graph.nodes[t];
        Vec2::new(n.x - pos.x, n.z - pos.z).length() < 0.5
    });
    if z.nav_target.is_none() || reached || z.nav_timer <= 0.0 {
        z.nav_timer = 0.5;
        let cur = match (reached, z.nav_target) {
            (true, Some(t)) => Some(t),
            _ => graph.nearest(here, |n| mesh.line_clear(V3::new(here.x, here.y + 0.8, here.z), V3::new(n.x, n.y + 0.8, n.z))),
        };
        z.nav_target = cur.map(|c| {
            let far = graph.nodes[c].sub(here).len() > 0.8;
            if far && !reached {
                c
            } else {
                graph.next(c, field, &world.door_open).unwrap_or(c)
            }
        });
    }
    z.nav_target.map(|t| {
        let n = graph.nodes[t];
        Vec3::new(n.x, n.y, n.z)
    })
}

/// Keeps a zombie out of the real map's walls.
fn push_out(world: &World, t: &mut Transform) {
    if let Some(mesh) = &world.mesh {
        let p = t.translation;
        let (q, _) = mesh.push_sphere(V3::new(p.x, p.y + 0.8, p.z), 0.3, 2, |tri| tri.n.y.abs() < 0.7);
        t.translation.x = q.x;
        t.translation.z = q.z;
    }
}

fn face(t: &mut Transform, dir: Vec3, dt: f32, rate: f32) {
    if dir.length_squared() < 1e-6 {
        return;
    }
    // Model faces -Z.
    let target = Quat::from_rotation_y((-dir.x).atan2(-dir.z));
    t.rotation = t.rotation.slerp(target, (rate * dt).min(1.0));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn ai(
    time: Res<Time>,
    level: Res<LevelRes>,
    world: Res<World>,
    mut boards: ResMut<Boards>,
    mut health: ResMut<Health>,
    mut next: ResMut<NextState<GameState>>,
    player: Query<(&Transform, &player::PlayerCtl), (With<Player>, Without<Zombie>)>,
    mut zq: Query<(&mut Transform, &mut Zombie), Without<Player>>,
    board_q: Query<(Entity, &Board)>,
    mut sfx: EventWriter<PlaySfx>,
    (mut alias, zs, round): (EventWriter<PlayAlias>, Res<crate::audio::ZoneSounds>, Res<Round>),
    mut commands: Commands,
) {
    let dt = time.delta_secs().min(0.05);
    let rules = &round.0.rules;
    let Ok((pt, pctl)) = player.single() else { return };
    let ppos = Vec3::new(pt.translation.x, 0.0, pt.translation.z);
    let level = &level.0;

    for (mut t, mut z) in &mut zq {
        if !z.alive() {
            continue;
        }
        z.attack_cd = (z.attack_cd - dt).max(0.0);
        z.groan_cd -= dt;
        let pos = Vec3::new(t.translation.x, 0.0, t.translation.z);
        let to_player = ppos - pos;
        let dist_player = to_player.length();
        // With the game's sounds, vocals come from the animations.
        if z.groan_cd <= 0.0 && zs.aliases.is_empty() {
            z.groan_cd = 3.0 + fastrand::f32() * 5.0;
            let vol = (1.0 - dist_player / 22.0).clamp(0.0, 1.0) * 0.6;
            sfx.write(PlaySfx::at(Sfx::ZombieGroan, vol));
        }
        let w = &level.windows[z.window];
        let (tx, tz) = w.tangent();
        let (ox, oz) = w.outside_point();
        let outside = Vec3::new(ox + tx * z.slot, 0.0, oz + tz * z.slot);
        let (ix, iz) = w.inside_point();
        let inside = Vec3::new(ix, 0.0, iz);
        let floor_in = if world.mesh.is_some() { ground_y(&world, ix, iz, w.center.y) } else { 0.0 };

        // A melee anim in progress: every `fire` notetrack is a `melee()`
        // that hits the player if still within reach.
        if z.swing > 0.0 {
            let anim = rules.attacks[z.act % rules.attacks.len()];
            let before = z.act_t;
            z.act_t += dt;
            z.swing -= dt;
            let reach = if z.state == ZState::Chase { rules.melee_range } else { rules.melee_range + 0.4 };
            for &hit in anim.events {
                if hit > before && hit <= z.act_t && dist_player < reach {
                    if zs.aliases.is_empty() {
                        sfx.write(PlaySfx::at(Sfx::ZombieAttack, 0.8));
                    }
                    if player::damage_player(&mut health, rules.zombie_hit_damage, &mut alias) {
                        next.set(GameState::GameOver);
                    }
                }
            }
        }

        match z.state {
            ZState::Approach => {
                let d = outside - pos;
                if d.length() < 0.3 {
                    z.state = ZState::AtWindow;
                    // A moment to turn to the window, then the first pull.
                    z.act = fastrand::usize(..rules.tears.len().max(1));
                    z.act_t = -0.25;
                } else if world.mesh.is_some() {
                    let feet = t.translation;
                    let goal = Vec3::new(outside.x, feet.y, outside.z);
                    let field = world.window_fields.get(z.window).cloned().unwrap_or_default();
                    if let Some(target) = graph_step(&world, &mut z, feet, goal, &field, dt) {
                        let dir = Vec3::new(target.x - feet.x, 0.0, target.z - feet.z);
                        if dir.length() > 0.01 {
                            let step = dir.normalize() * z.speed.min(3.5) * dt;
                            t.translation += step.clamp_length_max(dir.length());
                            face(&mut t, dir, dt, 8.0);
                        }
                    }
                    push_out(&world, &mut t);
                    t.translation.y = ground_y(&world, t.translation.x, t.translation.z, t.translation.y);
                    // Taking far too long (lost in the field): appear at the window.
                    z.timer += dt;
                    if z.timer > 25.0 {
                        let y = ground_y(&world, outside.x, outside.z, w.center.y + 1.0);
                        t.translation = Vec3::new(outside.x, y, outside.z);
                        z.timer = 0.0;
                    }
                    // Stuck on scenery the path nodes don't cover: move closer.
                    z.stuck_timer += dt;
                    if z.stuck_timer > 3.0 {
                        let moved = Vec2::new(t.translation.x - z.stuck_at.x, t.translation.z - z.stuck_at.z).length();
                        if moved < 0.6 {
                            let back = 2.5 + fastrand::f32() * 3.0;
                            let p = Vec3::new(outside.x + w.outward.0 * back, 0.0, outside.z + w.outward.1 * back);
                            let y = ground_y(&world, p.x, p.z, w.center.y + 1.0);
                            t.translation = Vec3::new(p.x, y, p.z);
                            z.nav_target = None;
                        }
                        z.stuck_timer = 0.0;
                        z.stuck_at = t.translation;
                    }
                } else {
                    let step = d.normalize() * z.speed.min(3.5) * dt;
                    t.translation += step.min(d);
                    face(&mut t, d, dt, 8.0);
                }
            }
            ZState::AtWindow => {
                face(&mut t, inside - pos, dt, 8.0);
                // Nacht's zombies ignore the player until they are in
                // (Verrückt's hit through the window).
                if rules.attack_through_windows && inside.distance(ppos) < 1.2 && z.swing <= 0.0 {
                    start_attack(&mut z, rules);
                }
                // One board per tear anim, pulled off on its `board`
                // notetrack; in through the window once none are left.
                let tear = rules.tears[z.act % rules.tears.len()];
                let before = z.act_t;
                if z.swing <= 0.0 {
                    z.act_t += dt;
                }
                let pulled = tear.events.iter().any(|&b| b > before && b <= z.act_t);
                let n = boards.0[z.window];
                if (n == 0 && before <= 0.0 && z.act_t > 0.0) || (z.act_t >= tear.len && n == 0) {
                    z.state = ZState::Climbing;
                    z.timer = 0.0;
                } else if z.act_t >= tear.len {
                    z.act = fastrand::usize(..rules.tears.len());
                    z.act_t = 0.0;
                } else if pulled {
                    if n > 0 {
                        boards.0[z.window] = n - 1;
                        for (e, b) in &board_q {
                            if b.window == z.window && b.index == n - 1 {
                                commands.entity(e).try_despawn();
                            }
                        }
                        let vol = (1.0 - dist_player / 25.0).clamp(0.15, 1.0);
                        alias.write(PlayAlias::at("break_boards", crate::v3(w.center)).or(Sfx::BoardTear).volume(if zs.aliases.is_empty() { vol } else { 1.0 }));
                    }
                }
            }
            ZState::Climbing => {
                // A quick vault from the outside point to the inside point.
                if z.timer == 0.0 {
                    z.climb_from = t.translation.y;
                }
                z.timer += dt / 0.9;
                let k = z.timer.min(1.0);
                let p = outside.lerp(inside, k);
                let base = z.climb_from + (floor_in - z.climb_from) * k;
                t.translation = Vec3::new(p.x, base + (k * std::f32::consts::PI).sin() * 0.75, p.z);
                face(&mut t, inside - outside, dt, 10.0);
                if k >= 1.0 {
                    t.translation.y = floor_in;
                    z.state = ZState::Chase;
                }
            }
            ZState::Chase if world.mesh.is_some() => {
                let feet = t.translation;
                let player_feet = pctl.feet(pt);
                let level_with = (player_feet.y - feet.y).abs() < 1.5;
                // Melee from 64 units; the swings repeat while in reach.
                if dist_player < rules.melee_range && level_with && z.swing <= 0.0 {
                    start_attack(&mut z, rules);
                }
                if dist_player < 1.1 && level_with {
                    face(&mut t, to_player, dt, 10.0);
                } else if let Some(target) = graph_step(&world, &mut z, feet, player_feet, &world.field, dt) {
                    let d = Vec3::new(target.x - feet.x, 0.0, target.z - feet.z);
                    if d.length() > 0.01 {
                        let speed = if z.swing > 0.0 { z.speed * 0.4 } else { z.speed };
                        t.translation += d.normalize() * speed * dt;
                        face(&mut t, d, dt, 7.0);
                    }
                }
                push_out(&world, &mut t);
                let (mut x, mut zz) = (t.translation.x, t.translation.z);
                for s in world.player_solids.iter().filter(|s| s.min.y < feet.y + 1.6 && s.max.y > feet.y + 0.2) {
                    if let Some((nx, nz)) = s.push_circle(x, zz, 0.3) {
                        x = nx;
                        zz = nz;
                    }
                }
                t.translation.x = x;
                t.translation.z = zz;
                t.translation.y = ground_y(&world, x, zz, feet.y);
            }
            ZState::Chase => {
                let here = (pos.x, pos.z);
                let target = if world.nav.line_clear(here, (ppos.x, ppos.z)) {
                    Some(ppos)
                } else if !world.field.is_empty() {
                    world.nav.next_waypoint(&world.field, here).map(|(x, z)| Vec3::new(x, 0.0, z))
                } else {
                    None
                };
                if dist_player < rules.melee_range && z.swing <= 0.0 {
                    start_attack(&mut z, rules);
                }
                if dist_player < 1.1 {
                    face(&mut t, to_player, dt, 10.0);
                } else if let Some(target) = target {
                    let d = target - pos;
                    if d.length() > 0.01 {
                        let speed = if z.swing > 0.0 { z.speed * 0.4 } else { z.speed };
                        t.translation += d.normalize() * speed * dt;
                        face(&mut t, d, dt, 7.0);
                    }
                }
                // Keep out of walls.
                let (mut x, mut zz) = (t.translation.x, t.translation.z);
                for s in &world.player_solids {
                    if let Some((nx, nz)) = s.push_circle(x, zz, 0.3) {
                        x = nx;
                        zz = nz;
                    }
                }
                t.translation.x = x;
                t.translation.z = zz;
            }
            ZState::Dying => {}
        }
        z.anim += dt * z.speed * if matches!(z.state, ZState::AtWindow) { 0.4 } else { 2.2 };
    }
}

/// Starts a melee anim, picked at random like `pick_zombie_melee_anim`.
fn start_attack(z: &mut Zombie, rules: &rules::ZombieRules) {
    if rules.attacks.is_empty() {
        return;
    }
    z.act = fastrand::usize(..rules.attacks.len());
    z.act_t = 0.0;
    z.swing = rules.attacks[z.act].len;
}

/// Push zombies apart so they crowd instead of stacking.
fn separate(mut zq: Query<(Entity, &mut Transform, &Zombie)>) {
    let pts: Vec<(Entity, Vec2, ZState)> =
        zq.iter().map(|(e, t, z)| (e, Vec2::new(t.translation.x, t.translation.z), z.state)).collect();
    for (e, mut t, z) in &mut zq {
        if !matches!(z.state, ZState::Chase | ZState::Approach) {
            continue;
        }
        let me = Vec2::new(t.translation.x, t.translation.z);
        let mut push = Vec2::ZERO;
        for (oe, op, os) in &pts {
            if *oe == e || *os == ZState::Dying {
                continue;
            }
            let d = me - *op;
            let l = d.length();
            if l < 0.6 && l > 1e-4 {
                push += d / l * (0.6 - l) * 0.5;
            }
        }
        t.translation.x += push.x;
        t.translation.z += push.y;
    }
}

#[allow(clippy::type_complexity)]
fn animate(zq: Query<(&Zombie, &Children)>, children: Query<&Children>, mut limbs: Query<(&mut Transform, &Limb)>) {
    for (z, kids) in &zq {
        if !z.alive() {
            continue;
        }
        let swing_amt = match z.gait {
            Gait::Walk => 0.45,
            Gait::Run => 0.8,
            Gait::Sprint => 1.0,
        };
        let walk = z.anim.sin() * swing_amt;
        let attacking = z.swing > 0.0;
        // Limbs live under the root (legs) or under the torso (arms).
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(e) = stack.pop() {
            if let Ok((mut lt, limb)) = limbs.get_mut(e) {
                lt.rotation = if limb.arm {
                    let reach = if z.state == ZState::AtWindow || attacking { -1.9 } else { -1.45 };
                    let flail = if attacking { (z.swing * 25.0).sin() * 0.6 } else { walk * 0.25 * limb.side };
                    Quat::from_euler(EulerRot::XYZ, reach + flail, 0.0, limb.side * -0.08)
                } else {
                    Quat::from_rotation_x(walk * limb.side)
                };
            } else if let Ok(c) = children.get(e) {
                stack.extend(c.iter());
            }
        }
    }
}

fn dying(
    time: Res<Time>,
    mut commands: Commands,
    mut zq: Query<(Entity, &mut Transform, &mut Zombie, Has<ZombieRig>)>,
) {
    let dt = time.delta_secs();
    for (e, mut t, mut z, rigged) in &mut zq {
        if z.alive() {
            continue;
        }
        z.timer += dt;
        if z.timer < 0.5 && !rigged {
            // Topple backwards.
            let fall = Quat::from_rotation_x(dt / 0.5 * 1.45);
            t.rotation *= fall;
            t.translation.y = t.translation.y.max(0.0) * 0.9;
        } else if z.timer > 2.5 {
            t.translation.y -= dt * 0.6;
        }
        if z.timer > 4.0 {
            commands.entity(e).try_despawn();
        }
    }
}

/// Used by the round system when a nuke goes off etc.
pub fn kill_all<F: bevy::ecs::query::QueryFilter>(zq: &mut Query<(Entity, &mut Transform, &mut Zombie), F>) -> Vec<Vec3> {
    let mut out = Vec::new();
    for (_, t, mut z) in zq.iter_mut() {
        if z.alive() {
            z.state = ZState::Dying;
            z.timer = 0.0;
            out.push(t.translation);
        }
    }
    out
}
