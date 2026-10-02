//! Dismemberment, as World at War's scripts do it.
//!
//! The models come from the zombies' character scripts (see
//! `waw_assets::character`): a gib swaps the whole body for an upper body
//! (`torsoDmg1..5`: clean, right arm off, left arm off, guts, beheaded) plus
//! a lower body (`legDmg1..4`: clean, right leg off, left leg off, no legs),
//! and launches the severed part (`gibSpawn1..4`) from its joint with a
//! blood trail. A popped head is swapped for the neck stump model.
//!
//! When (`maps/_zombiemode_spawner*.gsc` `zombie_gib_on_damage`,
//! `animscripts/death.gsc`):
//! * a hit that takes 10 % or more of what health was left, from anything
//!   but melee or a pistol-class bullet, can gib:
//!   - the head pops if the zombie is at 10 % health or less and the hit was
//!     to the head, helmet or neck (a grenade within 55 units of the head,
//!     a projectile within 10), and the zombie then bleeds 20 % of its
//!     remaining health a second;
//!   - otherwise, once per zombie, a part by hit location (torso: guts or
//!     right arm; an arm: that arm; a leg only on the killing hit;
//!     explosions: the nearest joint's part), which on a living zombie
//!     happens at once and a lost leg makes it crawl;
//! * a killing bullet that didn't gib that way: 75 % (shotguns 100/75/50 %
//!   by distance) if it did 50+ damage, at most one every 3 s, a part by hit
//!   location, the head for head shots;
//! * a killing explosion of 165+ damage: any part, the same 3 s limit.

use crate::audio::PlayAlias;
use crate::nacht::{Joint, ZombieModels};
use crate::zombies::{Hitboxes, Zombie, ZombieRig};
use crate::{ActivePowerups, Dynamic, PointsEvent, Score, World, ZombieKilled};
use bevy::prelude::*;
use zm_core::weapons::HitLoc;

/// What hurt a zombie.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HurtBy {
    Bullet { pistol: bool, shotgun: bool },
    Explosive,
    Projectile,
}

/// A zombie took damage (sent wherever damage is dealt).
#[derive(Event, Debug, Clone)]
pub struct ZombieHurt {
    pub zombie: Entity,
    pub amount: f32,
    /// Health after the hit (<= 0 if it killed).
    pub hp_after: f32,
    pub by: HurtBy,
    /// Hit location (`None` for explosions).
    pub loc: Option<HitLoc>,
    /// Where the damage came from and went.
    pub from: Vec3,
    pub point: Vec3,
    pub dir: Vec3,
}

/// A part a zombie can lose (`gib_ref`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gib {
    RightArm,
    LeftArm,
    RightLeg,
    LeftLeg,
    NoLegs,
    Guts,
    /// Head off on death (the beheaded upper body).
    Head,
}

impl Gib {
    const ALL: [Gib; 6] = [Gib::RightArm, Gib::LeftArm, Gib::RightLeg, Gib::LeftLeg, Gib::NoLegs, Gib::Guts];

    /// (upper body, lower body, severed parts) by `torsoDmg`/`legDmg`/
    /// `gibSpawn` index (`get_limb_data`).
    fn models(self) -> (usize, usize, &'static [usize]) {
        match self {
            Gib::RightArm => (1, 0, &[0]),
            Gib::LeftArm => (2, 0, &[1]),
            Gib::RightLeg => (0, 1, &[2]),
            Gib::LeftLeg => (0, 2, &[3]),
            Gib::NoLegs => (0, 3, &[3, 2]),
            Gib::Guts => (3, 0, &[]),
            Gib::Head => (4, 0, &[]),
        }
    }

    pub fn loses_legs(self) -> bool {
        matches!(self, Gib::RightLeg | Gib::LeftLeg | Gib::NoLegs)
    }
}

/// A real zombie's model parts and what it has lost.
#[derive(Component)]
pub struct ZombieBody {
    pub character: usize,
    /// Parent of the meshes.
    pub model: Entity,
    pub body_meshes: Vec<Entity>,
    pub head_meshes: Vec<Entity>,
    /// The head model worn (index into the character's heads).
    pub head: Option<usize>,
    pub gibbed: Option<Gib>,
    pub head_gone: bool,
}

impl ZombieBody {
    pub fn new(character: usize, model: Entity, head: Option<usize>, body_meshes: Vec<Entity>, head_meshes: Vec<Entity>) -> ZombieBody {
        ZombieBody { character, model, body_meshes, head_meshes, head, gibbed: None, head_gone: false }
    }
}

/// The 3 s between killing-shot gibs (`anim.gibDelay`) and the explosion
/// gib budget (`anim.totalGibs`, 2..4 per window).
#[derive(Resource)]
struct GibClock {
    last: f32,
    budget: u32,
}

impl Default for GibClock {
    fn default() -> Self {
        GibClock { last: -10.0, budget: 3 }
    }
}

const GIB_DELAY: f32 = 3.0;
const INCH: f32 = 0.0254;

/// Severed parts in flight (`CreateDynEntAndLaunch`).
#[derive(Component)]
struct FlyingGib {
    vel: Vec3,
    spin: Vec3,
    age: f32,
}

/// Head-popped zombies bleed out (`damage_over_time`).
#[derive(Component)]
pub struct Bleeding {
    per_second: f32,
    t: f32,
}

pub struct GibsPlugin;

impl Plugin for GibsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ZombieHurt>()
            .init_resource::<GibClock>()
            .add_systems(Update, (on_hurt, fly, bleed).chain().run_if(in_state(crate::GameState::Playing)))
            .add_systems(Update, test_gib.run_if(|| std::env::var_os("UNDEAD_TEST_GIB").is_some()));
    }
}

/// The joint named `name` (lower case) of a rig.
fn joint<'a>(joints: &'a [Joint], name: &str) -> Option<&'a Joint> {
    joints.iter().find(|j| j.0.eq_ignore_ascii_case(name))
}

/// The part to lose for a hit location (`zombie_gib_on_damage`).
fn refs_alive(loc: Option<HitLoc>, lethal: bool) -> Vec<Gib> {
    use HitLoc::*;
    match loc {
        Some(TorsoUpper | TorsoLower) => vec![Gib::Guts, Gib::RightArm],
        Some(RightArmUpper | RightArmLower | RightHand) => vec![Gib::RightArm],
        Some(LeftArmUpper | LeftArmLower | LeftHand) => vec![Gib::LeftArm],
        Some(RightLegUpper | RightLegLower | RightFoot) if lethal => vec![Gib::RightLeg, Gib::RightLeg, Gib::RightLeg, Gib::NoLegs],
        Some(LeftLegUpper | LeftLegLower | LeftFoot) if lethal => vec![Gib::LeftLeg, Gib::LeftLeg, Gib::LeftLeg, Gib::NoLegs],
        Some(RightLegUpper | RightLegLower | RightFoot | LeftLegUpper | LeftLegLower | LeftFoot) => vec![],
        Some(Head | Helmet | Neck) => vec![],
        Some(None | Gun) => Gib::ALL.to_vec(),
        Option::None => vec![],
    }
}

/// The part to lose on a killing bullet (`play_bulletgibbed_death_anim`).
fn refs_death(loc: Option<HitLoc>) -> Vec<Gib> {
    use HitLoc::*;
    match loc {
        Some(TorsoUpper | TorsoLower) => vec![Gib::Guts, Gib::RightArm, Gib::LeftArm],
        Some(RightArmUpper | RightArmLower | RightHand) => vec![Gib::RightArm],
        Some(LeftArmUpper | LeftArmLower | LeftHand) => vec![Gib::LeftArm],
        Some(RightLegUpper | RightLegLower | RightFoot) => vec![Gib::RightLeg, Gib::NoLegs],
        Some(LeftLegUpper | LeftLegLower | LeftFoot) => vec![Gib::LeftLeg, Gib::NoLegs],
        Some(Head | Helmet) => vec![Gib::Head],
        _ => vec![],
    }
}

/// The part nearest an explosion (`derive_damage_refs`), by the joints the
/// script checks.
fn refs_near(joints: &[Joint], globals: &Query<&GlobalTransform>, point: Vec3) -> Vec<Gib> {
    const TAGS: [(&str, &[Gib]); 15] = [
        ("j_spinelower", &[Gib::Guts, Gib::RightArm]),
        ("j_spineupper", &[Gib::Guts, Gib::RightArm]),
        ("j_spine4", &[Gib::Guts, Gib::RightArm]),
        ("j_shoulder_le", &[Gib::LeftArm]),
        ("j_elbow_le", &[Gib::LeftArm]),
        ("j_wrist_le", &[Gib::LeftArm]),
        ("j_shoulder_ri", &[Gib::RightArm]),
        ("j_elbow_ri", &[Gib::RightArm]),
        ("j_wrist_ri", &[Gib::RightArm]),
        ("j_hip_le", &[Gib::LeftLeg, Gib::NoLegs]),
        ("j_knee_le", &[Gib::LeftLeg, Gib::NoLegs]),
        ("j_ankle_le", &[Gib::LeftLeg, Gib::NoLegs]),
        ("j_hip_ri", &[Gib::RightLeg, Gib::NoLegs]),
        ("j_knee_ri", &[Gib::RightLeg, Gib::NoLegs]),
        ("j_ankle_ri", &[Gib::RightLeg, Gib::NoLegs]),
    ];
    TAGS.iter()
        .filter_map(|(tag, refs)| joint(joints, tag).and_then(|j| globals.get(j.1).ok()).map(|g| (g.translation().distance_squared(point), *refs)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, r)| r.to_vec())
        .unwrap_or_default()
}

/// A random number in `a..b`.
fn rand_range(a: f32, b: f32) -> f32 {
    a + (b - a) * fastrand::f32()
}

fn pick(refs: &[Gib]) -> Option<Gib> {
    (!refs.is_empty()).then(|| refs[fastrand::usize(..refs.len())])
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn on_hurt(
    time: Res<Time>,
    mut events: EventReader<ZombieHurt>,
    mut clock: ResMut<GibClock>,
    models: Option<Res<ZombieModels>>,
    mut zq: Query<(&Transform, &mut Zombie, &mut ZombieRig, &mut ZombieBody, &mut Hitboxes)>,
    globals: Query<&GlobalTransform>,
    mut commands: Commands,
    mut fx: EventWriter<crate::fx::FxEvent>,
    mut alias: EventWriter<PlayAlias>,
) {
    let Some(models) = models else { return };
    let now = time.elapsed_secs();
    for ev in events.read() {
        let Ok((t, mut z, mut rig, mut body, mut boxes)) = zq.get_mut(ev.zombie) else { continue };
        let lethal = ev.hp_after <= 0.0;
        let max = z.max_hp.max(1.0);
        let head_hit = ev.loc.is_some_and(|l| l.is_head() || l == HitLoc::Neck);
        let head_dist = joint(&rig.joints, "j_head").and_then(|j| globals.get(j.1).ok()).map(|g| g.translation().distance(ev.point) / INCH);

        // zombie_should_gib: not a pistol bullet (melee never reports), 10 %+
        // of what was left.
        let class_ok = !matches!(ev.by, HurtBy::Bullet { pistol: true, .. });
        let prev = (ev.amount + ev.hp_after.max(0.0)).max(1.0);
        let mut done = false;
        if class_ok && ev.amount / prev >= 0.10 {
            // head_should_gib
            let head_ok = match ev.by {
                HurtBy::Bullet { .. } => head_hit,
                HurtBy::Explosive => head_dist.is_some_and(|d| d <= 55.0),
                HurtBy::Projectile => head_dist.is_some_and(|d| d <= 10.0),
            };
            if !body.head_gone && ev.hp_after / max <= 0.10 && head_ok {
                pop_head(&mut commands, &models, &mut rig, &mut body, &mut boxes, &globals, &mut fx, &mut alias, ev.zombie);
                if !lethal {
                    commands.entity(ev.zombie).insert(Bleeding { per_second: ev.hp_after * 0.2, t: 0.0 });
                }
                done = true;
            } else if body.gibbed.is_none() && !head_hit {
                let refs = match (ev.loc, ev.by) {
                    (None, HurtBy::Explosive | HurtBy::Projectile) => refs_near(&rig.joints, &globals, ev.point),
                    (loc, _) => refs_alive(loc, lethal),
                };
                // On a living zombie the part goes now (a killing hit falls
                // through to the death roll below, as in death.gsc).
                if let (Some(g), false) = (pick(&refs), lethal) {
                    gib(&mut commands, &models, &mut rig, &mut body, &mut boxes, &globals, &mut fx, &mut alias, ev, g, t.translation);
                    if g.loses_legs() {
                        // One of the three crawls, at its own pace.
                        let which = fastrand::usize(..zm_core::rules::NACHT_CRAWLS.len());
                        let speed = models.clip(zm_core::rules::NACHT_CRAWLS[which].0).map_or(0.3, |c| models.clips[c].root_speed) * z.scale;
                        z.start_crawling(g, which, speed);
                    }
                    done = true;
                }
            }
        }
        if !lethal || done || body.gibbed.is_some() {
            continue;
        }
        // The death roll (death.gsc).
        let ready = now > clock.last + GIB_DELAY;
        let dist = ev.from.distance(t.translation) / INCH;
        let g = match ev.by {
            HurtBy::Bullet { shotgun, .. } => {
                let chance = if shotgun {
                    match dist {
                        d if d < 110.0 => 100,
                        d if d < 200.0 => 75,
                        d if d < 270.0 => 50,
                        d if d < 330.0 && fastrand::bool() => 50,
                        _ => 0,
                    }
                } else {
                    75
                };
                let strong = shotgun || ev.amount >= 50.0;
                (ready && strong && fastrand::u32(..100) < chance).then(|| pick(&refs_death(ev.loc))).flatten()
            }
            HurtBy::Explosive | HurtBy::Projectile => {
                let ok = ready && ev.amount >= 165.0 && clock.budget > 0;
                if ok {
                    clock.budget -= 1;
                }
                ok.then(|| pick(&Gib::ALL)).flatten()
            }
        };
        if let Some(g) = g {
            clock.last = now;
            if clock.budget == 0 {
                clock.budget = fastrand::u32(2..4);
            }
            gib(&mut commands, &models, &mut rig, &mut body, &mut boxes, &globals, &mut fx, &mut alias, ev, g, t.translation);
        }
    }
}

/// Swaps the body for the gib's upper and lower bodies and throws the
/// severed parts (`do_gib`).
#[allow(clippy::too_many_arguments)]
fn gib(
    commands: &mut Commands,
    models: &ZombieModels,
    rig: &mut ZombieRig,
    body: &mut ZombieBody,
    boxes: &mut Hitboxes,
    globals: &Query<&GlobalTransform>,
    fx: &mut EventWriter<crate::fx::FxEvent>,
    alias: &mut EventWriter<PlayAlias>,
    ev: &ZombieHurt,
    g: Gib,
    origin: Vec3,
) {
    let Some(ch) = models.chars.get(body.character) else { return };
    let (ti, li, spawns) = g.models();
    debug!("gib {g:?} ({:?}, {:.0} damage, {:.0} health left)", ev.by, ev.amount, ev.hp_after);
    let (Some(torso), Some(legs)) = (ch.torso[ti].get(fastrand::usize(..ch.torso[ti].len().max(1))), ch.legs[li].get(fastrand::usize(..ch.legs[li].len().max(1)))) else {
        return;
    };
    body.gibbed = Some(g);
    for m in body.body_meshes.drain(..) {
        commands.entity(m).try_despawn();
    }
    body.body_meshes = crate::nacht::spawn_part(commands, torso, &mut rig.joints, body.model, body.model);
    body.body_meshes.extend(crate::nacht::spawn_part(commands, legs, &mut rig.joints, body.model, body.model));
    if g == Gib::Head {
        for m in body.head_meshes.drain(..) {
            commands.entity(m).try_despawn();
        }
        body.head_gone = true;
    }
    // Hit boxes follow the parts now worn.
    boxes.0.clear();
    boxes.add(torso, &rig.joints);
    boxes.add(legs, &rig.joints);
    if !body.head_gone {
        if let Some(head) = body.head.and_then(|h| ch.heads.get(h)) {
            boxes.add(head, &rig.joints);
        }
    }
    alias.write(PlayAlias::at("death_gibs", origin));
    // Severed parts fly from their joints, trailing blood; the gib effect
    // plays on the joint.
    let base_vel = (ev.dir * rand_range(500.0, 900.0) + Vec3::new(rand_range(-600.0, 600.0), rand_range(400.0, 1000.0), rand_range(-600.0, 600.0))) * INCH;
    for &si in spawns {
        let Some((parts, tag)) = &ch.spawns[si] else { continue };
        let Some(j) = joint(&rig.joints, tag) else { continue };
        let Ok(at) = globals.get(j.1) else { continue };
        fx.write(crate::fx::FxEvent::Play { name: "animscript_gib_fx".into(), pos: at.translation(), forward: ev.dir, attach: Some(j.1) });
        let Some(part) = parts.get(fastrand::usize(..parts.len().max(1))) else { continue };
        let root = commands
            .spawn((
                Transform::from_matrix(at.compute_matrix()),
                Visibility::default(),
                FlyingGib { vel: base_vel, spin: Vec3::new(rand_range(-8.0, 8.0), rand_range(-8.0, 8.0), rand_range(-8.0, 8.0)), age: 0.0 },
                Dynamic,
            ))
            .id();
        let mut joints = Vec::new();
        crate::nacht::spawn_part(commands, part, &mut joints, root, root);
        fx.write(crate::fx::FxEvent::Play { name: "animscript_gibtrail_fx".into(), pos: at.translation(), forward: Vec3::Y, attach: Some(root) });
    }
    if g == Gib::Guts {
        if let Some(j) = joint(&rig.joints, "j_spinelower").and_then(|j| globals.get(j.1).ok()) {
            fx.write(crate::fx::FxEvent::Play { name: "animscript_gib_fx".into(), pos: j.translation(), forward: ev.dir, attach: None });
        }
    }
}

/// Pops the head: the neck stump replaces it (`zombie_head_gib`).
#[allow(clippy::too_many_arguments)]
fn pop_head(
    commands: &mut Commands,
    models: &ZombieModels,
    rig: &mut ZombieRig,
    body: &mut ZombieBody,
    boxes: &mut Hitboxes,
    globals: &Query<&GlobalTransform>,
    fx: &mut EventWriter<crate::fx::FxEvent>,
    alias: &mut EventWriter<PlayAlias>,
    zombie: Entity,
) {
    let Some(ch) = models.chars.get(body.character) else { return };
    debug!("head popped");
    body.head_gone = true;
    for m in body.head_meshes.drain(..) {
        commands.entity(m).try_despawn();
    }
    if let Some(stump) = &ch.behead {
        body.head_meshes = crate::nacht::spawn_part(commands, stump, &mut rig.joints, body.model, body.model);
    }
    // The head's boxes go with it.
    let head_joints: Vec<Entity> = ["j_head", "j_head_end", "j_helmet"].iter().filter_map(|n| joint(&rig.joints, n).map(|j| j.1)).collect();
    boxes.0.retain(|(e, _)| !head_joints.contains(e));
    alias.write(PlayAlias::on("zombie_head_gib", zombie));
    if let Some(neck) = joint(&rig.joints, "j_neck") {
        let (pos, fwd) = globals.get(neck.1).map(|g| (g.translation(), g.forward().as_vec3())).unwrap_or_default();
        for name in ["headshot", "headshot_nochunks"] {
            fx.write(crate::fx::FxEvent::Play { name: name.into(), pos, forward: fwd, attach: None });
        }
        fx.write(crate::fx::FxEvent::Play { name: "bloodspurt".into(), pos, forward: Vec3::Y, attach: Some(neck.1) });
    }
}

/// Severed parts: thrown, spinning, bouncing to rest on the floor, gone
/// after a while.
fn fly(time: Res<Time>, world: Res<World>, mut commands: Commands, mut q: Query<(Entity, &mut Transform, &mut FlyingGib)>) {
    let dt = time.delta_secs().min(0.05);
    for (e, mut t, mut g) in &mut q {
        g.age += dt;
        if g.age > 20.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        if g.vel.length_squared() < 1e-4 && g.spin.length_squared() < 1e-4 {
            continue;
        }
        g.vel.y -= 800.0 * INCH * dt;
        let next = t.translation + g.vel * dt;
        let floor = crate::zombies::ground_y(&world, next.x, next.z, t.translation.y + 0.2);
        if next.y <= floor + 0.03 {
            t.translation = Vec3::new(next.x, floor + 0.03, next.z);
            g.vel = Vec3::new(g.vel.x * 0.5, (-g.vel.y * 0.3).max(0.0), g.vel.z * 0.5);
            g.spin *= 0.5;
            if g.vel.length() < 0.3 {
                g.vel = Vec3::ZERO;
                g.spin = Vec3::ZERO;
            }
        } else {
            t.translation = next;
        }
        let spin = g.spin * dt;
        t.rotate(Quat::from_euler(EulerRot::XYZ, spin.x, spin.y, spin.z));
    }
}

/// A popped head bleeds the zombie out, the player getting the kill.
#[allow(clippy::too_many_arguments)]
fn bleed(
    time: Res<Time>,
    mut q: Query<(Entity, &Transform, &mut Zombie, &mut Bleeding)>,
    mut killed: EventWriter<ZombieKilled>,
    (mut score, mut points, pu): (ResMut<Score>, EventWriter<PointsEvent>, Res<ActivePowerups>),
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (e, t, mut z, mut b) in &mut q {
        if !z.alive() {
            commands.entity(e).remove::<Bleeding>();
            continue;
        }
        b.t += dt;
        while b.t >= 1.0 {
            b.t -= 1.0;
            if crate::zombies::apply_damage(&mut z, b.per_second.max(1.0), false) {
                crate::earn(&mut score, &mut points, &pu, zm_core::rules::kill_points(zm_core::rules::KillKind::Body));
                killed.write(ZombieKilled { pos: t.translation, drop_allowed: true });
                break;
            }
        }
    }
}

/// Developer aid: `UNDEAD_TEST_GIB=right_arm|left_arm|right_leg|left_leg|
/// no_legs|guts|head|head_pop` gibs the preview zombie (`UNDEAD_PREVIEW`)
/// 3 s in, to look at the models and the thrown parts.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn test_gib(
    time: Res<Time>,
    mut done: Local<bool>,
    models: Option<Res<ZombieModels>>,
    mut q: Query<(Entity, &Transform, &mut ZombieRig, &mut ZombieBody, &mut Hitboxes), With<crate::zombies::Preview>>,
    globals: Query<&GlobalTransform>,
    mut commands: Commands,
    mut fx: EventWriter<crate::fx::FxEvent>,
    mut alias: EventWriter<PlayAlias>,
) {
    let (Some(models), false) = (models, *done) else { return };
    let Ok(kind) = std::env::var("UNDEAD_TEST_GIB") else { return };
    let Some((e, t, mut rig, mut body, mut boxes)) = q.iter_mut().next() else { return };
    if time.elapsed_secs() < 3.0 {
        return;
    }
    *done = true;
    let g = match kind.as_str() {
        "right_arm" => Gib::RightArm,
        "left_arm" => Gib::LeftArm,
        "right_leg" => Gib::RightLeg,
        "left_leg" => Gib::LeftLeg,
        "no_legs" => Gib::NoLegs,
        "guts" => Gib::Guts,
        "head" => Gib::Head,
        _ => {
            pop_head(&mut commands, &models, &mut rig, &mut body, &mut boxes, &globals, &mut fx, &mut alias, e);
            return;
        }
    };
    let ev = ZombieHurt { zombie: e, amount: 100.0, hp_after: 0.0, by: HurtBy::Bullet { pistol: false, shotgun: true }, loc: None, from: t.translation, point: t.translation, dir: t.forward().as_vec3() };
    gib(&mut commands, &models, &mut rig, &mut body, &mut boxes, &globals, &mut fx, &mut alias, &ev, g, t.translation);
}
