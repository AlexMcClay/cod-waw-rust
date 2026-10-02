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
use zm_core::geom::{Aabb, V3};
use zm_core::navgraph::{self, NavGraph};
use zm_core::trimesh::TriMesh;
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
    /// Route following, collision and ground height (real maps only).
    pub mover: Mover,
    /// Ground height where a window vault started.
    pub climb_from: f32,
    /// Which window-vault animation this zombie uses.
    pub climb_anim: usize,
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
    rate: f32,
    map: Vec<Option<usize>>,
    /// The clip being faded out after a change, blended into the new one
    /// (as the game blends between animations).
    prev: Option<FadingClip>,
}

struct FadingClip {
    clip: usize,
    time: f32,
    rate: f32,
    map: Vec<Option<usize>>,
    age: f32,
}

/// Seconds to blend from one zombie animation into the next.
const BLEND_TIME: f32 = 0.2;

/// The hit boxes of a real zombie model: each bone's box from the model,
/// on the joint that carries it (tested in the joint's animated pose).
#[derive(Component, Default)]
pub struct Hitboxes(pub Vec<(Entity, crate::nacht::build::BoneHit)>);

impl Hitboxes {
    fn add(&mut self, part: &crate::nacht::PartAssets, joints: &[crate::nacht::Joint]) {
        for ((name, ..), hit) in part.bones.iter().zip(&part.hits) {
            let (Some(hit), Some(j)) = (hit, joints.iter().find(|j| j.0 == *name)) else { continue };
            self.0.push((j.1, *hit));
        }
    }
}

/// Ray vs a real zombie's bone boxes, as the game traces bullets: the
/// nearest box hit and its bone's hit location.
pub fn hit_location_boxes(origin: Vec3, dir: Vec3, boxes: &Hitboxes, globals: &Query<&GlobalTransform>, max: f32) -> Option<(f32, zm_core::weapons::HitLoc)> {
    use zm_core::weapons::HitLoc;
    let mut best: Option<(f32, HitLoc)> = None;
    for (joint, b) in &boxes.0 {
        let Ok(g) = globals.get(*joint) else { continue };
        let Some(loc) = HitLoc::ALL.get(b.loc as usize).copied().filter(|l| *l != HitLoc::None && *l != HitLoc::Gun) else { continue };
        // Into the bone's space; the ray parameter stays in world units.
        let inv = g.affine().inverse();
        let (o, d) = (inv.transform_point3(origin), inv.transform_vector3(dir));
        let (mut t0, mut t1) = (0.0f32, best.map_or(max, |b| b.0));
        let mut hit = true;
        for k in 0..3 {
            if d[k].abs() < 1e-8 {
                if o[k] < b.min[k] || o[k] > b.max[k] {
                    hit = false;
                    break;
                }
                continue;
            }
            let (a, c) = ((b.min[k] - o[k]) / d[k], (b.max[k] - o[k]) / d[k]);
            t0 = t0.max(a.min(c));
            t1 = t1.min(a.max(c));
            if t0 > t1 {
                hit = false;
                break;
            }
        }
        if hit {
            best = Some((t0, loc));
        }
    }
    best
}

/// F3 toggles drawing every zombie's hit boxes (red head/neck, yellow
/// torso, green limbs), to compare them with the model.
fn draw_hitboxes(keys: Res<ButtonInput<KeyCode>>, mut on: Local<bool>, zq: Query<&Hitboxes>, globals: Query<&GlobalTransform>, mut gizmos: Gizmos) {
    use zm_core::weapons::HitLoc;
    if keys.just_pressed(KeyCode::F3) {
        *on = !*on;
    }
    *on |= std::env::var_os("UNDEAD_SHOW_HITBOXES").is_some() && !keys.pressed(KeyCode::F3);
    if !*on {
        return;
    }
    for boxes in &zq {
        for (joint, b) in &boxes.0 {
            let Ok(g) = globals.get(*joint) else { continue };
            let loc = HitLoc::ALL.get(b.loc as usize).copied().unwrap_or(HitLoc::None);
            let color = if loc.is_head() || loc == HitLoc::Neck {
                Color::srgb(1.0, 0.2, 0.2)
            } else if matches!(loc, HitLoc::TorsoUpper | HitLoc::TorsoLower) {
                Color::srgb(1.0, 0.9, 0.2)
            } else {
                Color::srgb(0.3, 1.0, 0.3)
            };
            let local = Transform::from_translation((b.min + b.max) * 0.5).with_scale(b.max - b.min);
            gizmos.cuboid(g.mul_transform(local), color);
        }
    }
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
    let mut boxes = Hitboxes::default();
    crate::nacht::spawn_part(commands, &ch.body, &mut joints, model, model);
    boxes.add(&ch.body, &joints);
    if !ch.heads.is_empty() {
        let head = &ch.heads[fastrand::usize(..ch.heads.len())];
        crate::nacht::spawn_part(commands, head, &mut joints, model, model);
        boxes.add(head, &joints);
    }
    commands.entity(root).insert(boxes);
    commands.entity(root).insert(ZombieRig { joints, variant: fastrand::usize(..12), clip: usize::MAX, time: 0.0, rate: 1.0, map: Vec::new(), prev: None });
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
    times: Res<AnimTimes>,
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
            ZState::Climbing => match times.traverse(z.climb_anim) {
                // The AI's clock (seconds into the vault), at normal speed.
                Some(c) => {
                    sync = Some(z.timer);
                    (models.clip(&c.name), 1.0)
                }
                None => (find(&["ai_zombie_traverse_v1", "ai_zombie_traverse_v2"], v), 1.0),
            },
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
            // Keep the old clip playing while it fades out.
            if rig.clip != usize::MAX && rig.clip < models.clips.len() {
                let map = std::mem::take(&mut rig.map);
                rig.prev = Some(FadingClip { clip: rig.clip, time: rig.time, rate: rig.rate, map, age: 0.0 });
            }
            rig.clip = want;
            rig.time = 0.0;
            rig.map = crate::nacht::track_map(&models.clips[want], &rig.joints);
        }
        rig.rate = rate;
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
        let ZombieRig { joints, map, prev, .. } = &mut *rig;
        let pose = crate::nacht::sample_mapped(clip, frame, joints, map);
        let fading = prev.as_mut().and_then(|p| {
            p.age += dt;
            p.time += dt * p.rate;
            let w = p.age / BLEND_TIME;
            (w < 1.0).then(|| {
                let old = &models.clips[p.clip];
                (crate::nacht::sample_mapped(old, old.frame_at(p.time), joints, &p.map), w * w * (3.0 - 2.0 * w))
            })
        });
        if fading.is_none() {
            *prev = None;
        }
        // Smoothstep from the fading pose to the new one, joint by joint.
        let mut out: Vec<(usize, Quat, Vec3)> = match fading {
            Some((old, w)) => {
                let mut blended: std::collections::HashMap<usize, (Quat, Vec3)> = old.into_iter().map(|(j, r, t)| (j, (r, t))).collect();
                for (j, r, t) in pose {
                    let e = blended.entry(j).or_insert((r, t));
                    *e = (e.0.slerp(r, w), e.1.lerp(t, w));
                }
                blended.into_iter().map(|(j, (r, t))| (j, r, t)).collect()
            }
            None => pose,
        };
        for (j, r, t) in out.drain(..) {
            if let Ok(mut tr) = tq.get_mut(joints[j].1) {
                tr.rotation = r;
                tr.translation = t;
            }
        }
    }
}

/// An attack or board-tearing anim as the AI times it: its length and the
/// moments its notetracks fire (seconds).
#[derive(Debug, Clone, Default)]
pub struct Timed {
    pub len: f32,
    pub events: Vec<f32>,
    /// The animation is loaded (it can be shown).
    pub shown: bool,
}

/// The attack and tear timings, read from the loaded animations' own
/// notetracks (`fire` for a hit, `board` for a pulled board) so damage and
/// boards land on the frames the animation shows; the rules' numbers when
/// the animations aren't loaded.
#[derive(Resource, Default)]
pub struct AnimTimes {
    attacks: Vec<Timed>,
    tears: Vec<Timed>,
    /// The window-vault animations (`ai_zombie_traverse*`), whose root
    /// motion carries the zombie through the window.
    traverses: Vec<std::sync::Arc<crate::nacht::build::AnimClip>>,
    from_clips: bool,
}

impl AnimTimes {
    fn from_rules(list: &[rules::TimedAnim]) -> Vec<Timed> {
        list.iter().map(|a| Timed { len: a.len, events: a.events.to_vec(), shown: false }).collect()
    }

    fn attack(&self, i: usize, rules: &rules::ZombieRules) -> Timed {
        self.attacks.get(i).cloned().unwrap_or_else(|| Self::from_rules(rules.attacks).get(i).cloned().unwrap_or_default())
    }

    fn traverse(&self, i: usize) -> Option<&crate::nacht::build::AnimClip> {
        (!self.traverses.is_empty()).then(|| &*self.traverses[i % self.traverses.len()])
    }

    fn tear(&self, i: usize, rules: &rules::ZombieRules) -> Timed {
        self.tears.get(i).cloned().unwrap_or_else(|| Self::from_rules(rules.tears).get(i).cloned().unwrap_or_default())
    }
}

fn build_anim_times(models: Option<Res<crate::nacht::ZombieModels>>, round: Res<Round>, mut times: ResMut<AnimTimes>) {
    let Some(models) = models.filter(|m| !m.clips.is_empty()) else { return };
    if times.from_clips && !models.is_changed() {
        return;
    }
    let read = |list: &[rules::TimedAnim], note: &str| -> Vec<Timed> {
        list.iter()
            .map(|a| match models.clip(a.name).map(|i| &models.clips[i]) {
                Some(c) => {
                    let events: Vec<f32> = c.notify.iter().filter(|(n, _)| n.eq_ignore_ascii_case(note)).map(|(_, f)| f * c.duration).collect();
                    info!("zombie anim {}: {:.2}s, {note} at {:?}", a.name, c.duration, events.iter().map(|e| (e * 100.0).round() / 100.0).collect::<Vec<_>>());
                    Timed { len: c.duration, events: if events.is_empty() { a.events.to_vec() } else { events }, shown: true }
                }
                None => Timed { len: a.len, events: a.events.to_vec(), shown: false },
            })
            .collect()
    };
    let rules = &round.0.rules;
    times.attacks = read(rules.attacks, "fire");
    times.tears = read(rules.tears, "board");
    // (The crawl variant is for legless crawlers, which aren't in yet.)
    times.traverses = models
        .clips
        .iter()
        .filter(|c| {
            let n = c.name.to_ascii_lowercase();
            n.starts_with("ai_zombie_traverse") && !n.contains("crawl")
        })
        .cloned()
        .collect();
    for c in &times.traverses {
        let e = c.root_at(c.duration);
        info!("zombie vault {}: {:.2}s, root motion {:.0} forward, {:.0} up at the end", c.name, c.duration, e[0], e[2]);
    }
    times.from_clips = true;
}

pub struct ZombiesPlugin;

impl Plugin for ZombiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimTimes>().add_systems(
            Update,
            (build_anim_times, update_field, ai, separate, animate, animate_rigs).chain().run_if(in_state(GameState::Playing)),
        )
        .add_systems(Update, (dying, animate_previews, draw_hitboxes))
        .add_systems(Update, debug_paths.after(separate).run_if(in_state(GameState::Playing).and(path_debug)))
        .add_systems(Update, open_doors_for_test.run_if(in_state(GameState::Playing).and(|| std::env::var_os("UNDEAD_TEST_OPEN_DOORS").is_some())))
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
        let routed = NavCtx::of(world).and_then(|ctx| pick_window(&ctx, &world.window_fields, &windows, p));
        let wi = match routed {
            Some(wi) => wi,
            None => *windows.iter().min_by(|a, b| {
                let da = outside_of(level, **a).distance(p);
                let db = outside_of(level, **b).distance(p);
                da.total_cmp(&db)
            })?,
        };
        let jitter = Vec3::new(fastrand::f32() - 0.5, 0.0, fastrand::f32() - 0.5) * 2.0;
        let q = p + jitter;
        (wi, Vec3::new(q.x, ground_y(world, q.x, q.z, p.y + 1.0), q.z))
    };

    // The round's health and the gait rolled from `zombie_move_speed`.
    // Dev: `UNDEAD_TEST_ZOMBIE_SPEED=walk|run|sprint` forces the gait.
    let (gait, base_speed) = match std::env::var("UNDEAD_TEST_ZOMBIE_SPEED").ok().as_deref() {
        Some("walk") => (Gait::Walk, Gait::Walk.speed()),
        Some("run") => (Gait::Run, Gait::Run.speed()),
        Some("sprint") => (Gait::Sprint, Gait::Sprint.speed()),
        _ => (spec.gait, spec.speed),
    };
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
                speed: base_speed * (0.9 + fastrand::f32() * 0.2),
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
                mover: Mover::at(pos),
                climb_from: 0.0,
                climb_anim: fastrand::usize(..64),
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

/// Developer aid: `UNDEAD_TEST_ZOMBIE_PATH=1` logs every zombie's route,
/// node progress and height (see [`debug_paths`]).
pub fn path_debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("UNDEAD_TEST_ZOMBIE_PATH").is_some())
}

/// Where the player stands for the zombies: the ground under their feet
/// (so a jump doesn't lift the goal off the floor).
fn player_goal(world: &World, feet: Vec3) -> Vec3 {
    match &world.mesh {
        Some(m) => Vec3::new(feet.x, navgraph::ground(m, feet.x, feet.z, feet.y + 0.3, feet.y - 3.0).unwrap_or(feet.y), feet.z),
        None => feet,
    }
}

fn update_field(
    time: Res<Time>,
    mut world: ResMut<World>,
    player: Query<(&Transform, &player::PlayerCtl), With<Player>>,
    mut seeds: Local<Vec<(usize, f32)>>,
) {
    world.field_timer -= time.delta_secs();
    if world.field_timer > 0.0 && !world.field.is_empty() {
        return;
    }
    world.field_timer = 0.25;
    let Ok((p, ctl)) = player.single() else { return };
    let goal = player_goal(&world, ctl.feet(p));
    if let Some(ctx) = NavCtx::of(&world) {
        let f = player_field(&ctx, goal, &mut seeds);
        world.field = f;
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
        Some(m) => navgraph::ground(m, x, z, from + 0.6, from - 4.0).unwrap_or(from - 0.6),
        None => 0.0,
    }
}

fn v3c(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

fn flat(v: Vec3) -> f32 {
    Vec2::new(v.x, v.z).length()
}

/// Route following, collision and ground height of one zombie on a real
/// map. Positions handed in and out are feet positions.
#[derive(Debug, Clone, Default)]
pub struct Mover {
    /// Path node currently walked to.
    pub node: Option<usize>,
    /// Last path node reached (a known spot on the graph).
    pub last_node: Option<usize>,
    /// Walking straight at the goal (a clear, walkable line to it).
    pub direct: bool,
    pub plan_timer: f32,
    /// Seconds left with direct pursuit switched off (after getting stuck).
    pub no_direct: f32,
    /// Height of the ground under the feet; the model's height eases
    /// toward it so stairs read as a ramp.
    pub ground: f32,
    /// Falling speed after walking off a ledge.
    pub fall: f32,
    pub stuck_timer: f32,
    pub stuck_at: Vec3,
    pub stuck_count: u32,
    /// Seconds spent walking into something (little progress per step).
    pub blocked: f32,
    /// Node that must be stepped on exactly before moving on.
    pub precise: Option<usize>,
    /// Nodes skipped for a few seconds after failing to reach them.
    pub banned: Vec<(usize, f32)>,
    /// Times the zombie had to be moved out of a dead end (for the logs).
    pub rescues: u32,
}

impl Mover {
    pub fn at(p: Vec3) -> Mover {
        Mover { ground: p.y, stuck_at: p, ..default() }
    }

    pub fn feet(&self, t: Vec3) -> Vec3 {
        Vec3::new(t.x, self.ground, t.z)
    }

    /// Restart navigation from a new spot (after a vault or a rescue).
    pub fn reset(&mut self, p: Vec3) {
        *self = Mover { rescues: self.rescues, ..Mover::at(p) };
    }
}

/// What zombies find their way with on a real map.
pub struct NavCtx<'a> {
    pub mesh: &'a TriMesh,
    pub graph: &'a NavGraph,
    pub open: &'a [bool],
    /// Boxes that block like walls (window fills, closed doors, the box).
    pub solids: &'a [Aabb],
}

impl<'a> NavCtx<'a> {
    pub fn of(world: &'a World) -> Option<NavCtx<'a>> {
        Some(NavCtx { mesh: world.mesh.as_deref()?, graph: world.graph.as_deref()?, open: &world.door_open, solids: &world.player_solids })
    }

    pub fn node(&self, i: usize) -> Vec3 {
        let n = self.graph.nodes[i];
        Vec3::new(n.x, n.y, n.z)
    }

    /// Can a zombie walk straight from `a` to `b` (feet positions)? Same
    /// test as the graph links ([`navgraph::walkable`]) plus the gameplay
    /// solids.
    pub fn walk_ok(&self, a: Vec3, b: Vec3) -> bool {
        // Boxes grown by the body radius (a little less than the push-out
        // distance, so a zombie touching one can still walk away).
        let solids_ok = self.solids.iter().map(|s| s.inflate_xz(0.27)).all(|s| {
            [0.6f32, 1.3].iter().all(|h| {
                let o = V3::new(a.x, a.y + h, a.z);
                let d = V3::new(b.x, b.y + h, b.z).sub(o);
                let l = d.len();
                l < 1e-3 || s.ray_hit(o, d.scale(1.0 / l), l).is_none()
            })
        });
        solids_ok && navgraph::walkable(self.mesh, v3c(a), v3c(b))
    }
}

/// The shared chase field: Dijkstra over the path nodes from every node
/// with a walkable line to the player, each starting at its distance to
/// them. Covers the whole map (through open doors only), so every zombie
/// always has a route, however far away. When no node reaches the player
/// (standing on a crate, mid-jump over a gap) the last seeds stay, so
/// zombies head for where the player was last reachable, like the
/// original's breadcrumbs.
pub fn player_field(ctx: &NavCtx, player: Vec3, last: &mut Vec<(usize, f32)>) -> Vec<f32> {
    let here = v3c(player);
    let order = ctx.graph.by_distance(here);
    let mut seeds = Vec::new();
    for &(fd, i) in order.iter().take(16) {
        if fd > 10.0 || seeds.len() >= 4 {
            break;
        }
        let n = ctx.node(i);
        if ctx.walk_ok(n, player) {
            seeds.push((i, n.distance(player)));
        }
    }
    if seeds.is_empty() {
        if last.is_empty() {
            seeds.extend(order.first().map(|&(d, i)| (i, d)));
        } else {
            seeds = last.clone();
        }
    } else {
        *last = seeds.clone();
    }
    ctx.graph.field_from(&seeds, ctx.open)
}

/// Route field to the spot outside window `wi` (seeded like the player's).
pub fn window_field(world: &World, level: &zm_core::level::Level, wi: usize) -> Vec<f32> {
    let Some(ctx) = NavCtx::of(world) else { return Vec::new() };
    let o = outside_of(level, wi);
    let goal = Vec3::new(o.x, ground_y(world, o.x, o.z, o.y + 1.0), o.z);
    player_field(&ctx, goal, &mut Vec::new())
}

/// The window a zombie spawned at `p` heads for: like the original (the
/// closest entrances, a random one within 500 units of the best), but by
/// route length over the graph, so an entrance it can't walk to (an
/// upstairs window above a walled-in trench) is never chosen.
pub fn pick_window(ctx: &NavCtx, fields: &[Vec<f32>], windows: &[usize], p: Vec3) -> Option<usize> {
    let here = v3c(p);
    let near: Vec<(f32, usize)> = ctx.graph.by_distance(here).into_iter().take(6).collect();
    let cost = |wi: usize| {
        let f = fields.get(wi)?;
        near.iter().filter_map(|&(d, n)| f.get(n).filter(|c| c.is_finite()).map(|c| d + c)).min_by(f32::total_cmp)
    };
    let costs: Vec<(f32, usize)> = windows.iter().filter_map(|&wi| cost(wi).map(|c| (c, wi))).collect();
    let best = costs.iter().map(|c| c.0).min_by(f32::total_cmp)?;
    let close: Vec<usize> = costs.iter().filter(|c| c.0 <= best + 500.0 * 0.0254).map(|c| c.1).collect();
    close.get(fastrand::usize(..close.len())).copied()
}

/// Picks how to head for `goal`: straight at it when a zombie can walk
/// there in a line, else the best node it can walk to (lowest distance +
/// remaining route), which also skips nodes it can cut past.
fn plan(ctx: &NavCtx, m: &mut Mover, feet: Vec3, goal: Vec3, field: &[f32]) {
    m.plan_timer = 0.3 + fastrand::f32() * 0.2;
    m.direct = m.no_direct <= 0.0 && flat(goal - feet) < 30.0 && ctx.walk_ok(feet, goal);
    if m.direct {
        return;
    }
    let here = v3c(feet);
    let usable = |i: usize| field.get(i).is_some_and(|f| f.is_finite()) && !m.banned.iter().any(|b| b.0 == i);
    let score = |i: usize| ctx.graph.nodes[i].sub(here).len() + field[i];
    // Nodes in reach on a walkable slope (not on another floor straight
    // above or below), best (distance + rest of the route) first.
    let reachable = |n: V3| {
        let d = n.sub(here);
        let h = (d.x * d.x + d.z * d.z).sqrt();
        h < 12.0 && d.y.abs() <= h * 0.9 + 0.6
    };
    let mut cands: Vec<(f32, usize)> = (0..ctx.graph.nodes.len())
        .filter(|&i| usable(i) && reachable(ctx.graph.nodes[i]))
        .map(|i| (score(i), i))
        .collect();
    cands.sort_by(|a, b| a.0.total_cmp(&b.0));
    // A better node than the current one (further along, cutting a corner)?
    let current = m.node.filter(|&i| usable(i));
    let cur_score = current.map(score).unwrap_or(f32::INFINITY);
    let walkable = |i: usize| ctx.walk_ok(feet, ctx.node(i));
    let pick = cands
        .iter()
        .take_while(|c| c.0 < cur_score - 0.01)
        .take(12)
        .map(|c| c.1)
        .find(|&i| walkable(i))
        // Keep the current node while walking to it works in practice (a
        // line that slid a little off can still be followed; one that is
        // really blocked gets the node banned in `steer`).
        .or(current)
        // No node yet (just vaulted in, or the last one was banned): the
        // best walkable one, then the last node reached.
        .or_else(|| cands.iter().map(|c| c.1).take(24).find(|&i| walkable(i)))
        .or_else(|| m.last_node.filter(|&i| usable(i) && walkable(i)));
    if pick.is_some() {
        m.node = pick;
    } else if m.node.is_none() {
        // Nothing walkable at all: the closest node with a route.
        m.node = cands
            .iter()
            .min_by(|a, b| navgraph::floor_dist(ctx.graph.nodes[a.1], here).total_cmp(&navgraph::floor_dist(ctx.graph.nodes[b.1], here)))
            .map(|c| c.1)
            .or_else(|| ctx.graph.by_distance(here).first().map(|c| c.1));
    }
}

/// The point to walk to next on the way to `goal` along `field`.
pub fn steer(ctx: &NavCtx, m: &mut Mover, feet: Vec3, goal: Vec3, field: &[f32], dt: f32) -> Vec3 {
    m.plan_timer -= dt;
    m.no_direct -= dt;
    m.banned.retain_mut(|b| {
        b.1 -= dt;
        b.1 > 0.0
    });
    // Walking into something for a while: the line to the node (or the
    // player) doesn't work in practice, so leave that node alone for a bit
    // and pick another way.
    if m.blocked > 0.4 {
        m.blocked = 0.0;
        if let Some(n) = m.node.filter(|_| !m.direct) {
            m.banned.push((n, 3.0));
            m.node = None;
        }
        m.no_direct = m.no_direct.max(1.5);
        m.plan_timer = 0.0;
    }
    if m.plan_timer <= 0.0 {
        plan(ctx, m, feet, goal, field);
    }
    if m.direct {
        return goal;
    }
    let Some(n) = m.node else { return goal };
    let p = ctx.node(n);
    if flat(p - feet) < 0.5 && (p.y - feet.y).abs() >= 1.2 {
        // Right above or below the node: on the wrong floor, so pick again.
        m.node = None;
        plan(ctx, m, feet, goal, field);
        return m.node.map(|n| ctx.node(n)).unwrap_or(goal);
    }
    let d = flat(p - feet);
    if d < 0.5 {
        let next = ctx.graph.next(n, field, ctx.open);
        // Links are only known to work from the node itself: near it, move
        // on only if the next one can be walked to from here.
        if d > 0.15 && m.precise != Some(n) {
            if next.is_some_and(|nx| !ctx.walk_ok(feet, ctx.node(nx))) {
                m.precise = Some(n);
                return p;
            }
        } else if d > 0.15 {
            return p;
        }
        m.last_node = Some(n);
        match next {
            Some(nx) => {
                m.node = Some(nx);
                return ctx.node(nx);
            }
            // End of the route: the goal is in reach from here.
            None => {
                m.direct = true;
                return goal;
            }
        }
    }
    p
}

/// Walks toward `target` at `speed`: slides along walls and props, stays
/// out of the gameplay solids, and keeps the feet on the ground (rising
/// up stairs as on a ramp, falling off ledges). `pos` is the model's
/// position (feet); returns the direction walked.
pub fn walk_to(ctx: &NavCtx, m: &mut Mover, pos: &mut Vec3, target: Vec3, speed: f32, dt: f32) -> Vec3 {
    let d = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z);
    let len = d.length();
    let dir = if len > 1e-3 { d / len } else { Vec3::ZERO };
    let step = (speed * dt).min(len);
    let before = *pos;
    pos.x += dir.x * step;
    pos.z += dir.z * step;
    collide(ctx, m, pos);
    // Progress along the wanted direction (sliding along a wall counts
    // only as far as it gets us there).
    let made = (*pos - before).dot(dir);
    if step > 1e-4 && made < step * 0.3 {
        m.blocked += dt;
    } else {
        m.blocked = (m.blocked - dt).max(0.0);
    }
    settle(ctx, m, pos, dir, target.y, speed, dt);
    dir
}

/// Pushes the body out of walls (two spheres above step height, so stairs
/// and kerbs are walked onto, not pushed off) and the gameplay solids.
pub fn collide(ctx: &NavCtx, m: &Mover, pos: &mut Vec3) {
    for h in navgraph::BODY_HEIGHTS {
        let (q, _) = navgraph::push_body(ctx.mesh, V3::new(pos.x, m.ground + h, pos.z), navgraph::BODY, 2);
        pos.x = q.x;
        pos.z = q.z;
    }
    for s in ctx.solids.iter().filter(|s| s.min.y < m.ground + 1.6 && s.max.y > m.ground + 0.2) {
        if let Some((x, z)) = s.push_circle(pos.x, pos.z, 0.3) {
            pos.x = x;
            pos.z = z;
        }
    }
}

/// Ground following. Stairs are averaged over half a metre along the walk
/// and eased, so the body glides up and down them like a ramp.
pub fn settle(ctx: &NavCtx, m: &mut Mover, pos: &mut Vec3, dir: Vec3, target_y: f32, speed: f32, dt: f32) {
    let step = navgraph::STEP;
    // Steps up only toward the height of where it is heading.
    let climb = navgraph::climb_limit(m.ground, target_y) + 0.05;
    match navgraph::ground(ctx.mesh, pos.x, pos.z, m.ground + climb, m.ground - 6.0) {
        Some(g) if g < m.ground - step => {
            m.fall += 9.8 * dt;
            m.ground = (m.ground - m.fall * dt).max(g);
        }
        Some(g) => {
            m.ground = g;
            m.fall = 0.0;
        }
        None => {}
    }
    let mut sum = m.ground;
    let mut n = 1.0;
    if m.fall == 0.0 && dir != Vec3::ZERO {
        for s in [-0.3f32, 0.3] {
            if let Some(g) = navgraph::ground(ctx.mesh, pos.x + dir.x * s, pos.z + dir.z * s, m.ground + step + 0.1, m.ground - step - 0.1) {
                sum += g;
                n += 1.0;
            }
        }
    }
    let want = sum / n;
    if m.fall > 0.0 {
        pos.y = m.ground;
    } else {
        // Ease toward the ramp height, no faster than a steep climb at
        // this walking speed.
        let eased = pos.y + (want - pos.y) * (1.0 - (-dt * 12.0).exp());
        let vmax = (1.2 * speed + 1.0) * dt;
        pos.y = (pos.y + (eased - pos.y).clamp(-vmax, vmax)).clamp(m.ground - step, m.ground + step);
    }
}

/// Progress check every 1.5 s while trying to move. A zombie that made no
/// headway stops cutting corners and replans; after two failures it goes
/// back to the last node it reached; after four it is put on that node (a
/// last resort, logged).
pub fn check_stuck(ctx: &NavCtx, m: &mut Mover, pos: &mut Vec3, moving: bool, dt: f32) -> bool {
    m.stuck_timer += dt;
    if m.stuck_timer < 1.5 {
        return false;
    }
    m.stuck_timer = 0.0;
    let moved = flat(*pos - m.stuck_at);
    m.stuck_at = *pos;
    if !moving || moved > 0.4 {
        m.stuck_count = 0;
        return false;
    }
    m.stuck_count += 1;
    m.no_direct = 3.0;
    m.plan_timer = 0.0;
    if m.stuck_count == 2 {
        m.node = m.last_node.or(m.node);
    }
    if m.stuck_count < 4 {
        return false;
    }
    let here = v3c(m.feet(*pos));
    let to = m.last_node.or_else(|| ctx.graph.by_distance(here).first().map(|c| c.1));
    if let Some(n) = to {
        let p = ctx.node(n);
        *pos = p;
        let rescues = m.rescues + 1;
        m.reset(p);
        m.rescues = rescues;
        m.last_node = Some(n);
        m.stuck_at = p;
    }
    true
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
    times: Res<AnimTimes>,
) {
    let dt = time.delta_secs().min(0.05);
    let rules = &round.0.rules;
    let Ok((pt, pctl)) = player.single() else { return };
    let ppos = Vec3::new(pt.translation.x, 0.0, pt.translation.z);
    let level = &level.0;
    let nav = NavCtx::of(&world);
    let goal = player_goal(&world, pctl.feet(pt));
    let debug = path_debug();

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
            let anim = times.attack(z.act % rules.attacks.len().max(1), rules);
            let before = z.act_t;
            z.act_t += dt;
            z.swing -= dt;
            let reach = if z.state == ZState::Chase { rules.melee_range } else { rules.melee_range + 0.4 };
            // On a real map a chase swing only lands on the same floor.
            let level_ok = nav.is_none() || z.state != ZState::Chase || (goal.y - z.mover.ground).abs() < 1.2;
            for &hit in &anim.events {
                if hit > before && hit <= z.act_t && dist_player < reach && level_ok {
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
                } else if let Some(ctx) = nav.as_ref() {
                    // Walk the graph to the spot outside the window (its own field).
                    let mut pos = t.translation;
                    let feet = z.mover.feet(pos);
                    let goal = Vec3::new(outside.x, ground_y(&world, outside.x, outside.z, w.center.y + 1.0), outside.z);
                    let field = world.window_fields.get(z.window).map(Vec::as_slice).unwrap_or(&[]);
                    let speed = z.speed.min(3.5);
                    let m = &mut z.mover;
                    let target = steer(ctx, m, feet, goal, field, dt);
                    let dir = walk_to(ctx, m, &mut pos, target, speed, dt);
                    // (Queueing behind others at the window is not being stuck.)
                    let queueing = flat(goal - pos) < 2.0;
                    if check_stuck(ctx, m, &mut pos, !queueing, dt) && debug {
                        info!("[zpath] approach rescue: back to node {:?} at {pos:?}", m.last_node);
                    }
                    t.translation = pos;
                    face(&mut t, dir, dt, 8.0);
                    // Lost (rescued off dead ends again and again) or taking
                    // far too long: appear at the window.
                    z.timer += dt;
                    if (z.timer > 30.0 && z.mover.rescues >= 3) || z.timer > 150.0 {
                        if debug {
                            info!("[zpath] approach timeout: placed at window {} from {:?}", z.window, t.translation);
                        }
                        t.translation = goal;
                        z.mover.reset(goal);
                        z.timer = 0.0;
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
                    start_attack(&mut z, rules, &times);
                }
                // One board per tear anim, pulled off on its `board`
                // notetrack; in through the window once none are left.
                let tear = times.tear(z.act % rules.tears.len().max(1), rules);
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
                // Through the window on the vault animation's own root
                // motion (seconds in `timer`), fitted to run from the
                // outside point to the inside one.
                if z.timer == 0.0 {
                    z.climb_from = t.translation.y;
                }
                z.timer += dt;
                let (k, rise) = match times.traverse(z.climb_anim) {
                    Some(c) => {
                        let (r, e) = (c.root_at(z.timer), c.root_at(c.duration));
                        let f = if e[0].abs() > 1.0 { (r[0] / e[0]).clamp(0.0, 1.0) } else { (z.timer / c.duration).min(1.0) };
                        // The anim's own up-and-over, ending level.
                        let done = z.timer >= c.duration;
                        (if done { 1.0 } else { f.min(0.999) }, (r[2] - e[2] * f) * crate::nacht::build::INCH * z.scale)
                    }
                    None => {
                        let k = (z.timer / 0.9).min(1.0);
                        (k, (k * std::f32::consts::PI).sin() * 0.75)
                    }
                };
                let p = outside.lerp(inside, k);
                let base = z.climb_from + (floor_in - z.climb_from) * k;
                t.translation = Vec3::new(p.x, base + rise.max(0.0), p.z);
                face(&mut t, inside - outside, dt, 10.0);
                if k >= 1.0 {
                    t.translation.y = floor_in;
                    z.state = ZState::Chase;
                    z.mover.reset(t.translation);
                }
            }
            ZState::Chase if nav.is_some() => {
                let ctx = nav.as_ref().expect("nav");
                let mut pos = t.translation;
                let feet = z.mover.feet(pos);
                let level_with = (goal.y - feet.y).abs() < 1.2;
                // Melee from 64 units; the swings repeat while in reach.
                if dist_player < rules.melee_range && level_with && z.swing <= 0.0 {
                    start_attack(&mut z, rules, &times);
                }
                let near = flat(goal - feet) < 1.1 && level_with;
                if near {
                    face(&mut t, to_player, dt, 10.0);
                } else {
                    // Every zombie always knows where the player is and
                    // follows the shared field there, however far.
                    let speed = if z.swing > 0.0 { z.speed * 0.4 } else { z.speed };
                    let m = &mut z.mover;
                    let target = steer(ctx, m, feet, goal, &world.field, dt);
                    let dir = walk_to(ctx, m, &mut pos, target, speed, dt);
                    face(&mut t, dir, dt, 7.0);
                }
                if check_stuck(ctx, &mut z.mover, &mut pos, !near, dt) && debug {
                    info!("[zpath] chase rescue #{}: back to node {:?} at {pos:?}", z.mover.rescues, z.mover.last_node);
                }
                t.translation = pos;
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
                    start_attack(&mut z, rules, &times);
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
fn start_attack(z: &mut Zombie, rules: &rules::ZombieRules, times: &AnimTimes) {
    if rules.attacks.is_empty() {
        return;
    }
    // Only swings whose animation is loaded, so every hit is seen.
    let shown: Vec<usize> = (0..rules.attacks.len()).filter(|&i| times.attacks.get(i).is_some_and(|t| t.shown)).collect();
    z.act = if shown.is_empty() { fastrand::usize(..rules.attacks.len()) } else { shown[fastrand::usize(..shown.len())] };
    z.act_t = 0.0;
    z.swing = times.attack(z.act, rules).len;
}

/// Push zombies apart so they crowd instead of stacking.
/// Push zombies apart so they crowd instead of stacking (only zombies on
/// the same floor), then back out of any wall the push moved them into.
/// A zombie's body for collision (the engine's actor radius, 15 units) and
/// its height.
pub const BODY_RADIUS: f32 = 15.0 * 0.0254;
pub const BODY_HEIGHT: f32 = 1.8;

fn separate(world: Res<World>, mut zq: Query<(Entity, &mut Transform, &Zombie)>, player: Query<(&Transform, &player::PlayerCtl), (With<Player>, Without<Zombie>)>) {
    // The player's body: zombies stop against it rather than walk in.
    let me = player.single().ok().map(|(t, c)| (Vec2::new(t.translation.x, t.translation.z), c.feet_y, c.feet_y + c.stance.height()));
    let real = world.mesh.is_some();
    let pts: Vec<(Entity, Vec3, ZState)> = zq
        .iter()
        .map(|(e, t, z)| (e, Vec3::new(t.translation.x, if real { z.mover.ground } else { 0.0 }, t.translation.z), z.state))
        .collect();
    let nav = NavCtx::of(&world);
    for (e, mut t, z) in &mut zq {
        if !matches!(z.state, ZState::Chase | ZState::Approach) {
            continue;
        }
        let ground = if real { z.mover.ground } else { 0.0 };
        let at = Vec2::new(t.translation.x, t.translation.z);
        let mut push = Vec2::ZERO;
        if let Some((pp, pf, ph)) = me {
            let d = at - pp;
            let (l, min) = (d.length(), crate::player::RADIUS + BODY_RADIUS * z.scale);
            if l < min && l > 1e-4 && pf < t.translation.y + BODY_HEIGHT * z.scale && ph > t.translation.y {
                push += d / l * (min - l);
            }
        }
        let me = at;
        for (oe, op, os) in &pts {
            if *oe == e || *os == ZState::Dying || (op.y - ground).abs() > 1.2 {
                continue;
            }
            let d = me - Vec2::new(op.x, op.z);
            let l = d.length();
            if l < 0.6 && l > 1e-4 {
                push += d / l * (0.6 - l) * 0.5;
            }
        }
        if push == Vec2::ZERO {
            continue;
        }
        let mut p = t.translation;
        p.x += push.x;
        p.z += push.y;
        if let Some(ctx) = &nav {
            collide(ctx, &z.mover, &mut p);
        }
        t.translation.x = p.x;
        t.translation.z = p.z;
    }
}

/// Per-zombie record for [`debug_paths`].
#[derive(Default)]
struct Track {
    last_y: f32,
    last_state: Option<ZState>,
    /// Largest height change between two frames while walking, and the
    /// largest vertical speed.
    max_dy: f32,
    max_vy: f32,
    chase_for: f32,
    reached: bool,
    climbed: f32,
}

/// `UNDEAD_TEST_ZOMBIE_PATH=1`: logs each zombie's route (node, field
/// distance, next nodes), height and the biggest per-frame height change,
/// and when it reaches the player. Height jumps over 6 cm in a frame while
/// walking are logged as `JUMP`.
fn debug_paths(
    time: Res<Time>,
    world: Res<World>,
    player: Query<(&Transform, &player::PlayerCtl), With<Player>>,
    zq: Query<(Entity, &Transform, &Zombie)>,
    mut tracks: Local<std::collections::HashMap<Entity, Track>>,
    mut every: Local<f32>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let Ok((pt, ctl)) = player.single() else { return };
    let goal = player_goal(&world, ctl.feet(pt));
    let nav = NavCtx::of(&world);
    let now = time.elapsed_secs();
    tracks.retain(|e, _| zq.contains(*e));
    *every -= dt;
    let report = *every <= 0.0;
    if report {
        *every = 2.0;
        info!("[zpath] t={now:.1} player ground ({:.2} {:.2} {:.2}) zombies {}", goal.x, goal.y, goal.z, zq.iter().count());
    }
    for (e, t, z) in &zq {
        let tr = tracks.entry(e).or_insert_with(|| Track { last_y: t.translation.y, ..default() });
        // (Rescues and the approach timeout move a zombie on purpose.)
        let moved_on_purpose = (t.translation - Vec3::new(t.translation.x, tr.last_y, t.translation.z)).length() > 0.0
            && z.mover.stuck_count == 0
            && z.mover.stuck_at == t.translation;
        let walking = matches!(z.state, ZState::Chase | ZState::Approach) && tr.last_state == Some(z.state) && !moved_on_purpose;
        let dy = t.translation.y - tr.last_y;
        if walking {
            // Faster than the eased climb allows (a snap), unless falling.
            let cap = (1.2 * z.speed + 1.0) * dt.min(0.05) + 0.01;
            if dy.abs() > cap && z.mover.fall == 0.0 {
                info!("[zpath] JUMP {e} {:?} dy {dy:.3} in {dt:.3}s at {:?} ground {:.2}", z.state, t.translation, z.mover.ground);
            }
            tr.max_dy = tr.max_dy.max(dy.abs());
            tr.max_vy = tr.max_vy.max(dy.abs() / dt);
            tr.climbed += dy.max(0.0);
        }
        tr.last_y = t.translation.y;
        tr.last_state = Some(z.state);
        if z.state == ZState::Chase {
            tr.chase_for += dt;
            let close = flat(goal - z.mover.feet(t.translation)) < 1.2 && (goal.y - z.mover.ground).abs() < 1.2;
            if close && !tr.reached {
                tr.reached = true;
                info!("[zpath] REACHED {e} after {:.1}s of chase, climbed {:.2} m, max dy/frame {:.3}, max vy {:.2} m/s, rescues {}", tr.chase_for, tr.climbed, tr.max_dy, tr.max_vy, z.mover.rescues);
            } else if flat(goal - z.mover.feet(t.translation)) > 2.5 {
                tr.reached = false;
            }
        }
        if report && z.alive() {
            let (node, field, route) = match (&nav, z.mover.node) {
                (Some(ctx), Some(n)) => {
                    let field = if z.state == ZState::Approach { world.window_fields.get(z.window).map(Vec::as_slice).unwrap_or(&[]) } else { &world.field[..] };
                    let route: Vec<usize> = ctx.graph.route(n, field, ctx.open).into_iter().take(6).collect();
                    (format!("{n}"), field.get(n).copied().unwrap_or(f32::NAN), route)
                }
                _ => ("-".into(), f32::NAN, Vec::new()),
            };
            let p = t.translation;
            info!(
                "[zpath]   {e} {:?} pos ({:.2} {:.2} {:.2}) ground {:.2} direct {} node {node} ({field:.1} m left) route {route:?} to player {:.1} m flat, dy {:.2}; max dy/frame {:.3} vy {:.2}",
                z.state, p.x, p.y, p.z, z.mover.ground, z.mover.direct, flat(goal - p), goal.y - z.mover.ground, tr.max_dy, tr.max_vy
            );
        }
    }
}

/// `UNDEAD_TEST_OPEN_DOORS=1`: clears every door and debris pile at the
/// start, so the whole map is reachable for navigation tests.
fn open_doors_for_test(
    mut commands: Commands,
    mut world: ResMut<World>,
    level: Res<LevelRes>,
    debris: Query<Entity, With<crate::world::Debris>>,
) {
    // (Again after a restart, which closes them.)
    if world.graph.is_none() || world.door_open.iter().all(|d| *d) {
        return;
    }
    world.door_open.iter_mut().for_each(|d| *d = true);
    world.rebuild(&level.0);
    for e in &debris {
        commands.entity(e).try_despawn();
    }
    info!("[zpath] test: opened all {} doors", world.door_open.len());
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

/// Headless walks over the real map (needs a World at War install):
/// `cargo test -p zm_game nacht_ -- --ignored --nocapture`.
#[cfg(test)]
mod sim {
    use super::*;
    use crate::nacht::level::testmap;

    pub struct Outcome {
        pub reached: bool,
        pub time: f32,
        pub max_dy: f32,
        pub max_dy_at: Vec3,
        pub rescues: u32,
        pub end: Vec3,
        pub trace: Vec<Vec3>,
    }

    /// Walks one zombie from `start` to a player standing at `player`.
    pub fn chase(ctx: &NavCtx, start: Vec3, player: Vec3, speed: f32, limit: f32) -> Outcome {
        let dt = 1.0 / 60.0;
        fastrand::seed((start.x.to_bits() as u64) << 32 | player.z.to_bits() as u64);
        let field = player_field(ctx, player, &mut Vec::new());
        let mut m = Mover::at(start);
        let mut pos = start;
        let mut t = 0.0;
        let mut max_dy = 0.0f32;
        let mut max_dy_at = start;
        let mut trace = vec![pos];
        while t < limit {
            let feet = m.feet(pos);
            if flat(player - feet) < 1.1 && (player.y - feet.y).abs() < 1.2 {
                return Outcome { reached: true, time: t, max_dy, max_dy_at, rescues: m.rescues, end: pos, trace };
            }
            let y0 = pos.y;
            let was = (m.node, m.direct);
            let target = steer(ctx, &mut m, feet, player, &field, dt);
            if std::env::var_os("SIM_VERBOSE").is_some() && was != (m.node, m.direct) && t < 30.0 {
                println!("    t {t:.2} at {pos:?} ground {:.2}: node {:?} direct {}", m.ground, m.node.map(|n| (n, ctx.node(n))), m.direct);
            }
            walk_to(ctx, &mut m, &mut pos, target, speed, dt);
            let before = m.stuck_count;
            let stuck_pos = pos;
            let stuck_node = m.node;
            let rescued = check_stuck(ctx, &mut m, &mut pos, true, dt);
            if rescued {
                println!("    rescue at {stuck_pos:?} ground {:.2} heading for {:?} direct {} banned {:?}", m.ground, stuck_node.map(|n| (n, ctx.node(n))), m.direct, m.banned);
            }
            if std::env::var_os("SIM_VERBOSE").is_some() && (m.stuck_count != before || rescued) {
                println!("    stuck {} at {pos:?} ground {:.2} node {:?} last {:?} direct {} target {target:?}", m.stuck_count, m.ground, m.node.map(|n| (n, ctx.node(n))), m.last_node, m.direct);
            }
            if !rescued && (pos.y - y0).abs() > max_dy {
                max_dy = (pos.y - y0).abs();
                max_dy_at = pos;
            }
            t += dt;
            if (t / 0.25).floor() != ((t - dt) / 0.25).floor() {
                trace.push(pos);
            }
        }
        Outcome { reached: false, time: t, max_dy, max_dy_at, rescues: m.rescues, end: pos, trace }
    }

    /// Every spawner's zombies reach the outside of every window of its
    /// area (the Approach phase), over the hill and the yard.
    #[test]
    #[ignore = "needs a World at War install"]
    fn nacht_zombies_reach_their_windows() {
        let map = testmap::get();
        let solids = map.level.player_colliders();
        let open = vec![false; map.level.doors.len()];
        let ctx = NavCtx { mesh: &map.scene.collision, graph: &map.nav, open: &open, solids: &solids };
        let (mut ok, mut total) = (0, 0);
        let mut fields = Vec::new();
        for wi in 0..map.level.windows.len() {
            let o = outside_of(&map.level, wi);
            let goal = Vec3::new(o.x, navgraph::ground(&map.scene.collision, o.x, o.z, o.y + 1.0, o.y - 4.0).unwrap_or(o.y - 0.6), o.z);
            fields.push(player_field(&ctx, goal, &mut Vec::new()));
        }
        for (sp, _) in &map.level.spawners {
            let start = Vec3::new(sp.x, sp.y, sp.z);
            let start = Vec3::new(start.x, navgraph::ground(&map.scene.collision, start.x, start.z, start.y + 1.0, start.y - 3.0).unwrap_or(start.y), start.z);
            // The window a zombie from here heads for.
            let windows: Vec<usize> = (0..map.level.windows.len()).collect();
            let Some(wi) = pick_window(&ctx, &fields, &windows, start) else {
                println!("spawner {start:?}: no window with a route");
                total += 1;
                continue;
            };
            let o = outside_of(&map.level, wi);
            let goal = Vec3::new(o.x, navgraph::ground(&map.scene.collision, o.x, o.z, o.y + 1.0, o.y - 4.0).unwrap_or(o.y - 0.6), o.z);
            let field = &fields[wi];
            let mut m = Mover::at(start);
            let mut pos = start;
            let mut t = 0.0;
            let dt = 1.0 / 60.0;
            while t < 60.0 && flat(goal - pos) >= 0.3 {
                let feet = m.feet(pos);
                let target = steer(&ctx, &mut m, feet, goal, field, dt);
                walk_to(&ctx, &mut m, &mut pos, target, 1.1, dt);
                check_stuck(&ctx, &mut m, &mut pos, true, dt);
                t += dt;
            }
            total += 1;
            let reached = flat(goal - pos) < 0.3;
            ok += reached as usize;
            println!("spawner {start:?} -> window {wi}: {} in {t:.1}s, rescues {}", if reached { "reached" } else { "FAILED" }, m.rescues);
        }
        println!("{ok}/{total} spawners reached their window");
        assert_eq!(ok, total);
    }

    /// `SIM_PROBE="x y z node"`: why a walk to a node is (not) possible.
    #[test]
    #[ignore = "needs a World at War install"]
    fn nacht_probe() {
        let Some(v) = std::env::var("SIM_PROBE").ok() else { return };
        let v: Vec<f32> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let map = testmap::get();
        let solids = map.level.player_colliders();
        let open = vec![true; map.level.doors.len()];
        let ctx = NavCtx { mesh: &map.scene.collision, graph: &map.nav, open: &open, solids: &solids };
        let a = Vec3::new(v[0], v[1], v[2]);
        let b = ctx.node(v[3] as usize);
        println!("walk_ok {} walkable {} profile {:?}", ctx.walk_ok(a, b), navgraph::walkable(ctx.mesh, v3c(a), v3c(b)), navgraph::ground_profile(ctx.mesh, v3c(a), v3c(b)));
        for k in 0..=20 {
            let p = a.lerp(b, k as f32 / 20.0);
            let all = map.scene.collision.ground_mask(p.x, p.z, p.y + 2.0, p.y - 3.0, 0.0, zm_core::trimesh::blocks::ALL);
            let ai = navgraph::ground(&map.scene.collision, p.x, p.z, p.y + 2.0, p.y - 3.0);
            println!("  ground at ({:.2} {:.2}): any {all:?} ai-walkable {ai:?}", p.x, p.z);
        }
        let mut m = Mover::at(a);
        let mut pos = a;
        for i in 0..40 {
            let before = pos;
            walk_to(&ctx, &mut m, &mut pos, b, 1.1, 1.0 / 30.0);
            if i % 5 == 0 {
                println!("  step {i}: {before:?} -> {pos:?} ground {:.2}", m.ground);
            }
        }
        for h in navgraph::BODY_HEIGHTS {
            let c = V3::new(pos.x, m.ground + h, pos.z);
            for (ti, t) in ctx.mesh.tris.iter().enumerate() {
                if t.blocks & navgraph::AI_MASK != 0 && t.n.y.abs() < 0.7 && t.closest_point(c).sub(c).len() < navgraph::BODY + 0.01 {
                    println!("  touching tri {ti} at h {h:.2}: {:?} {:?} {:?} n {:?}", t.a, t.b, t.c, t.n);
                }
            }
        }
        for s in &solids {
            if s.push_circle(pos.x, pos.z, 0.31).is_some() {
                println!("  touching solid {s:?}");
            }
        }
    }

    fn interior(map: &testmap::TestMap) -> (u32, Vec<u32>) {
        let comps = map.nav.components();
        let (sx, sz) = map.level.player_start;
        let start = V3::new(sx, map.level.player_start_y, sz);
        let n = map.nav.by_distance(start)[0].1;
        (comps[n], comps)
    }

    /// Players spread over the inside of the map, upstairs and down, with
    /// every door open; zombies start on every third interior node.
    #[test]
    #[ignore = "needs a World at War install"]
    fn nacht_zombies_reach_the_player_anywhere() {
        let map = testmap::get();
        let solids = map.level.player_colliders();
        let open = vec![true; map.level.doors.len()];
        let ctx = NavCtx { mesh: &map.scene.collision, graph: &map.nav, open: &open, solids: &solids };
        let (inside, comps) = interior(map);
        let nodes: Vec<usize> = (0..map.nav.nodes.len()).filter(|i| comps[*i] == inside).collect();
        // Player spots: the inside nodes farthest apart (greedy spread),
        // which covers the start room, the help room and upstairs.
        let mut spots = vec![nodes[0]];
        while spots.len() < 6 {
            let far = *nodes
                .iter()
                .max_by(|a, b| {
                    let d = |i: usize| spots.iter().map(|s| map.nav.nodes[*s].sub(map.nav.nodes[i]).len()).fold(f32::MAX, f32::min);
                    d(**a).total_cmp(&d(**b))
                })
                .unwrap();
            spots.push(far);
        }
        let dump = std::env::var_os("NAV_DUMP").map(std::path::PathBuf::from);
        // `SIM_SPEED`: walking speed (default: the slowest walk, 1.1 m/s).
        let speed: f32 = std::env::var("SIM_SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1.1);
        let (mut total, mut ok, mut worst_dy, mut rescues) = (0, 0, 0.0f32, 0);
        for (k, &s) in spots.iter().enumerate() {
            let player = ctx.node(s) + Vec3::new(0.4, 0.0, 0.3);
            let player = Vec3::new(player.x, navgraph::ground(&map.scene.collision, player.x, player.z, player.y + 0.5, player.y - 1.0).unwrap_or(player.y), player.z);
            let mut times = Vec::new();
            let only: Option<usize> = std::env::var("SIM_ONLY").ok().and_then(|v| v.parse().ok());
            for &n in nodes.iter().step_by(3).filter(|n| only.is_none_or(|o| o == **n)) {
                let o = chase(&ctx, ctx.node(n), player, speed, 120.0);
                total += 1;
                worst_dy = worst_dy.max(o.max_dy);
                rescues += o.rescues;
                if o.reached {
                    ok += 1;
                    times.push(o.time);
                } else {
                    println!("  FAIL from node {n} {:?} to {player:?}: ended at {:?} after {:.0}s", ctx.node(n), o.end, o.time);
                    if let Some(d) = &dump {
                        let s: String = o.trace.iter().map(|p| format!("{} {} {}\n", p.x, p.y, p.z)).collect();
                        std::fs::write(d.join(format!("fail_{k}_{n}.txt")), s).unwrap();
                    }
                }
                if o.max_dy > 0.06 {
                    println!("  height step {:.3} at {:?} (from node {n})", o.max_dy, o.max_dy_at);
                }
                if o.rescues > 0 {
                    println!("  rescued x{} from node {n} {:?} to {player:?}", o.rescues, ctx.node(n));
                    if let Some(d) = &dump {
                        let s: String = o.trace.iter().map(|p| format!("{} {} {}\n", p.x, p.y, p.z)).collect();
                        std::fs::write(d.join(format!("rescue_{k}_{n}.txt")), s).unwrap();
                    }
                }
            }
            times.sort_by(f32::total_cmp);
            println!(
                "player at node {s} {player:?}: {}/{} reached, median {:.1}s, slowest {:.1}s",
                times.len(),
                nodes.len().div_ceil(3),
                times.get(times.len() / 2).copied().unwrap_or(0.0),
                times.last().copied().unwrap_or(0.0)
            );
        }
        println!("total {ok}/{total} reached, rescues {rescues}, max height change per 1/60 s frame {worst_dy:.3} m");
        assert_eq!(ok, total);
        // A 1.2 m rise per metre walked at most (stairs are ~0.7), eased.
        assert!(worst_dy <= ((1.2 * speed + 1.0) / 60.0) + 1e-3, "height must ease, not snap ({worst_dy})");
    }
}
