//! First-person controller: mouse look, walking/sprinting/jumping, collision
//! against the bunker, health and regeneration.

use crate::audio::{PlayAlias, Sfx};
use crate::settings::UserSettings;
use crate::{cursor_locked, GameState, LevelRes, World};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::camera::Exposure;
use bevy::window::PrimaryWindow;
use zm_core::geom::V3;
use zm_core::rules;
use zm_core::trimesh::blocks;

/// Game units (inches) to metres.
const U: f32 = 0.0254;
/// Standing eye height (60 units).
pub const EYE: f32 = 60.0 * U;
pub const RADIUS: f32 = 15.0 * U;

// Movement constants of the original game (g_speed, g_gravity, jump_height,
// player_*SpeedScale), in metres.
const RUN_SPEED: f32 = 190.0 * U;
const SPRINT_SCALE: f32 = 1.5;
const BACK_SCALE: f32 = 0.7;
const STRAFE_SCALE: f32 = 0.8;
const ADS_SCALE: f32 = 0.6;
const GRAVITY: f32 = 800.0 * U;
const JUMP_HEIGHT: f32 = 39.0 * U;
const ACCELERATE: f32 = 10.0;
const AIR_ACCELERATE: f32 = 1.0;
const FRICTION: f32 = 6.0;
const STOP_SPEED: f32 = 100.0 * U;
/// Seconds of sprint before the player is winded.
const SPRINT_TIME: f32 = 4.0;
/// Highest step the player walks up without jumping (`jump_stepSize`).
const STEP: f32 = 18.0 * U;
/// Walkable surfaces: normal.y of at least this (as the game's 0.7).
const MIN_WALK_NORMAL: f32 = 0.7;
/// Time constant of the view easing over steps (seconds).
const STEP_SMOOTH: f32 = 0.1;
/// How far ahead and behind the ground slope is probed for the view.
const SLOPE_PROBE: f32 = 12.0 * U;

#[derive(Component)]
pub struct Player;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Stance {
    #[default]
    Stand,
    Crouch,
    Prone,
}

impl Stance {
    pub fn eye(self) -> f32 {
        match self {
            Stance::Stand => EYE,
            Stance::Crouch => 40.0 * U,
            Stance::Prone => 11.0 * U,
        }
    }
    /// Collision height.
    pub fn height(self) -> f32 {
        match self {
            Stance::Stand => 70.0 * U,
            Stance::Crouch => 50.0 * U,
            Stance::Prone => 30.0 * U,
        }
    }
    fn speed_scale(self) -> f32 {
        match self {
            Stance::Stand => 1.0,
            Stance::Crouch => 0.65,
            Stance::Prone => 0.15,
        }
    }
}

#[derive(Component, Default)]
pub struct PlayerCtl {
    pub yaw: f32,
    pub pitch: f32,
    pub feet_y: f32,
    pub vel_y: f32,
    /// Horizontal velocity (x, z).
    pub vel: Vec2,
    pub on_ground: bool,
    pub bob: f32,
    pub moving: bool,
    pub sprinting: bool,
    pub stance: Stance,
    /// Current eye height above the feet (eases between stances).
    pub eye: f32,
    /// Sprint stamina left, in seconds.
    pub sprint_left: f32,
    /// 0 = hip, 1 = fully aimed down sights.
    pub ads: f32,
    /// Extra pitch from recoil, recovers over time.
    pub recoil: f32,
    /// View offset left over from stepping up or down; eases back to 0 so
    /// steps and stairs never jerk the camera.
    pub step_offset: f32,
}

impl PlayerCtl {
    fn new(feet_y: f32, yaw: f32) -> Self {
        PlayerCtl { on_ground: true, feet_y, yaw, eye: EYE, sprint_left: SPRINT_TIME, ..default() }
    }

    /// Position of the feet given the camera transform.
    pub fn feet(&self, t: &Transform) -> Vec3 {
        Vec3::new(t.translation.x, self.feet_y, t.translation.z)
    }
}

#[derive(Resource)]
pub struct Health {
    pub hp: f32,
    pub since_hit: f32,
    /// Flash intensity for the damage overlay.
    pub flash: f32,
    /// The game's health/regen state (`_gameskill`).
    pub state: rules::PlayerHealth,
}

impl Default for Health {
    fn default() -> Self {
        Health { hp: rules::PLAYER_MAX_HEALTH, since_hit: 99.0, flash: 0.0, state: rules::PlayerHealth::new(rules::PLAYER_MAX_HEALTH) }
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Health>()
            .add_systems(Startup, spawn_player)
            .add_systems(
                Update,
                (look, test_walk_setup, movement, regen, breathing, tweak_settings).chain().run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, apply_exposure);
        if let Some(walk) = TestWalk::from_env() {
            app.insert_resource(walk);
        }
        // Developer aid: `UNDEAD_TEST_DT=<seconds>` advances the game clock by
        // a fixed step every frame, so test runs don't depend on machine load.
        if let Some(dt) = std::env::var("UNDEAD_TEST_DT").ok().and_then(|s| s.parse::<f32>().ok()).filter(|d| *d > 0.0) {
            app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(dt)));
        }
    }
}

pub fn spawn_player(mut commands: Commands, level: Res<LevelRes>, settings: Res<UserSettings>) {
    let (x, z) = level.0.player_start;
    commands.spawn((
        Camera3d::default(),
        // LDR like the original (no tonemapping or bloom: the zombie vision
        // turns glow off); fog and the film grade come from `postfx`.
        Camera { hdr: false, ..default() },
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        Msaa::Off,
        bevy::core_pipeline::prepass::DepthPrepass,
        crate::postfx::WawPost::off(1.0),
        Projection::Perspective(PerspectiveProjection { fov: settings.fov.to_radians(), near: 0.03, ..default() }),
        Exposure { ev100: settings.exposure_ev },
        DistanceFog {
            color: Color::srgb(0.02, 0.025, 0.04),
            falloff: FogFalloff::Linear { start: 12.0, end: 48.0 },
            ..default()
        },
        Transform::from_xyz(x, EYE, z),
        SpatialListener::new(0.3),
        Player,
        PlayerCtl::new(0.0, 0.0),
    ));
}

/// Reset the player for a new game.
pub fn reset_player(q: &mut Query<(&mut Transform, &mut PlayerCtl), With<Player>>, level: &LevelRes) {
    if let Ok((mut t, mut c)) = q.single_mut() {
        let (x, z) = level.0.player_start;
        let y = level.0.player_start_y;
        *c = PlayerCtl::new(y, level.0.player_yaw);
        *t = Transform::from_xyz(x, y + EYE, z);
    }
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    windows: Query<&Window, With<PrimaryWindow>>,
    settings: Res<UserSettings>,
    time: Res<Time>,
    mut q: Query<(&mut Transform, &mut PlayerCtl, &mut Projection), With<Player>>,
) {
    let Ok((mut t, mut c, mut proj)) = q.single_mut() else { return };
    if cursor_locked(&windows) {
        let sens = settings.sensitivity * (1.0 - 0.45 * c.ads);
        c.yaw -= motion.delta.x * sens;
        c.pitch = (c.pitch - motion.delta.y * sens).clamp(-1.5, 1.5);
    }
    c.recoil *= (1.0 - 10.0 * time.delta_secs()).max(0.0);
    t.rotation = Quat::from_euler(EulerRot::YXZ, c.yaw, (c.pitch + c.recoil).clamp(-1.55, 1.55), 0.0);
    if let Projection::Perspective(p) = proj.as_mut() {
        let sprint_fov = if c.sprinting { 6.0 } else { 0.0 };
        let target = (settings.fov - 22.0 * c.ads + sprint_fov).to_radians();
        p.fov += (target - p.fov) * (12.0 * time.delta_secs()).min(1.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn movement(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    world: Res<World>,
    mut walk: Option<ResMut<TestWalk>>,
    mut q: Query<(&mut Transform, &mut PlayerCtl), With<Player>>,
) {
    let Ok((mut t, mut c)) = q.single_mut() else { return };
    let cpu = std::time::Instant::now();
    let dt = time.delta_secs().min(0.05);
    let driven = walk.as_ref().is_some_and(|w| w.active());

    let mut wish = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        wish.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        wish.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        wish.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        wish.x += 1.0;
    }
    let wish = wish.normalize_or_zero();
    let aiming = mouse.pressed(MouseButton::Right);
    c.ads = (c.ads + if aiming { 6.0 } else { -6.0 } * dt).clamp(0.0, 1.0);

    // Stance: C toggles crouch, Ctrl toggles prone, jump or sprint stand up
    // one step at a time. Getting up needs head room.
    let here = (t.translation.x, t.translation.z);
    let fits = |s: Stance, c: &PlayerCtl| -> bool {
        let (x, z) = here;
        let top = c.feet_y + s.height();
        let mesh_ok = world.mesh.as_ref().is_none_or(|m| {
            m.raycast_mask(V3::new(x, c.feet_y + 0.3, z), V3::new(0.0, 1.0, 0.0), top - c.feet_y - 0.3, blocks::PLAYER, |_| true).is_none()
        });
        let solids_ok = !world.player_solids.iter().any(|b| b.min.y < top && b.max.y > c.feet_y + 0.3 && b.push_circle(x, z, RADIUS * 0.5).is_some());
        mesh_ok && solids_ok
    };
    let mut want = c.stance;
    if keys.just_pressed(KeyCode::KeyC) {
        want = if c.stance == Stance::Crouch { Stance::Stand } else { Stance::Crouch };
    }
    if keys.just_pressed(KeyCode::ControlLeft) || keys.just_pressed(KeyCode::KeyZ) {
        want = if c.stance == Stance::Prone { Stance::Crouch } else { Stance::Prone };
    }
    let up_pressed = keys.just_pressed(KeyCode::Space) || (keys.just_pressed(KeyCode::ShiftLeft) && wish.y > 0.5);
    if up_pressed && c.stance != Stance::Stand {
        want = if c.stance == Stance::Prone { Stance::Crouch } else { Stance::Stand };
    }
    if driven {
        want = c.stance;
    }
    let standing_up = want != c.stance;
    if standing_up && (want.height() <= c.stance.height() || fits(want, &c)) && c.on_ground {
        c.stance = want;
    }

    // Sprint: standing, moving forward, not aiming, while stamina lasts.
    let wants_sprint = keys.pressed(KeyCode::ShiftLeft) && wish.y > 0.5 && !aiming && c.stance == Stance::Stand && c.on_ground;
    if wants_sprint && (c.sprinting || c.sprint_left > 1.0) && c.sprint_left > 0.0 {
        c.sprinting = true;
        c.sprint_left -= dt;
    } else {
        c.sprinting = false;
        c.sprint_left = (c.sprint_left + dt).min(SPRINT_TIME);
    }

    let (s, co) = c.yaw.sin_cos();
    let forward = Vec2::new(-s, -co);
    let right = Vec2::new(co, -s);
    let mut dir_scale = if wish.y < -0.1 { BACK_SCALE } else if wish.y.abs() < 0.1 && wish.x != 0.0 { STRAFE_SCALE } else { 1.0 };
    let mut wish_dir = (forward * wish.y + right * wish.x).normalize_or_zero();
    if let Some(w) = walk.as_mut().filter(|w| w.active()) {
        wish_dir = w.steer(Vec2::new(t.translation.x, t.translation.z));
        dir_scale = 1.0;
    }
    c.moving = wish_dir != Vec2::ZERO;
    let wish_speed = RUN_SPEED * c.stance.speed_scale() * dir_scale * if c.sprinting { SPRINT_SCALE } else { 1.0 } * (1.0 - (1.0 - ADS_SCALE) * c.ads);

    // Quake-style friction and acceleration (the game's movement code is
    // derived from it): quick but not instant starts and stops.
    if c.on_ground {
        let speed = c.vel.length();
        if speed > 1e-4 {
            let drop = speed.max(STOP_SPEED) * FRICTION * dt;
            c.vel *= ((speed - drop).max(0.0)) / speed;
        }
    }
    // Unlike Quake, the game scales acceleration by at least the stop
    // speed. Without that, a slow wish speed (prone: 28.5 u/s) adds less per
    // frame than friction takes away (stop speed 100 u/s) and the player
    // barely creeps.
    let accel = if c.on_ground { ACCELERATE } else { AIR_ACCELERATE };
    let add = wish_speed - c.vel.dot(wish_dir);
    if add > 0.0 {
        c.vel += wish_dir * (accel * wish_speed.max(STOP_SPEED) * dt).min(add);
    }

    let start = Vec2::new(t.translation.x, t.translation.z);
    let mut x = start.x + c.vel.x * dt;
    let mut z = start.y + c.vel.y * dt;
    let height = c.stance.height();
    if let Some(mesh) = &world.mesh {
        // Walls of the map's own player collision, against the body above
        // step height: anything lower is stepped onto (see the ground below).
        let (nx, nz, _) = mesh.push_cylinder_mask(x, z, RADIUS, c.feet_y + STEP, c.feet_y + height, 4, blocks::PLAYER, |t| t.n.y.abs() < MIN_WALK_NORMAL);
        x = nx;
        z = nz;
    }
    let (feet, head) = (c.feet_y + 0.3, c.feet_y + height);
    for _ in 0..3 {
        for solid in world.player_solids.iter().filter(|s| s.min.y < head && s.max.y > feet) {
            if let Some((nx, nz)) = solid.push_circle(x, z, RADIUS) {
                x = nx;
                z = nz;
            }
        }
    }
    // Lose the velocity that ran into walls.
    if dt > 0.0 {
        let actual = (Vec2::new(x, z) - start) / dt;
        if actual.length_squared() < c.vel.length_squared() {
            c.vel = actual;
        }
    }

    if c.on_ground && keys.just_pressed(KeyCode::Space) && c.stance == Stance::Stand && !standing_up && !driven {
        c.vel_y = (2.0 * GRAVITY * JUMP_HEIGHT).sqrt();
        c.on_ground = false;
    }
    let (was_on_ground, old_feet) = (c.on_ground, c.feet_y);
    c.vel_y -= GRAVITY * dt;
    c.feet_y += c.vel_y * dt;
    if let (Some(mesh), true) = (&world.mesh, c.vel_y > 0.0) {
        // Head against a ceiling.
        let from = old_feet + 0.1;
        if let Some(h) = mesh.raycast_mask(V3::new(x, from, z), V3::new(0.0, 1.0, 0.0), c.feet_y + height - from, blocks::PLAYER, |t| t.n.y.abs() >= MIN_WALK_NORMAL) {
            c.feet_y = (from + h.t - height).clamp(old_feet, c.feet_y);
            c.vel_y = 0.0;
        }
    }
    // The ground: where a ball as wide as the player comes to rest. It rides
    // smoothly over step edges, so stairs climb like a ramp.
    let ground = match &world.mesh {
        Some(mesh) => mesh.support_sphere_mask(x, z, RADIUS, c.feet_y + STEP - c.vel_y.min(0.0) * dt, c.feet_y - 6.0, MIN_WALK_NORMAL, blocks::PLAYER),
        None => Some(0.0),
    };
    match ground {
        Some(g) if c.feet_y <= g || (c.on_ground && c.vel_y <= 0.0 && c.feet_y - g < STEP) => {
            c.feet_y = g;
            c.vel_y = 0.0;
            c.on_ground = true;
        }
        _ => c.on_ground = false,
    }
    if c.feet_y < -50.0 {
        // Fell out of the world: back to the start.
        c.feet_y = 0.0;
        c.vel_y = 0.0;
    }
    // The view climbs at the slope of the ground around the player (probed
    // ahead and behind along the motion): exact on ramps, a straight ramp
    // over stairs. Whatever the feet do beyond that (a step's edge) goes
    // into an offset that eases out, as the game smooths steps.
    if was_on_ground && c.on_ground {
        let dy = c.feet_y - old_feet;
        let mv = Vec2::new(x, z) - start;
        let moved = mv.length();
        let mut expected = 0.0;
        if let (Some(mesh), true) = (&world.mesh, moved > 1e-5) {
            let d = mv / moved;
            let probe = |s: f32| mesh.support_sphere_mask(x + d.x * s, z + d.y * s, RADIUS, c.feet_y + STEP, c.feet_y - STEP, MIN_WALK_NORMAL, blocks::PLAYER);
            if let (Some(ahead), Some(behind)) = (probe(SLOPE_PROBE), probe(-SLOPE_PROBE)) {
                expected = ((ahead - behind) / (2.0 * SLOPE_PROBE)).clamp(-1.2, 1.2) * moved;
            }
        }
        c.step_offset -= dy - expected;
    }
    c.step_offset = (c.step_offset * (-dt / STEP_SMOOTH).exp()).clamp(-STEP, STEP);

    // Eye height eases to the stance's.
    let target_eye = c.stance.eye();
    let step = 3.0 * dt;
    c.eye += (target_eye - c.eye).clamp(-step, step);

    let speed_frac = (c.vel.length() / RUN_SPEED).min(1.6);
    if c.on_ground && speed_frac > 0.05 {
        c.bob += dt * (6.0 + 5.0 * speed_frac);
    }
    let bob = (c.bob).sin() * 0.022 * speed_frac.min(1.0) * if c.on_ground { 1.0 } else { 0.0 };
    t.translation = Vec3::new(x, c.feet_y + c.eye + bob + c.step_offset, z);
    if let Some(w) = walk.as_mut().filter(|w| w.started && !w.logged_done) {
        w.log(dt, &c, t.translation, cpu.elapsed());
    }
}

/// Developer aid: `UNDEAD_TEST_WALK="<stand|crouch|prone> x y z  x y  x y ..."`
/// (game units: a start point with its floor height, then waypoints) puts
/// the player at the start in that stance a second into the game, opens
/// every door and walks the path, logging feet and eye heights and the
/// speed every frame (`[walk]` lines in the log).
#[derive(Resource)]
pub struct TestWalk {
    stance: Stance,
    start: Vec3,
    path: Vec<Vec2>,
    next: usize,
    wait: f32,
    started: bool,
    done: bool,
    logged_done: bool,
    t: f32,
    dist: f32,
    last: Option<(f32, f32, Vec2)>,
    max_eye_step: f32,
    max_feet_step: f32,
}

impl TestWalk {
    fn from_env() -> Option<TestWalk> {
        let spec = std::env::var("UNDEAD_TEST_WALK").ok()?;
        let mut it = spec.split_whitespace();
        let stance = match it.next()? {
            "crouch" => Stance::Crouch,
            "prone" => Stance::Prone,
            _ => Stance::Stand,
        };
        let v: Vec<f32> = it.filter_map(|s| s.parse().ok()).collect();
        if v.len() < 5 {
            return None;
        }
        let game = |x: f32, y: f32| Vec2::new(x * U, -y * U);
        let start = Vec3::new(v[0] * U, v[2] * U, -v[1] * U);
        let path = v[3..].as_chunks::<2>().0.iter().map(|p| game(p[0], p[1])).collect();
        Some(TestWalk {
            stance,
            start,
            path,
            next: 0,
            wait: 0.0,
            started: false,
            done: false,
            logged_done: false,
            t: 0.0,
            dist: 0.0,
            last: None,
            max_eye_step: 0.0,
            max_feet_step: 0.0,
        })
    }

    fn active(&self) -> bool {
        self.started && !self.done
    }

    /// Direction towards the next waypoint (zero when the path is done).
    fn steer(&mut self, pos: Vec2) -> Vec2 {
        while let Some(&p) = self.path.get(self.next) {
            if p.distance(pos) > 0.05 {
                return (p - pos).normalize_or_zero();
            }
            self.next += 1;
        }
        self.done = true;
        Vec2::ZERO
    }

    fn log(&mut self, dt: f32, c: &PlayerCtl, eye: Vec3, cpu: std::time::Duration) {
        let pos = Vec2::new(eye.x, eye.z);
        if let Some((feet, eye_y, last)) = self.last {
            self.max_eye_step = self.max_eye_step.max((eye.y - eye_y).abs());
            self.max_feet_step = self.max_feet_step.max((c.feet_y - feet).abs());
            self.dist += pos.distance(last);
        }
        self.last = Some((c.feet_y, eye.y, pos));
        self.t += dt;
        info!(
            "[walk] t={:.3} stance={:?} x={:.1} y={:.1} feet={:.2} eye={:.2} speed={:.3} m/s cpu={}us",
            self.t,
            c.stance,
            eye.x / U,
            -eye.z / U,
            c.feet_y / U,
            eye.y / U,
            c.vel.length(),
            cpu.as_micros()
        );
        if self.done {
            self.logged_done = true;
            info!(
                "[walk] done: {:.2} s, {:.2} m, average {:.3} m/s; largest change in one frame: feet {:.2} in, eye {:.2} in",
                self.t,
                self.dist,
                self.dist / self.t.max(1e-3),
                self.max_feet_step / U,
                self.max_eye_step / U
            );
        }
    }
}

/// Starts a [`TestWalk`]: opens the doors and places the player.
fn test_walk_setup(
    time: Res<Time>,
    walk: Option<ResMut<TestWalk>>,
    level: Res<LevelRes>,
    mut world: ResMut<World>,
    mut q: Query<(&mut Transform, &mut PlayerCtl), With<Player>>,
) {
    let Some(mut w) = walk else { return };
    if w.started {
        return;
    }
    w.wait += time.delta_secs();
    if w.wait < 1.0 {
        return;
    }
    let Ok((mut t, mut c)) = q.single_mut() else { return };
    world.door_open.iter_mut().for_each(|d| *d = true);
    world.rebuild(&level.0);
    c.stance = w.stance;
    c.eye = w.stance.eye();
    c.feet_y = w.start.y;
    c.vel = Vec2::ZERO;
    c.vel_y = 0.0;
    c.on_ground = true;
    c.step_offset = 0.0;
    t.translation = w.start + Vec3::Y * c.eye;
    w.started = true;
    info!("[walk] start {:?} at {:?}, {} waypoints", w.stance, w.start / U, w.path.len());
}

/// Heavy breathing while badly hurt, a relieved breath on recovery.
fn breathing(time: Res<Time>, hp: Res<Health>, mut alias: EventWriter<PlayAlias>, mut state: Local<(bool, f32)>) {
    let hurt = hp.hp <= rules::PLAYER_MAX_HEALTH * 0.35;
    let (breathing, wait) = &mut *state;
    if hurt {
        *wait -= time.delta_secs();
        if *wait <= 0.0 {
            alias.write(PlayAlias::local("breathing_hurt"));
            *wait = 1.2 + fastrand::f32() * 0.4;
        }
        *breathing = true;
    } else if *breathing {
        *breathing = false;
        *wait = 0.0;
        alias.write(PlayAlias::local("breathing_better"));
    }
}

/// World at War's single-player regeneration (Regular): nothing for 2.4 s
/// after a hit, then straight back to full; when badly hurt (20% or less)
/// it waits 5 s and then fills in half a second.
fn regen(time: Res<Time>, mut hp: ResMut<Health>, round: Res<crate::Round>) {
    let dt = time.delta_secs();
    hp.since_hit += dt;
    hp.flash = (hp.flash - dt * 1.5).max(0.0);
    let h = &mut *hp;
    h.state.hp = h.hp;
    h.state.tick(&round.0.rules, dt);
    h.hp = h.state.hp;
}

/// Damage the player. Returns true if this hit downed them: alone in
/// zombie mode any hit at least as big as the remaining health ends the
/// game. A big hit makes the player briefly invulnerable.
pub fn damage_player(hp: &mut Health, amount: f32, alias: &mut EventWriter<PlayAlias>) -> bool {
    let rules = rules::ZombieRules::nacht();
    hp.state.hp = hp.hp;
    if hp.state.invulnerable > 0.0 {
        return false;
    }
    let downed = hp.state.hit(&rules, amount);
    hp.hp = hp.state.hp;
    hp.since_hit = 0.0;
    hp.flash = 1.0;
    alias.write(PlayAlias::local("player_pain_small").or(Sfx::PlayerHurt));
    downed
}

/// `-`/`=` adjust mouse sensitivity, `[`/`]` brightness (also in Options).
fn tweak_settings(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<UserSettings>) {
    let mut s = settings.as_ref().clone();
    if keys.just_pressed(KeyCode::Minus) {
        s.sensitivity = (s.sensitivity / 1.15).max(0.0003);
    }
    if keys.just_pressed(KeyCode::Equal) {
        s.sensitivity = (s.sensitivity * 1.15).min(0.02);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        s.exposure_ev = (s.exposure_ev + 0.5).min(14.0);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        s.exposure_ev = (s.exposure_ev - 0.5).max(3.0);
    }
    if s != *settings {
        *settings = s;
        settings.save();
    }
}

/// The brightness setting. On the real maps the exposure is fixed (the
/// game's lighting is scaled to it) and brightness is a display gamma in
/// the post pass; the prototype map uses camera exposure.
fn apply_exposure(
    settings: Res<UserSettings>,
    real_map: Option<Res<crate::nacht::NachtActive>>,
    mut cams: Query<(&mut Exposure, &mut crate::postfx::WawPost), With<Player>>,
) {
    let Ok((mut e, mut post)) = cams.single_mut() else { return };
    let (ev, gamma) = if real_map.is_some() {
        // Exposure factor 1 / (1.2 * 2^ev) = 1.
        let brightness = 15.0 - settings.exposure_ev;
        (crate::nacht::REAL_MAP_EV, (1.0 + (brightness - 7.5) * 0.08).clamp(0.4, 2.0))
    } else {
        (settings.exposure_ev, 1.0)
    };
    if e.ev100 != ev {
        e.ev100 = ev;
    }
    if post.dark_tint.w != gamma {
        post.set_gamma(gamma);
    }
}
