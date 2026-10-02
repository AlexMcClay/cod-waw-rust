//! First-person controller: mouse look, walking/sprinting/jumping, collision
//! against the bunker, health and regeneration.

use crate::audio::{PlaySfx, Sfx};
use crate::settings::UserSettings;
use crate::{cursor_locked, GameState, LevelRes, World};
use bevy::core_pipeline::bloom::Bloom;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::camera::Exposure;
use bevy::window::PrimaryWindow;
use zm_core::geom::V3;
use zm_core::rules;

pub const EYE: f32 = 1.65;
pub const RADIUS: f32 = 0.35;

#[derive(Component)]
pub struct Player;

#[derive(Component, Default)]
pub struct PlayerCtl {
    pub yaw: f32,
    pub pitch: f32,
    pub feet_y: f32,
    pub vel_y: f32,
    pub on_ground: bool,
    pub bob: f32,
    pub moving: bool,
    pub sprinting: bool,
    /// 0 = hip, 1 = fully aimed down sights.
    pub ads: f32,
    /// Extra pitch from recoil, recovers over time.
    pub recoil: f32,
}

#[derive(Resource)]
pub struct Health {
    pub hp: f32,
    pub since_hit: f32,
    /// Flash intensity for the damage overlay.
    pub flash: f32,
}

impl Default for Health {
    fn default() -> Self {
        Health { hp: rules::PLAYER_MAX_HEALTH, since_hit: 99.0, flash: 0.0 }
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Health>()
            .add_systems(Startup, spawn_player)
            .add_systems(
                Update,
                (look, movement, regen, tweak_settings).chain().run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, apply_exposure);
    }
}

pub fn spawn_player(mut commands: Commands, level: Res<LevelRes>, settings: Res<UserSettings>) {
    let (x, z) = level.0.player_start;
    commands.spawn((
        Camera3d::default(),
        Camera { hdr: true, ..default() },
        Projection::Perspective(PerspectiveProjection { fov: settings.fov.to_radians(), near: 0.03, ..default() }),
        Exposure { ev100: settings.exposure_ev },
        Bloom::NATURAL,
        DistanceFog {
            color: Color::srgb(0.02, 0.025, 0.04),
            falloff: FogFalloff::Linear { start: 12.0, end: 48.0 },
            ..default()
        },
        Transform::from_xyz(x, EYE, z),
        Player,
        PlayerCtl { on_ground: true, ..default() },
    ));
}

/// Reset the player for a new game.
pub fn reset_player(q: &mut Query<(&mut Transform, &mut PlayerCtl), With<Player>>, level: &LevelRes) {
    if let Ok((mut t, mut c)) = q.single_mut() {
        let (x, z) = level.0.player_start;
        let y = level.0.player_start_y;
        *c = PlayerCtl { on_ground: true, feet_y: y, yaw: level.0.player_yaw, ..default() };
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
    c.sprinting = keys.pressed(KeyCode::ShiftLeft) && wish.y > 0.5 && !aiming;
    let speed = if c.sprinting { 6.6 } else { 4.3 } * (1.0 - 0.4 * c.ads);

    let (s, co) = c.yaw.sin_cos();
    let forward = Vec2::new(-s, -co);
    let right = Vec2::new(co, -s);
    let delta = (forward * wish.y + right * wish.x) * speed * dt;
    c.moving = wish != Vec2::ZERO;

    let mut x = t.translation.x + delta.x;
    let mut z = t.translation.z + delta.y;
    if let Some(mesh) = &world.mesh {
        // Walls of the real map: spheres above step height, ignoring floors.
        for h in [0.6f32, 1.1, 1.55] {
            let (p, _) = mesh.push_sphere(V3::new(x, c.feet_y + h, z), RADIUS, 3, |t| t.n.y.abs() < 0.7);
            x = p.x;
            z = p.z;
        }
    }
    let (feet, head) = (c.feet_y + 0.3, c.feet_y + 1.7);
    for _ in 0..3 {
        for solid in world.player_solids.iter().filter(|s| s.min.y < head && s.max.y > feet) {
            if let Some((nx, nz)) = solid.push_circle(x, z, RADIUS) {
                x = nx;
                z = nz;
            }
        }
    }

    if c.on_ground && keys.just_pressed(KeyCode::Space) {
        c.vel_y = 4.6;
        c.on_ground = false;
    }
    c.vel_y -= 13.0 * dt;
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

    if c.moving && c.on_ground {
        c.bob += dt * if c.sprinting { 13.0 } else { 9.0 };
    }
    let bob = (c.bob).sin() * 0.035 * if c.moving { 1.0 } else { 0.0 };
    t.translation = Vec3::new(x, EYE + c.feet_y + bob, z);

}

fn regen(time: Res<Time>, mut hp: ResMut<Health>) {
    let dt = time.delta_secs();
    hp.since_hit += dt;
    hp.flash = (hp.flash - dt * 1.5).max(0.0);
    if hp.since_hit > rules::REGEN_DELAY {
        hp.hp = (hp.hp + rules::REGEN_PER_SEC * dt).min(rules::PLAYER_MAX_HEALTH);
    }
}

/// Damage the player. Returns true if this hit downed them.
pub fn damage_player(hp: &mut Health, amount: f32, sfx: &mut EventWriter<PlaySfx>) -> bool {
    hp.hp -= amount;
    hp.since_hit = 0.0;
    hp.flash = 1.0;
    sfx.write(PlaySfx::new(Sfx::PlayerHurt));
    hp.hp <= 0.0
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

fn apply_exposure(settings: Res<UserSettings>, mut cams: Query<&mut Exposure, With<Player>>) {
    if settings.is_changed() {
        if let Ok(mut e) = cams.single_mut() {
            e.ev100 = settings.exposure_ev;
        }
    }
}
