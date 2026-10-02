//! Undead Rounds — a round-based zombie survival game built with Bevy.
//!
//! All gameplay code here is original. The game reads art, sounds, weapon
//! stats and (for Nacht der Untoten) the map itself from the user's own World
//! at War install at runtime; without one it falls back to procedural sounds,
//! the built-in weapon table and the original bunker map.

// No console window for release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod fx;
mod gibs;
mod grenades;
mod hud;
mod interact;
mod menu;
mod nacht;
mod player;
mod panning;
mod postfx;
mod powerups;
mod round;
mod settings;
mod waw;
mod weapons;
mod world;
mod xwma;
mod zombies;

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use zm_core::geom::Aabb;
use zm_core::level::Level;
use zm_core::nav::NavGrid;
use zm_core::rules::{self, RoundState};
use zm_core::weapons::WeaponDef;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    MainMenu,
    /// Tearing down the previous session and building the chosen map.
    Loading,
    Playing,
    Paused,
    GameOver,
}

/// Static level entities, despawned when a session ends.
#[derive(Component)]
pub struct SessionEntity;

/// Static level description.
#[derive(Resource)]
pub struct LevelRes(pub Level);

/// Door state + derived colliders and navigation.
#[derive(Resource)]
pub struct World {
    pub door_open: Vec<bool>,
    /// Wall weapons bought at least once (their chalk drawing slid out).
    pub wall_bought: Vec<bool>,
    /// What blocks the player (walls, window fills, crate, closed doors).
    pub player_solids: Vec<Aabb>,
    /// What stops bullets (walls, closed doors, crate) — windows are open.
    pub bullet_solids: Vec<Aabb>,
    pub nav: NavGrid,
    pub field: Vec<f32>,
    pub field_timer: f32,
    /// Real-map collision (triangle soup), if the map comes from the install.
    pub mesh: Option<std::sync::Arc<zm_core::trimesh::TriMesh>>,
    /// Real-map navigation over path nodes (replaces `nav`).
    pub graph: Option<std::sync::Arc<zm_core::navgraph::NavGraph>>,
    /// Per window: distances over the graph to the spot outside it.
    pub window_fields: Vec<Vec<f32>>,
}

impl World {
    pub fn new(level: &Level) -> Self {
        let mut w = World {
            door_open: vec![false; level.doors.len()],
            wall_bought: vec![false; level.wall_buys.len()],
            player_solids: Vec::new(),
            bullet_solids: Vec::new(),
            nav: NavGrid::build(level, 0.3, &[]),
            field: Vec::new(),
            field_timer: 0.0,
            mesh: None,
            graph: None,
            window_fields: Vec::new(),
        };
        w.rebuild(level);
        w
    }

    pub fn rebuild(&mut self, level: &Level) {
        self.player_solids = level.player_colliders();
        self.bullet_solids = level.walls.clone();
        if level.crate_solid {
            self.bullet_solids.push(level.crate_box);
        }
        for (i, d) in level.doors.iter().enumerate() {
            if !self.door_open[i] {
                self.player_solids.push(d.blocker);
                self.bullet_solids.push(d.blocker);
            }
        }
        self.nav = NavGrid::build(level, 0.3, &self.door_open);
        self.field_timer = 0.0;
    }

    /// Whether zombies may spawn at windows of this area.
    pub fn area_open(&self, level: &Level, area: usize) -> bool {
        area == 0
            || level
                .doors
                .iter()
                .enumerate()
                .any(|(i, d)| d.opens == area && self.door_open[i])
    }
}

#[derive(Resource)]
pub struct Score {
    pub points: u32,
    pub kills: u32,
    pub headshots: u32,
    /// Points ever earned from zombies (`score_total`, starting points
    /// included): drives the power-up drop trigger. Repairs don't count.
    pub total: u32,
}

impl Default for Score {
    fn default() -> Self {
        Score { points: rules::POINTS_START, kills: 0, headshots: 0, total: rules::POINTS_START }
    }
}

#[derive(Resource, Default)]
pub struct Round(pub RoundState);

/// The loaded map's zombie rules (from its scripts); sessions on it start
/// with these.
#[derive(Resource, Clone)]
pub struct MapRules(pub rules::ZombieRules);

#[derive(Resource, Default)]
pub struct ActivePowerups {
    pub insta_kill: f32,
    pub double_points: f32,
    /// Double Points picked up while one ran: on the World at War maps
    /// each one doubles the points again (x4, x8...) until the timer ends.
    pub double_stacks: u32,
}

impl ActivePowerups {
    pub fn double(&self) -> bool {
        self.double_points > 0.0
    }

    /// `zombie_point_scalar`.
    pub fn point_scalar(&self) -> u32 {
        if self.double() {
            1 << self.double_stacks.clamp(1, 8)
        } else {
            1
        }
    }
}

/// Boards remaining per window.
#[derive(Resource)]
pub struct Boards(pub Vec<u8>);

#[derive(Resource)]
pub struct Defs(pub Vec<WeaponDef>);

/// Marker for entities that are rebuilt on restart.
#[derive(Component)]
pub struct Dynamic;

/// Points change, shown as a floating "+10" on the HUD.
#[derive(Event)]
pub struct PointsEvent(pub i32);

/// Big centre-screen message (round start, power-up names...).
#[derive(Event)]
pub struct Banner(pub String);

/// A zombie died (round bookkeeping, drops).
#[derive(Event)]
pub struct ZombieKilled {
    pub pos: Vec3,
    pub drop_allowed: bool,
}

/// Spend points; returns false (and changes nothing) if too poor.
pub fn try_spend(score: &mut Score, ev: &mut EventWriter<PointsEvent>, cost: u32) -> bool {
    if score.points < cost {
        return false;
    }
    score.points -= cost;
    ev.write(PointsEvent(-(cost as i32)));
    true
}

/// Points from zombies (hits, kills): scaled by Double Points and counted
/// towards the power-up drop trigger.
pub fn earn(score: &mut Score, ev: &mut EventWriter<PointsEvent>, pu: &ActivePowerups, base: u32) {
    let p = base * pu.point_scalar();
    score.points += p;
    score.total += p;
    ev.write(PointsEvent(p as i32));
}

/// Points added as they are (board repairs: `add_to_player_score`).
pub fn earn_flat(score: &mut Score, ev: &mut EventWriter<PointsEvent>, p: u32) {
    if p == 0 {
        return;
    }
    score.points += p;
    ev.write(PointsEvent(p as i32));
}

pub fn v3(v: zm_core::geom::V3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn main() {
    install_panic_hook();
    let launch = settings::LaunchConfig::load();
    let user = settings::UserSettings::load();
    let waw = waw::Waw::locate(&launch);
    let assets = audio::AssetDir::locate();
    let mut defs = zm_core::weapons::default_weapons();
    let overrides = assets.apply_weapon_files(&waw, &mut defs);
    let level = Level::bunker();
    let world = World::new(&level);
    let boards = Boards(level.windows.iter().map(|w| w.boards).collect());

    let start_map = match std::env::var("UNDEAD_START").ok().as_deref() {
        Some("nacht") => Some(menu::MapKind::Nacht),
        Some("bunker") => Some(menu::MapKind::Bunker),
        _ => None,
    };
    let mode = if user.fullscreen {
        bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current)
    } else {
        bevy::window::WindowMode::Windowed
    };

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Undead Rounds".into(),
                        resolution: (1600.0_f32, 900.0_f32).into(),
                        mode,
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::log::LogPlugin { custom_layer: file_log_layer, ..default() }),
        )
        .init_state::<GameState>()
        .insert_resource(launch)
        .insert_resource(user)
        .insert_resource(waw)
        .init_resource::<waw::WawImages>()
        .insert_resource(menu::CurrentMap(start_map.unwrap_or(menu::MapKind::Nacht)))
        .insert_resource(ClearColor(Color::srgb(0.02, 0.025, 0.04)))
        .insert_resource(AmbientLight {
            color: Color::srgb(0.6, 0.65, 0.8),
            brightness: 120.0,
            ..default()
        })
        .insert_resource(LevelRes(level))
        .insert_resource(world)
        .insert_resource(boards)
        .insert_resource(weapons::Loadout::starting(&defs))
        .insert_resource(Defs(defs))
        .insert_resource(assets.with_override_count(overrides))
        .init_resource::<Score>()
        .init_resource::<Round>()
        .init_resource::<ActivePowerups>()
        .add_event::<PointsEvent>()
        .add_event::<Banner>()
        .add_event::<ZombieKilled>()
        .add_plugins((
            audio::AudioPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            weapons::WeaponsPlugin,
            grenades::GrenadesPlugin,
            zombies::ZombiesPlugin,
            interact::InteractPlugin,
            powerups::PowerupsPlugin,
            round::RoundPlugin,
            hud::HudPlugin,
            menu::MenuPlugin,
            nacht::NachtPlugin,
            postfx::PostFxPlugin,
            fx::FxPlugin,
            gibs::GibsPlugin,
        ))
        .add_systems(Startup, move |mut next: ResMut<NextState<GameState>>| {
            if start_map.is_some() {
                next.set(GameState::Loading);
            }
        })
        .add_systems(Update, (cursor_grab, debug_capture, collision_map))
        .add_systems(Update, log_state)
        .add_systems(PreUpdate, (watch_for_user, autopilot.run_if(autopilot_enabled)).chain().after(bevy::input::InputSystem))
        .run();
}

fn log_state(state: Res<State<GameState>>) {
    if state.is_changed() {
        info!("state -> {:?}", state.get());
    }
}

/// The mouse is captured while playing and free in every menu.
fn cursor_grab(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    state: Res<State<GameState>>,
) {
    let Ok(mut window) = windows.single_mut() else { return };
    let playing = *state.get() == GameState::Playing;
    let grabbed = window.cursor_options.grab_mode != CursorGrabMode::None;
    if !playing && grabbed {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
    } else if playing && !grabbed && (state.is_changed() || mouse.just_pressed(MouseButton::Left)) && window.focused {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
    }
}

/// Mirrors the log to `%LOCALAPPDATA%\UndeadRounds\log.txt` (release builds
/// have no console).
fn file_log_layer(_app: &mut App) -> Option<bevy::log::BoxedLayer> {
    let file = std::fs::File::create(settings::data_dir().join("log.txt")).ok()?;
    Some(Box::new(
        bevy::log::tracing_subscriber::fmt::layer()
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false),
    ))
}

/// Writes panics to `crash.txt` so failures are visible without a console.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::write(settings::data_dir().join("crash.txt"), format!("{info}\n"));
        default(info);
    }));
}

/// True when the developer smoke-test mode (`UNDEAD_CAPTURE`) is active.
/// Developer aid: `UNDEAD_COLLISION_MAP=<file.ppm>` writes a top-down map
/// (5 cm pixels, 50 m around the player start, at the start's floor) of what
/// stops the player once the map has loaded: red = clip-only collision
/// with nothing visible within 15 cm, grey = blocked by visible geometry, blue = gameplay boxes (windows,
/// closed doors), black = no floor.
fn collision_map(world: Res<World>, player: Query<(&Transform, &player::PlayerCtl), With<player::Player>>, mut done: Local<bool>) {
    use zm_core::trimesh::blocks;
    if *done {
        return;
    }
    let Some(path) = std::env::var_os("UNDEAD_COLLISION_MAP") else { return };
    let (Some(mesh), Ok((t, c))) = (world.mesh.as_ref(), player.single()) else { return };
    *done = true;
    let (n, px) = (1000usize, 0.05f32);
    let floor = mesh.ground_mask(t.translation.x, t.translation.z, c.feet_y + 0.5, c.feet_y - 5.0, 0.7, blocks::PLAYER).unwrap_or(c.feet_y);
    let (x0, z0) = (t.translation.x - n as f32 * px * 0.5, t.translation.z - n as f32 * px * 0.5);
    let mut img = vec![0u8; n * n * 3];
    for j in 0..n {
        for i in 0..n {
            let (x, z) = (x0 + i as f32 * px, z0 + j as f32 * px);
            let Some(g) = mesh.ground_mask(x, z, floor + 0.8, floor - 0.8, 0.7, blocks::PLAYER) else { continue };
            let (y0, y1) = (g + 0.45, g + 1.7);
            let clip = mesh.push_cylinder_mask(x, z, 0.03, y0, y1, 1, blocks::PLAYER, |_| true).2;
            let seen = clip && mesh.push_cylinder_mask(x, z, 0.15, y0 - 0.2, y1, 1, blocks::SOLID, |_| true).2;
            let solid = world.player_solids.iter().any(|b| b.min.y < y1 && b.max.y > y0 && b.min.x <= x && b.max.x >= x && b.min.z <= z && b.max.z >= z);
            let rgb = if solid {
                [60, 90, 255]
            } else if clip && !seen {
                [255, 40, 40]
            } else if clip {
                [150, 150, 150]
            } else {
                [40, 40, 40]
            };
            img[(j * n + i) * 3..(j * n + i) * 3 + 3].copy_from_slice(&rgb);
        }
    }
    let mut out = format!("P6 {n} {n} 255
").into_bytes();
    out.extend(img);
    let _ = std::fs::write(&path, out);
    info!("collision map written: origin ({x0:.2}, {z0:.2}) {px} m/px, floor {floor:.2}");
}

/// Set once the player touches the keyboard or mouse during a test run
/// (`UNDEAD_CAPTURE` and the `UNDEAD_TEST_*` hooks): from then on the bot
/// and the test hooks stand down and the game plays normally.
static USER_TOOK_OVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn user_took_over() -> bool {
    USER_TOOK_OVER.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn autopilot_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("UNDEAD_CAPTURE").is_some()) && !user_took_over()
}

/// Hands a test run to the player on their first key press, click or mouse
/// movement while the window has focus.
fn watch_for_user(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    // `UNDEAD_NO_TAKEOVER`: automated runs that must not be interrupted.
    if user_took_over() || std::env::var_os("UNDEAD_NO_TAKEOVER").is_some() || std::env::var_os("UNDEAD_CAPTURE").is_none() && std::env::var_os("UNDEAD_TEST_WALK").is_none() {
        return;
    }
    let Ok(mut w) = windows.single_mut() else { return };
    if !w.focused {
        return;
    }
    let touched = keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some() || motion.delta.length() > 4.0;
    if touched {
        USER_TOOK_OVER.store(true, std::sync::atomic::Ordering::Relaxed);
        w.cursor_options.grab_mode = CursorGrabMode::Locked;
        w.cursor_options.visible = false;
        info!("[autopilot] the player took over: test hooks off");
    }
}

pub fn cursor_locked(windows: &Query<&Window, With<PrimaryWindow>>) -> bool {
    if autopilot_enabled() {
        return true;
    }
    windows
        .single()
        .map(|w| w.cursor_options.grab_mode != CursorGrabMode::None)
        .unwrap_or(false)
}

/// Developer aid: `UNDEAD_CAPTURE=prefix` saves screenshots at a few
/// timestamps and then quits. Lets the game be smoke-tested headlessly.
fn debug_capture(
    time: Res<Time>,
    mut commands: Commands,
    mut taken: Local<usize>,
    mut exit: EventWriter<AppExit>,
    mut player: Query<&mut player::PlayerCtl, With<player::Player>>,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let Ok(prefix) = std::env::var("UNDEAD_CAPTURE") else { return };
    let _ = &mut player;
    let at: Vec<f32> = std::env::var("UNDEAD_CAPTURE_AT")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![4.0, 9.0, 30.0, 45.0]);
    let t = time.elapsed_secs();
    if let Some(&when) = at.get(*taken) {
        if t >= when {
            let path = format!("{prefix}_{}.png", *taken);
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            *taken += 1;
        }
    } else if t > at.last().copied().unwrap_or(0.0) + 2.0 && !user_took_over() {
        exit.write(AppExit::Success);
    }
}

/// Smoke-test bot: aims at the nearest visible zombie's head and fires,
/// logging progress so a headless run exercises the whole game loop.
#[allow(clippy::too_many_arguments)]
fn autopilot(
    time: Res<Time>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut motion: ResMut<bevy::input::mouse::AccumulatedMouseMotion>,
    mut frame: Local<u32>,
    mut last_log: Local<f32>,
    world: Res<World>,
    round: Res<Round>,
    score: Res<Score>,
    mut health: ResMut<player::Health>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut player: Query<(&mut Transform, &mut player::PlayerCtl), With<player::Player>>,
    zq: Query<(&Transform, &zombies::Zombie), Without<player::Player>>,
    (level, mut mcrate): (Res<LevelRes>, ResMut<interact::MysteryCrate>),
) {
    *frame += 1;
    // Ignore real input while the bot plays (the window may have focus).
    keys.reset_all();
    mouse.reset_all();
    motion.delta = Vec2::ZERO;
    if std::env::var_os("UNDEAD_PREVIEW").is_some() {
        return;
    }
    let t = time.elapsed_secs();
    if t - *last_log > 5.0 {
        *last_log = t;
        info!(
            "[autopilot] t={t:.0}s state={:?} round={} alive={} to_spawn={} kills={} points={} hp={:.0} pos={:?}",
            state.get(), round.0.round, round.0.alive, round.0.to_spawn, score.kills, score.points, health.hp,
            player.single().map(|p| p.0.translation).ok()
        );
        let mut states: std::collections::BTreeMap<String, usize> = Default::default();
        for (zt, z) in &zq {
            *states.entry(format!("{:?}", z.state)).or_default() += 1;
            if z.state == zombies::ZState::Approach && states["Approach"] == 1 {
                info!("[autopilot]   approaching zombie at {:?} window {}", zt.translation, z.window);
            }
        }
        info!("[autopilot]   zombies {states:?}");
    }
    // Optional: force a death every 25 s to exercise game over + restart.
    let period = 25.0;
    if std::env::var_os("UNDEAD_TEST_DEATH").is_some() && t > period && (t / period).floor() != ((t - time.delta_secs()) / period).floor() {
        health.hp = 0.0;
        next.set(GameState::GameOver);
    }
    if *state.get() == GameState::GameOver {
        if *frame % 30 == 0 {
            keys.press(KeyCode::Enter);
        } else {
            keys.release(KeyCode::Enter);
        }
        return;
    }
    keys.release(KeyCode::Enter);
    // Optional: `UNDEAD_TEST_ADS=1` aims down the sights the whole time.
    if std::env::var_os("UNDEAD_TEST_ADS").is_some() {
        mouse.press(MouseButton::Right);
    }
    // Optional: `UNDEAD_TEST_LOOK="x y z yaw pitch"` (game units, degrees)
    // holds the camera there, for looking at parts of the map.
    if let Some(v) = std::env::var("UNDEAD_TEST_LOOK").ok().map(|s| s.split_whitespace().filter_map(|x| x.parse::<f32>().ok()).collect::<Vec<_>>()) {
        if let (Ok((mut pt, mut ctl)), [x, y, z, yaw, pitch]) = (player.single_mut(), v.as_slice()) {
            let p = Vec3::new(*x, *z, -*y) * 0.0254;
            ctl.feet_y = p.y - ctl.eye;
            ctl.vel_y = 0.0;
            pt.translation = p;
            ctl.yaw = (yaw - 90.0).to_radians();
            ctl.pitch = pitch.to_radians();
        }
        return;
    }
    // Optional: stand in front of the box and roll it at 5 s.
    if std::env::var_os("UNDEAD_TEST_BOX").is_some() {
        if let Ok((mut pt, mut ctl)) = player.single_mut() {
            let c = v3(level.0.crate_box.center());
            let (sx, sz) = level.0.player_start;
            let away = Vec3::new(sx - c.x, 0.0, sz - c.z).normalize_or_zero();
            let feet = Vec3::new(c.x, level.0.crate_box.min.y, c.z) + away * 1.7;
            ctl.feet_y = feet.y;
            pt.translation = feet + Vec3::Y * ctl.eye;
            let d = (c + Vec3::Y * 0.4 - pt.translation).normalize();
            ctl.yaw = (-d.x).atan2(-d.z);
            ctl.pitch = d.y.asin();
        }
        if t >= 5.0 && t - time.delta_secs() < 5.0 {
            mcrate.0 = interact::CrateState::Rolling { t: 0.0, result: 3, shown: 3, tick: 0.0 };
        }
        return;
    }
    // Optional: cycle the stances (crouch at 6 s, prone at 10 s, up at 14/16 s).
    if std::env::var_os("UNDEAD_TEST_STANCE").is_some() {
        let dt = time.delta_secs();
        for (at, key) in [(6.0, KeyCode::KeyC), (10.0, KeyCode::ControlLeft), (14.0, KeyCode::Space), (16.0, KeyCode::Space)] {
            if t >= at && t - dt < at {
                keys.press(key);
            }
        }
    }
    let Ok((pt, mut ctl)) = player.single_mut() else { return };
    let eye = pt.translation;
    let target = zq
        .iter()
        .filter(|(_, z)| z.alive() && matches!(z.state, zombies::ZState::Chase | zombies::ZState::AtWindow | zombies::ZState::Climbing))
        .map(|(zt, z)| zt.translation + Vec3::Y * 1.63 * z.scale)
        .filter(|h| {
            let d = *h - eye;
            weapons::wall_distance(&world, eye, d.normalize(), d.length()) >= d.length() - 0.05
        })
        .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
    if let Some(h) = target {
        let d = (h - eye).normalize();
        ctl.yaw = (-d.x).atan2(-d.z);
        ctl.pitch = d.y.asin();
        if *frame % 2 == 0 {
            mouse.press(MouseButton::Left);
        } else {
            mouse.release(MouseButton::Left);
        }
    } else {
        mouse.release(MouseButton::Left);
    }
}
