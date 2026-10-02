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
                (look, movement, regen, breathing, tweak_settings).chain().run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, apply_exposure);
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

fn movement(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    world: Res<World>,
    mut q: Query<(&mut Transform, &mut PlayerCtl), With<Player>>,
) {
    let Ok((mut t, mut c)) = q.single_mut() else { return };
    let dt = time.delta_secs().min(0.05);

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
        let mesh_ok = world.mesh.as_ref().is_none_or(|m| m.raycast(V3::new(x, c.feet_y + 0.3, z), V3::new(0.0, 1.0, 0.0), top - c.feet_y - 0.3).is_none());
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
    c.moving = wish != Vec2::ZERO;
    let dir_scale = if wish.y < -0.1 { BACK_SCALE } else if wish.y.abs() < 0.1 && wish.x != 0.0 { STRAFE_SCALE } else { 1.0 };
    let wish_speed = RUN_SPEED * c.stance.speed_scale() * dir_scale * if c.sprinting { SPRINT_SCALE } else { 1.0 } * (1.0 - (1.0 - ADS_SCALE) * c.ads);
    let wish_dir = (forward * wish.y + right * wish.x).normalize_or_zero();

    // Quake-style friction and acceleration (the game's movement code is
    // derived from it): quick but not instant starts and stops.
    if c.on_ground {
        let speed = c.vel.length();
        if speed > 1e-4 {
            let drop = speed.max(STOP_SPEED) * FRICTION * dt;
            c.vel *= ((speed - drop).max(0.0)) / speed;
        }
    }
    let accel = if c.on_ground { ACCELERATE } else { AIR_ACCELERATE };
    let add = wish_speed - c.vel.dot(wish_dir);
    if add > 0.0 {
        c.vel += wish_dir * (accel * wish_speed * dt).min(add);
    }

    let start = Vec2::new(t.translation.x, t.translation.z);
    let mut x = start.x + c.vel.x * dt;
    let mut z = start.y + c.vel.y * dt;
    let height = c.stance.height();
    if let Some(mesh) = &world.mesh {
        // Walls of the real map: spheres above step height, ignoring floors.
        let heights: &[f32] = match c.stance {
            Stance::Stand => &[0.6, 1.1, 1.5],
            Stance::Crouch => &[0.6, 1.0],
            Stance::Prone => &[0.45],
        };
        for &h in heights {
            let (p, _) = mesh.push_sphere(V3::new(x, c.feet_y + h, z), RADIUS, 3, |t| t.n.y.abs() < 0.7);
            x = p.x;
            z = p.z;
        }
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

    if c.on_ground && keys.just_pressed(KeyCode::Space) && c.stance == Stance::Stand && !standing_up {
        c.vel_y = (2.0 * GRAVITY * JUMP_HEIGHT).sqrt();
        c.on_ground = false;
    }
    c.vel_y -= GRAVITY * dt;
    c.feet_y += c.vel_y * dt;
    let ground = match &world.mesh {
        Some(mesh) => mesh.ground(x, z, c.feet_y + 0.55 - c.vel_y.min(0.0) * dt, c.feet_y - 6.0, 0.6),
        None => Some(0.0),
    };
    match ground {
        Some(g) if c.feet_y <= g || (c.on_ground && c.vel_y <= 0.0 && c.feet_y - g < 0.45) => {
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

    // Eye height eases to the stance's.
    let target_eye = c.stance.eye();
    let step = 3.0 * dt;
    c.eye += (target_eye - c.eye).clamp(-step, step);

    let speed_frac = (c.vel.length() / RUN_SPEED).min(1.6);
    if c.on_ground && speed_frac > 0.05 {
        c.bob += dt * (6.0 + 5.0 * speed_frac);
    }
    let bob = (c.bob).sin() * 0.022 * speed_frac.min(1.0) * if c.on_ground { 1.0 } else { 0.0 };
    t.translation = Vec3::new(x, c.feet_y + c.eye + bob, z);
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

/// The brightness setting. On the real maps everything is lit in the
/// game's own units (exposure fixed at 1) and brightness is a display gamma
/// in the post pass; the prototype map uses camera exposure.
fn apply_exposure(
    settings: Res<UserSettings>,
    real_map: Option<Res<crate::nacht::NachtActive>>,
    mut cams: Query<(&mut Exposure, &mut crate::postfx::WawPost), With<Player>>,
) {
    let Ok((mut e, mut post)) = cams.single_mut() else { return };
    let (ev, gamma) = if real_map.is_some() {
        // Exposure factor 1 / (1.2 * 2^ev) = 1.
        let brightness = 15.0 - settings.exposure_ev;
        ((1.0f32 / 1.2).log2(), (1.0 + (brightness - 7.5) * 0.08).clamp(0.4, 2.0))
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
