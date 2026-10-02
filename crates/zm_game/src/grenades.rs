//! Hand grenades (Nacht's Stielhandgranate): the allowance per round,
//! pulling the pin and cooking, the first-person throw, the bouncing
//! projectile and the explosion.
//!
//! Stats come from the install's weapon file (`weapons/sp/stielhandgranate`);
//! the viewmodel, its animations, the projectile model and the sounds come
//! from the zone (loaded with the weapons, see `nacht::start_load`). Without
//! an install, built-in stats and simple stand-in models are used.

use crate::audio::{PlayAlias, Sfx, ZoneSounds};
use crate::player::{self, Health, Player, PlayerCtl};
use crate::weapons::{spawn_burst, Gun, GunModel, ViewModel};
use crate::world::Mats;
use crate::zombies::{self, Zombie};
use crate::{earn, ActivePowerups, Dynamic, GameState, PointsEvent, Round, Score, World, ZombieKilled};
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use zm_core::geom::V3;
use zm_core::grenade::{self, GrenadeDef};
use zm_core::rules::{self, KillKind};

/// The zone/weapon-file name of the grenade.
pub const GRENADE_ID: &str = "stielhandgranate";
/// Sound aliases the grenade plays besides its weapon fields: the explosion
/// on an unknown surface (a silent layer whose secondary is the blast and
/// its distant/bass/trail layers) and the default bounce.
pub const ALIASES: &[&str] = &["grenade_explode_default", "grenade_bounce_default"];
/// The HUD icon (`hudIcon` of the weapon).
pub const HUD_ICON: &str = "hud_us_grenade";

/// Game units (inches) to metres.
const U: f32 = 0.0254;
/// `g_gravity` (800 units/s²).
const GRAVITY: f32 = 800.0 * U;
/// Collision radius of the flying grenade.
const PROJ_RADIUS: f32 = 0.04;
/// A grenade in hand explodes this far in front of the eye.
const HAND: Vec3 = Vec3::new(0.12, -0.12, -0.35);

pub struct GrenadesPlugin;

impl Plugin for GrenadesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Grenades>()
            .add_systems(Startup, load_stats)
            .add_systems(OnEnter(GameState::Loading), reset)
            .add_systems(OnEnter(GameState::MainMenu), reset)
            .add_systems(Update, (award, throw_input, fly).chain().run_if(in_state(GameState::Playing)))
            .add_systems(Update, (view_rig, fade_fx))
            .add_systems(
                PostUpdate,
                shake.run_if(in_state(GameState::Playing)).before(bevy::transform::TransformSystem::TransformPropagate),
            );
    }
}

/// Grenade stats (weapon file, else built-in).
#[derive(Resource, Clone)]
pub struct GrenadeStats(pub GrenadeDef);

/// The player's grenades and the throw in progress.
#[derive(Resource, Default)]
pub struct Grenades {
    pub count: u32,
    /// Last round whose allowance was given.
    awarded_round: u32,
    pub throw: Throw,
    /// Screen flash from a nearby explosion (0..1), drawn by the HUD.
    pub flash: f32,
    /// Camera shake strength (0..1).
    shake: f32,
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub enum Throw {
    #[default]
    Idle,
    /// Pin pulled `t` seconds ago (cooking); `release` once the key is up.
    Hold { t: f32, release: bool },
    /// Thrown `t` seconds ago (the throw animation).
    Throwing { t: f32 },
}

/// A grenade in flight or at rest.
#[derive(Component)]
struct Projectile {
    vel: Vec3,
    fuse: f32,
    spin: Vec3,
    resting: bool,
    bounces: u32,
}

/// The first-person arms holding the grenade during a throw.
#[derive(Component)]
struct GrenadeRig {
    joints: Vec<crate::nacht::Joint>,
    posed: bool,
    /// Parent of the grenade's meshes.
    held: Entity,
}

/// The grenade model in the first-person hand.
#[derive(Component)]
struct HeldGrenade;

/// Stand-in grenade in hand (no game viewmodel).
#[derive(Component)]
struct HandGrenade;

/// Explosion visuals that grow and fade (fireball, smoke, light).
#[derive(Component)]
struct Blast {
    t: f32,
    ttl: f32,
    size: f32,
    grow: f32,
    light: f32,
}

fn load_stats(mut commands: Commands, waw: Res<crate::waw::Waw>) {
    let mut def = GrenadeDef::default();
    match waw.read(&format!("weapons/sp/{GRENADE_ID}")).and_then(|b| zm_core::weaponfile::WeaponFile::parse_bytes(&b)) {
        Some(wf) => {
            let n = def.apply_weapon_file(&wf);
            info!(
                "grenade: applied {n} stats from weapons/sp/{GRENADE_ID}: fuse {}s, radius {}, damage {}..{}, speed {}+{} up, max {}, cook {}",
                def.fuse, def.radius, def.inner_damage, def.outer_damage, def.speed, def.speed_up, def.max_ammo, def.cook
            );
        }
        None => info!("grenade: no weapon file, using built-in stats"),
    }
    commands.insert_resource(GrenadeStats(def));
}

fn reset(mut g: ResMut<Grenades>, mut commands: Commands, rigs: Query<Entity, Or<(With<GrenadeRig>, With<HandGrenade>)>>, mut guns: Query<&mut Visibility, With<GunModel>>) {
    *g = Grenades::default();
    for e in &rigs {
        commands.entity(e).try_despawn();
    }
    for mut v in &mut guns {
        *v = Visibility::Inherited;
    }
}

/// Round start: every survivor's grenades are topped up (the round-1 call
/// gives the first two).
fn award(round: Res<Round>, stats: Res<GrenadeStats>, mut g: ResMut<Grenades>) {
    let r = round.0.round;
    if r > g.awarded_round && !round.0.in_intermission() {
        g.awarded_round = r;
        let before = g.count;
        g.count = grenade::award_for_survivor(g.count, stats.0.max_ammo);
        info!("grenade: round {r} allowance {before} -> {}", g.count);
    }
}

/// Throw direction for the player's view.
fn view_dir(ctl: &PlayerCtl) -> (Vec3, Vec3, Vec3) {
    let q = Quat::from_euler(EulerRot::YXZ, ctl.yaw, (ctl.pitch + ctl.recoil).clamp(-1.55, 1.55), 0.0);
    (q * Vec3::NEG_Z, q * Vec3::X, q * Vec3::Y)
}

/// Developer aid: `UNDEAD_TEST_GRENADE=1` throws (after a 1 s cook) at
/// zombies 4-15 m away; `UNDEAD_TEST_GRENADE_AT=5,12` throws at those
/// seconds wherever the player looks. `UNDEAD_TEST_GRENADE_COOK` sets the
/// cook time.
#[derive(Default)]
struct TestDriver {
    next: f32,
    hold_until: Option<f32>,
    target: Option<Vec3>,
    timed: usize,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn throw_input(
    (time, keys, mouse, windows): (Res<Time>, Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, Query<&Window, With<PrimaryWindow>>),
    (stats, mut g, mut gun, world): (Res<GrenadeStats>, ResMut<Grenades>, ResMut<Gun>, Res<World>),
    mut player: Query<(&Transform, &mut PlayerCtl), With<Player>>,
    mut test: Local<TestDriver>,
    mut commands: Commands,
    (mats, nacht): (Res<Mats>, Option<Res<crate::nacht::NachtAssets>>),
    mut boom: ExplodeParams,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    let def = &stats.0;
    let Ok((cam, mut ctl)) = player.single_mut() else { return };
    let mut pressed = crate::cursor_locked(&windows) && (keys.just_pressed(KeyCode::KeyG) || mouse.just_pressed(MouseButton::Back));
    let mut held = keys.pressed(KeyCode::KeyG) || mouse.pressed(MouseButton::Back);

    // Smoke-test driver.
    if crate::autopilot_enabled() {
        let cook: f32 = std::env::var("UNDEAD_TEST_GRENADE_COOK").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
        let at: Vec<f32> =
            std::env::var("UNDEAD_TEST_GRENADE_AT").ok().map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
        if test.hold_until.is_none() && g.throw == Throw::Idle && g.count > 0 {
            if at.get(test.timed).is_some_and(|t| now >= *t) {
                test.timed += 1;
                test.hold_until = Some(now + cook);
                test.target = None;
                info!("[grenade-test] timed throw at {now:.1}s (cook {cook}s)");
            } else if std::env::var_os("UNDEAD_TEST_GRENADE").is_some() && now >= test.next {
                let eye = cam.translation;
                let target = boom
                    .zq
                    .iter()
                    .filter(|(_, _, z)| z.alive() && matches!(z.state, zombies::ZState::Chase | zombies::ZState::AtWindow | zombies::ZState::Climbing))
                    .map(|(_, t, _)| t.translation)
                    .filter(|p| (4.0..15.0).contains(&p.distance(eye)) && line_clear(&world, eye, *p + Vec3::Y))
                    .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
                if let Some(p) = target {
                    test.hold_until = Some(now + cook);
                    test.target = Some(p);
                    test.next = now + 6.0;
                    info!("[grenade-test] throwing at zombie {p:?} ({:.1} m, cook {cook}s)", p.distance(eye));
                }
            }
        }
        if let Some(until) = test.hold_until {
            pressed |= g.throw == Throw::Idle;
            held = now < until;
            if !held {
                test.hold_until = None;
            }
        }
    }

    match g.throw {
        Throw::Idle => {
            if pressed && g.count > 0 && gun.knife_anim <= 0.0 {
                g.count -= 1;
                g.throw = Throw::Hold { t: 0.0, release: false };
                gun.reload = None;
                gun.switch = 0.45;
                if let Some(a) = crate::weapons::weapon_sound(&boom.zs, GRENADE_ID, "pullbackSoundPlayer", Sfx::Reload) {
                    boom.alias.write(a);
                }
            }
        }
        Throw::Hold { t, release } => {
            let t = t + dt;
            let release = release || !held;
            gun.switch = 0.45;
            if def.cook && t >= def.fuse {
                // Cooked too long: it goes off in the hand.
                let (fwd, right, up) = view_dir(&ctl);
                let at = cam.translation + right * HAND.x + up * HAND.y - fwd * HAND.z;
                info!("grenade: exploded in hand after {t:.2}s");
                explode(at, def, &mut boom, &mut commands, &mats, &mut g, cam.translation, ctl.feet_y);
                g.throw = Throw::Throwing { t: def.fire_time };
            } else if release && t >= def.hold_time {
                // Test throws aim at the target's feet.
                if let Some(p) = test.target.take() {
                    let d = (p - cam.translation).normalize_or_zero();
                    ctl.yaw = (-d.x).atan2(-d.z);
                    ctl.pitch = d.y.asin();
                }
                let fuse = if def.cook { def.fuse - t } else { def.fuse };
                let (fwd, right, up) = view_dir(&ctl);
                let vel = fwd * def.speed * U + up * def.speed_up * U;
                // Released in front of the eye unless a wall is right there.
                let eye = cam.translation;
                let ahead = right * 0.08 - up * 0.05 + fwd * 0.3;
                let start = if line_clear(&world, eye, eye + ahead * 1.3) { eye + ahead } else { eye };
                spawn_projectile(&mut commands, &mats, nacht.as_deref(), def, start, vel, fuse);
                info!("grenade: thrown from {start:?} at {:.1} m/s, fuse {fuse:.2}s ({} left)", vel.length(), g.count);
                if let Some(a) = crate::weapons::weapon_sound(&boom.zs, GRENADE_ID, "fireSoundPlayer", Sfx::Knife) {
                    boom.alias.write(a);
                }
                g.throw = Throw::Throwing { t: 0.0 };
            } else {
                g.throw = Throw::Hold { t, release };
            }
        }
        Throw::Throwing { t } => {
            let t = t + dt;
            gun.switch = 0.45;
            if t >= def.fire_time {
                // The weapon comes back up (raise animation).
                g.throw = Throw::Idle;
            } else {
                g.throw = Throw::Throwing { t };
            }
        }
    }
}

fn line_clear(world: &World, a: Vec3, b: Vec3) -> bool {
    let d = b - a;
    let len = d.length();
    len < 1e-4 || crate::weapons::wall_distance(world, a, d / len, len) >= len - 1e-3
}

fn spawn_projectile(commands: &mut Commands, mats: &Mats, nacht: Option<&crate::nacht::NachtAssets>, def: &GrenadeDef, at: Vec3, vel: Vec3, fuse: f32) {
    let model = nacht.and_then(|n| {
        [def.projectile_model.as_deref(), def.world_model.as_deref(), n.weapon_world_models.get(GRENADE_ID).map(String::as_str)]
            .into_iter()
            .flatten()
            .find_map(|name| n.models.get(name))
    });
    // The model's long axis is +X: point it along the throw.
    let rot = Quat::from_rotation_arc(Vec3::X, vel.normalize_or(Vec3::X));
    let spin = Vec3::new(fastrand::f32() - 0.5, fastrand::f32() - 0.5, 1.0).normalize() * 12.0;
    let e = commands
        .spawn((Transform::from_translation(at).with_rotation(rot), Visibility::default(), Projectile { vel, fuse, spin, resting: false, bounces: 0 }, Dynamic))
        .id();
    commands.entity(e).with_children(|p| match model {
        Some(m) => {
            for (mesh, mat) in &m.parts {
                p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), NotShadowCaster));
            }
        }
        None => stand_in_grenade(p, mats),
    });
}

/// A stick grenade from boxes (head along +X, handle behind it).
fn stand_in_grenade(p: &mut ChildSpawnerCommands, mats: &Mats) {
    p.spawn((Mesh3d(mats.cube.clone()), MeshMaterial3d(mats.gun_metal.clone()), Transform::from_xyz(0.1, 0.0, 0.0).with_scale(Vec3::new(0.09, 0.06, 0.06)), NotShadowCaster));
    p.spawn((Mesh3d(mats.cube.clone()), MeshMaterial3d(mats.gun_wood.clone()), Transform::from_xyz(-0.03, 0.0, 0.0).with_scale(Vec3::new(0.2, 0.03, 0.03)), NotShadowCaster));
}

/// Moves grenades: gravity, bounces off the map (keeping the weapon's
/// parallel/perpendicular fractions), resting on floors, the fuse.
#[allow(clippy::too_many_arguments)]
fn fly(
    time: Res<Time>,
    stats: Res<GrenadeStats>,
    world: Res<World>,
    mut q: Query<(Entity, &mut Transform, &mut Projectile), Without<Player>>,
    mut commands: Commands,
    mats: Res<Mats>,
    mut g: ResMut<Grenades>,
    mut boom: ExplodeParams,
    player: Query<(&Transform, &PlayerCtl), With<Player>>,
) {
    let dt = time.delta_secs().min(0.05);
    let def = &stats.0;
    let (eye, feet_y) = player.single().map(|(t, c)| (t.translation, c.feet_y)).unwrap_or_default();
    for (e, mut t, mut p) in &mut q {
        p.fuse -= dt;
        if p.fuse <= 0.0 {
            let at = t.translation;
            info!("grenade: exploded at {at:?} after {} bounces", p.bounces);
            commands.entity(e).try_despawn();
            explode(at + Vec3::Y * 0.05, def, &mut boom, &mut commands, &mats, &mut g, eye, feet_y);
            continue;
        }
        if p.resting {
            continue;
        }
        p.vel.y -= GRAVITY * dt;
        let mut pos = t.translation;
        let mut left = dt;
        // A few sub-steps per frame so fast throws don't tunnel.
        for _ in 0..4 {
            if left <= 1e-5 {
                break;
            }
            let step = p.vel * left;
            let len = step.length();
            if len < 1e-6 {
                break;
            }
            let dir = step / len;
            match surface_hit(&world, pos, dir, len + PROJ_RADIUS) {
                Some((hit_t, mut n)) => {
                    if n.dot(dir) > 0.0 {
                        n = -n;
                    }
                    let travel = (hit_t - PROJ_RADIUS).max(0.0);
                    pos += dir * travel;
                    left *= 1.0 - (travel / len).min(1.0);
                    let into = -p.vel.dot(n);
                    if n.y > 0.7 && into < 0.8 {
                        // Sliding along a floor: lose the push into it, rub.
                        let along = p.vel + n * into;
                        p.vel = along * (1.0 - 4.0 * dt).max(0.0);
                    } else {
                        let v = grenade::bounce(to_v3(p.vel), to_v3(n), def.parallel_bounce, def.perpendicular_bounce);
                        p.vel = Vec3::new(v.x, v.y, v.z);
                        p.bounces += 1;
                    }
                    // Bounce sound on real impacts, not while rolling.
                    if into > 1.5 {
                        if let Some(a) = def.bounce_alias("default") {
                            let vol = ((into - 1.5) / 6.0).clamp(0.3, 1.0);
                            boom.alias.write(PlayAlias::at(a, pos).volume(vol));
                        }
                    }
                    // Settles on a floor once slow.
                    if n.y > 0.7 && p.vel.length() < 0.3 {
                        p.resting = true;
                        p.vel = Vec3::ZERO;
                        pos += n * 0.01;
                        // Lie flat on the floor.
                        let fwd = (t.rotation * Vec3::X).reject_from(n).normalize_or(Vec3::X);
                        t.rotation = Quat::from_rotation_arc(Vec3::X, fwd);
                        break;
                    }
                    pos += n * 0.002;
                }
                None => {
                    pos += step;
                    left = 0.0;
                }
            }
        }
        if !p.resting {
            let spin = p.spin * dt * (p.vel.length() / 10.0).min(1.5);
            t.rotation = Quat::from_scaled_axis(spin) * t.rotation;
        }
        t.translation = pos;
        if pos.y < -50.0 {
            commands.entity(e).try_despawn();
        }
    }
}

fn to_v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

/// First surface along a ray: the map mesh, solid boxes, or (on the
/// prototype map) the floor plane. Returns the distance and the normal.
fn surface_hit(world: &World, o: Vec3, d: Vec3, max: f32) -> Option<(f32, Vec3)> {
    let mut best: Option<(f32, Vec3)> = None;
    if let Some(m) = &world.mesh {
        if let Some(h) = m.raycast(to_v3(o), to_v3(d), max) {
            best = Some((h.t, Vec3::new(h.normal.x, h.normal.y, h.normal.z)));
        }
    } else if d.y < 0.0 && o.y >= 0.0 {
        let t = -o.y / d.y;
        if t <= max {
            best = Some((t, Vec3::Y));
        }
    }
    for s in &world.bullet_solids {
        let Some(t) = s.ray_hit(to_v3(o), to_v3(d), max) else { continue };
        if best.is_some_and(|b| b.0 <= t) {
            continue;
        }
        // Normal of the face hit: the axis where the point is closest to a side.
        let p = o + d * t;
        let (lo, hi) = (Vec3::new(s.min.x, s.min.y, s.min.z), Vec3::new(s.max.x, s.max.y, s.max.z));
        let faces = [(p.x - lo.x, -Vec3::X), (hi.x - p.x, Vec3::X), (p.y - lo.y, -Vec3::Y), (hi.y - p.y, Vec3::Y), (p.z - lo.z, -Vec3::Z), (hi.z - p.z, Vec3::Z)];
        let n = faces.iter().min_by(|a, b| a.0.abs().total_cmp(&b.0.abs())).map(|f| f.1).unwrap_or(Vec3::Y);
        best = Some((t, n));
    }
    best
}

/// What an explosion touches.
#[derive(bevy::ecs::system::SystemParam)]
struct ExplodeParams<'w, 's> {
    world: Res<'w, World>,
    round: Res<'w, Round>,
    pu: Res<'w, ActivePowerups>,
    score: ResMut<'w, Score>,
    health: ResMut<'w, Health>,
    zq: Query<'w, 's, (Entity, &'static Transform, &'static mut Zombie), (Without<Player>, Without<Projectile>)>,
    hurt_ev: EventWriter<'w, crate::gibs::ZombieHurt>,
    points: EventWriter<'w, PointsEvent>,
    killed: EventWriter<'w, ZombieKilled>,
    alias: EventWriter<'w, PlayAlias>,
    next: ResMut<'w, NextState<GameState>>,
    zs: Res<'w, ZoneSounds>,
    fx: EventWriter<'w, crate::fx::FxEvent>,
}

/// Radius damage, sounds and visuals of an explosion at `at`.
#[allow(clippy::too_many_arguments)]
fn explode(at: Vec3, def: &GrenadeDef, b: &mut ExplodeParams, commands: &mut Commands, mats: &Mats, g: &mut Grenades, eye: Vec3, feet_y: f32) {
    let radius = def.radius * U;
    let insta = b.pu.insta_kill > 0.0;
    let round = b.round.0.round.max(1);
    let mut kills = 0;
    let mut hurt = 0;
    for (ze, t, mut z) in b.zq.iter_mut() {
        if !z.alive() {
            continue;
        }
        // Distance to the nearest point of the body, which must be in view
        // of the blast (its middle or its head).
        let feet = t.translation;
        let head = feet + Vec3::Y * 1.63 * z.scale;
        let near = closest_on_segment(at, feet, head);
        let dist = near.distance(at);
        let Some(dmg) = def.damage_at(dist / U) else { continue };
        if !line_clear(&b.world, at, feet + Vec3::Y * 0.9) && !line_clear(&b.world, at, head) {
            continue;
        }
        hurt += 1;
        let before = z.hp;
        let mut dead = zombies::apply_damage(&mut z, dmg, insta);
        if !dead {
            // The zombie damage script follows grenade damage with
            // `round + RandomInt(100, 500)`; RandomInt takes one argument.
            dead = zombies::apply_damage(&mut z, round as f32 + fastrand::u32(0..100) as f32, false);
        }
        // Explosions gib by the nearest joint (see `gibs`).
        let dir = (feet + Vec3::Y * 0.9 - at).normalize_or(Vec3::Y);
        b.hurt_ev.write(crate::gibs::ZombieHurt { zombie: ze, amount: before - z.hp.max(-1e6), hp_after: z.hp, by: crate::gibs::HurtBy::Explosive, loc: None, from: at, point: at, dir });
        if dead {
            kills += 1;
            earn(&mut b.score, &mut b.points, &b.pu, rules::kill_points(KillKind::Explosive));
            b.killed.write(ZombieKilled { pos: feet, drop_allowed: true });
            spawn_burst(commands, mats, feet + Vec3::Y * 1.0, &mats.blood, 12, 3.5);
        } else {
            earn(&mut b.score, &mut b.points, &b.pu, rules::POINTS_HIT);
            spawn_burst(commands, mats, feet + Vec3::Y * 1.0, &mats.blood, 5, 2.0);
        }
    }
    // The player is hurt by it too.
    let player_dist = closest_on_segment(at, Vec3::new(eye.x, feet_y, eye.z), eye).distance(at);
    let mut self_dmg = 0.0;
    if let Some(dmg) = def.damage_at(player_dist / U) {
        if line_clear(&b.world, at, eye) || line_clear(&b.world, at, eye - Vec3::Y * 0.8) {
            self_dmg = dmg;
            if player::damage_player(&mut b.health, dmg, &mut b.alias) {
                b.next.set(GameState::GameOver);
            }
        }
    }
    info!("grenade: explosion at {at:?}: {hurt} zombies hit, {kills} killed, player {:.1} m away took {self_dmg:.0}", player_dist);

    // Sound: the surface layer (unknown surface) carries the blast.
    let sound = if b.zs.has("grenade_explode_default") { "grenade_explode_default" } else { "grenade_explode" };
    b.alias.write(PlayAlias::at(sound, at).or(Sfx::DoorOpen));

    // Shake and flash by distance.
    let k = (1.0 - player_dist / (radius * 2.5)).clamp(0.0, 1.0);
    if k > 0.0 {
        g.shake = g.shake.max(0.25 + 0.75 * k).min(1.0);
    }
    g.flash = g.flash.max(0.5 * k * k);

    // The game's explosion effect (by the surface underneath); the
    // stand-in fireball, light, smoke and debris otherwise.
    b.fx.write(crate::fx::FxEvent::Explosion { weapon: GRENADE_ID, pos: at });
    if crate::fx::live() {
        return;
    }
    commands.spawn((
        Mesh3d(mats.sphere.clone()),
        MeshMaterial3d(mats.glow_gold.clone()),
        Transform::from_translation(at).with_scale(Vec3::splat(0.2)),
        Blast { t: 0.0, ttl: 0.18, size: 0.2, grow: 6.0, light: 0.0 },
        NotShadowCaster,
        Dynamic,
    ));
    commands.spawn((
        PointLight { color: Color::srgb(1.0, 0.65, 0.3), intensity: 2_000_000.0, range: radius * 1.5, shadows_enabled: false, ..default() },
        Transform::from_translation(at + Vec3::Y * 0.3),
        Blast { t: 0.0, ttl: 0.3, size: 0.0, grow: 0.0, light: 2_000_000.0 },
        Dynamic,
    ));
    for _ in 0..6 {
        let off = Vec3::new(fastrand::f32() - 0.5, fastrand::f32() * 0.6, fastrand::f32() - 0.5) * 0.8;
        commands.spawn((
            Mesh3d(mats.sphere.clone()),
            MeshMaterial3d(mats.rubble.clone()),
            Transform::from_translation(at + off).with_scale(Vec3::splat(0.25)),
            Blast { t: 0.0, ttl: 1.2 + fastrand::f32() * 0.6, size: 0.25, grow: 0.6 + fastrand::f32() * 0.5, light: 0.0 },
            NotShadowCaster,
            Dynamic,
        ));
    }
    spawn_burst(commands, mats, at, &mats.spark, 18, 7.0);
    spawn_burst(commands, mats, at, &mats.rubble, 10, 4.0);
}

fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let k = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    a + ab * k
}

/// Explosion visuals: grow and shrink (fireball, smoke) or fade (light).
fn fade_fx(time: Res<Time>, mut commands: Commands, mut q: Query<(Entity, &mut Transform, &mut Blast, Option<&mut PointLight>)>, mut g: ResMut<Grenades>) {
    let dt = time.delta_secs();
    g.flash = (g.flash - dt * 2.5).max(0.0);
    g.shake = (g.shake - dt * 1.6).max(0.0);
    for (e, mut t, mut b, light) in &mut q {
        b.t += dt;
        if b.t >= b.ttl {
            commands.entity(e).try_despawn();
            continue;
        }
        let k = b.t / b.ttl;
        if let Some(mut l) = light {
            l.intensity = b.light * (1.0 - k) * (1.0 - k);
        } else {
            // Grows, then shrinks away over the last 30 %.
            let fade = if k > 0.7 { (1.0 - (k - 0.7) / 0.3).max(0.05) } else { 1.0 };
            t.scale = Vec3::splat((b.size + b.grow * b.t) * fade);
            t.translation.y += dt * 0.4;
        }
    }
}

/// A shaky camera after a nearby blast.
fn shake(g: Res<Grenades>, time: Res<Time>, mut cam: Query<&mut Transform, With<Player>>) {
    if g.shake <= 0.0 {
        return;
    }
    let Ok(mut t) = cam.single_mut() else { return };
    let s = g.shake * g.shake * 0.035;
    let tt = time.elapsed_secs() * 40.0;
    t.rotation *= Quat::from_euler(EulerRot::YXZ, (tt * 1.3).sin() * s, (tt * 1.7).cos() * s, (tt * 0.9).sin() * s * 0.5);
}

/// Shows the arms with the grenade during a throw: the pin pull (played over
/// `holdFireTime`, then held while cooking) and the throw (over `fireTime`),
/// with the weapon in hand hidden meanwhile.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn view_rig(
    mut commands: Commands,
    g: Res<Grenades>,
    stats: Option<Res<GrenadeStats>>,
    view_rig: Res<crate::nacht::ViewRig>,
    vm: Query<Entity, With<ViewModel>>,
    mut rigs: Query<(Entity, &mut GrenadeRig)>,
    hands: Query<Entity, With<HandGrenade>>,
    mut guns: Query<&mut Visibility, (With<GunModel>, Without<GrenadeRig>, Without<HandGrenade>, Without<HeldGrenade>)>,
    mut vis: Query<&mut Visibility, (Or<(With<GrenadeRig>, With<HandGrenade>, With<HeldGrenade>)>, Without<GunModel>)>,
    mut tq: Query<&mut Transform>,
    mats: Res<Mats>,
    (zs, mut alias, mut last): (Res<ZoneSounds>, EventWriter<PlayAlias>, Local<Option<(String, f32)>>),
) {
    let Some(stats) = stats else { return };
    let def = &stats.0;
    let active = g.throw != Throw::Idle;
    for mut v in &mut guns {
        v.set_if_neq(if active { Visibility::Hidden } else { Visibility::Inherited });
    }
    if !active {
        for (e, _) in &rigs {
            commands.entity(e).try_despawn();
        }
        for e in &hands {
            commands.entity(e).try_despawn();
        }
        *last = None;
        return;
    }
    let Ok(root) = vm.single() else { return };
    let anims = view_rig.anims.get(GRENADE_ID);
    let real = view_rig.arms.is_some() && view_rig.guns.contains_key(GRENADE_ID) && anims.is_some();
    if real {
        let Ok((rig_e, mut rig)) = rigs.single_mut() else {
            if rigs.is_empty() {
                let (arms, part) = (view_rig.arms.as_ref().unwrap(), &view_rig.guns[GRENADE_ID]);
                let rig_root = commands
                    .spawn((
                        Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                        Visibility::Hidden,
                        ChildOf(root),
                    ))
                    .id();
                let mut joints = Vec::new();
                crate::nacht::spawn_part(&mut commands, arms, &mut joints, rig_root, rig_root);
                let tag_weapon = joints.iter().find(|j| j.0 == "tag_weapon").map(|j| j.1).unwrap_or(rig_root);
                // The grenade's own meshes under their own parent, hidden
                // once it has left the hand.
                let held = commands.spawn((Transform::default(), Visibility::Inherited, HeldGrenade, ChildOf(rig_root))).id();
                crate::nacht::spawn_part(&mut commands, part, &mut joints, tag_weapon, held);
                commands.entity(rig_root).insert(GrenadeRig { joints, posed: false, held });
            }
            return;
        };
        let anims = anims.unwrap();
        let (clip, p) = match g.throw {
            Throw::Hold { t, .. } => (anims.get("hold_fire"), t / def.hold_time.max(0.01)),
            Throw::Throwing { t } => (anims.get("fire"), t / def.fire_time.max(0.01)),
            Throw::Idle => (None, 0.0),
        };
        let Some(clip) = clip.or_else(|| anims.get("idle")) else { return };
        let p = p.clamp(0.0, 1.0);
        // Notetrack sounds crossed since last frame.
        let from = last.as_ref().filter(|(n, _)| *n == clip.name).map(|l| l.1);
        for (note, nt) in &clip.notify {
            let crossed = match from {
                Some(q) => *nt > q && *nt <= p,
                None => *nt <= p,
            };
            if crossed {
                if let Some(a) = zs.notetrack(GRENADE_ID, note) {
                    alias.write(PlayAlias::local(a));
                }
            }
        }
        *last = Some((clip.name.clone(), p));
        let map = crate::nacht::track_map(clip, &rig.joints);
        crate::nacht::pose_mapped(clip, p * clip.numframes, &rig.joints, &map, &mut tq, None);
        if !rig.posed {
            rig.posed = true;
            if let Ok(mut v) = vis.get_mut(rig_e) {
                *v = Visibility::Inherited;
            }
        }
        if let Ok(mut v) = vis.get_mut(rig.held) {
            v.set_if_neq(if matches!(g.throw, Throw::Throwing { .. }) { Visibility::Hidden } else { Visibility::Inherited });
        }
    } else {
        // Stand-in: a stick grenade raised into view, gone once thrown.
        let e = match hands.single() {
            Ok(e) => e,
            Err(_) => {
                commands
                    .spawn((Transform::from_xyz(0.1, -0.3, -0.4).with_scale(Vec3::splat(0.6)), Visibility::Inherited, HandGrenade, ChildOf(root)))
                    .with_children(|p| stand_in_grenade(p, &mats));
                return;
            }
        };
        if let Ok(mut v) = vis.get_mut(e) {
            v.set_if_neq(if matches!(g.throw, Throw::Throwing { .. }) { Visibility::Hidden } else { Visibility::Inherited });
        }
        if let (Throw::Hold { t, .. }, Ok(mut tr)) = (g.throw, tq.get_mut(e)) {
            let k = (t / def.hold_time.max(0.01)).clamp(0.0, 1.0);
            tr.translation = Vec3::new(0.12, -0.32 + 0.2 * k, -0.4 + 0.15 * k);
            tr.rotation = Quat::from_rotation_z(0.6 + 0.6 * k) * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        }
    }
}
