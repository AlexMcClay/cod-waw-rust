//! Round flow, game over, and session setup/teardown (new game, restart,
//! quit to menu).

use crate::audio::{PlaySfx, Sfx};
use crate::interact::MysteryCrate;
use crate::player::{self, Health, Player, PlayerCtl};
use crate::weapons::{Gun, Loadout, Tracers};
use crate::world::{self, Mats};
use crate::zombies;
use crate::menu::{CurrentMap, MapKind};
use crate::nacht::{self, NachtActive, NachtAssets, NachtError};
use crate::{ActivePowerups, Banner, Boards, Defs, Dynamic, GameState, LevelRes, Round, Score, SessionEntity, World};
use zm_core::level::Level;
use bevy::prelude::*;
use zm_core::rules::RoundEvent;

pub struct RoundPlugin;

impl Plugin for RoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, tick.run_if(in_state(GameState::Playing)))
            .add_systems(OnEnter(GameState::GameOver), game_over)
            .add_systems(OnEnter(GameState::MainMenu), reset_session)
            .add_systems(OnEnter(GameState::Loading), reset_session)
            .add_systems(Update, finish_loading.run_if(in_state(GameState::Loading)));
    }
}

#[allow(clippy::too_many_arguments)]
fn tick(
    time: Res<Time>,
    mut round: ResMut<Round>,
    level: Res<LevelRes>,
    world: Res<World>,
    mats: Res<Mats>,
    mut commands: Commands,
    mut sfx: EventWriter<PlaySfx>,
    mut banner: EventWriter<Banner>,
    models: Option<Res<nacht::ZombieModels>>,
) {
    match round.0.tick(time.delta_secs()) {
        RoundEvent::Spawn => {
            let r = zombies::round_of(&round);
            if zombies::spawn_zombie(&mut commands, &mats, &level, &world, r, models.as_deref()).is_none() {
                // No open window (shouldn't happen) - give the zombie back.
                round.0.to_spawn += 1;
                round.0.alive -= 1;
            } else if fastrand::f32() < 0.35 {
                sfx.write(PlaySfx::at(Sfx::ZombieSpawn, 0.35));
            }
        }
        RoundEvent::RoundStarted(r) => {
            banner.write(Banner(format!("ROUND {r}")));
            sfx.write(PlaySfx::new(if r == 1 { Sfx::GameStart } else { Sfx::RoundStart }));
        }
        RoundEvent::RoundEnded(r) => {
            banner.write(Banner(format!("ROUND {r} SURVIVED")));
            sfx.write(PlaySfx::new(Sfx::RoundEnd));
        }
        RoundEvent::None => {}
    }
}

fn game_over(mut sfx: EventWriter<PlaySfx>) {
    sfx.write(PlaySfx::new(Sfx::GameOver));
}

/// Ends any running session: despawns the level and everything spawned
/// during play, and resets all per-game state.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn reset_session(
    mut commands: Commands,
    dynamic: Query<Entity, Or<(With<Dynamic>, With<SessionEntity>)>>,
    level: Res<LevelRes>,
    defs: Res<Defs>,
    mut player_q: Query<(&mut Transform, &mut PlayerCtl), With<Player>>,
    (mut score, mut round, mut pu, mut boards, mut world, mut health): (
        ResMut<Score>,
        ResMut<Round>,
        ResMut<ActivePowerups>,
        ResMut<Boards>,
        ResMut<World>,
        ResMut<Health>,
    ),
    (mut loadout, mut gun, mut mcrate, mut tracers): (ResMut<Loadout>, ResMut<Gun>, ResMut<MysteryCrate>, ResMut<Tracers>),
) {
    for e in &dynamic {
        commands.entity(e).try_despawn();
    }
    *score = Score::default();
    *round = Round::default();
    *pu = ActivePowerups::default();
    *boards = Boards(level.0.windows.iter().map(|w| w.boards).collect());
    *world = World::new(&level.0);
    *health = Health::default();
    *loadout = Loadout::starting(&defs.0);
    // Developer aid: start holding a given weapon.
    if let Some(i) = std::env::var("UNDEAD_WEAPON").ok().and_then(|w| zm_core::weapons::find(&defs.0, &w)) {
        loadout.give(&defs.0, i);
    }
    *gun = Gun::default();
    *mcrate = MysteryCrate::default();
    tracers.0.clear();
    player::reset_player(&mut player_q, &level);
}

/// Time since the scene was spawned, and frames with no shader compiling.
#[derive(Resource)]
struct Warmup(f32, u32);

/// Builds the chosen map and starts playing once it is ready.
fn finish_loading(
    mut commands: Commands,
    current: Res<CurrentMap>,
    nacht_assets: Option<Res<NachtAssets>>,
    error: Res<NachtError>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<GameState>>,
    mut warmup: Option<ResMut<Warmup>>,
    time: Res<Time>,
    pending: Res<nacht::PendingPipelines>,
) {
    match current.0 {
        MapKind::Bunker => {
            commands.remove_resource::<NachtActive>();
            commands.insert_resource(LevelRes(Level::bunker()));
            commands.run_system_cached(start_session);
            commands.run_system_cached(world::spawn_static);
            commands.run_system_cached(world::spawn_dynamic);
            next.set(GameState::Playing);
        }
        MapKind::Nacht => {
            if let Some(w) = warmup.as_mut() {
                // The scene is spawned behind the loading screen; reveal it
                // once the GPU has compiled its shaders (or after a cap).
                w.0 += time.delta_secs();
                if pending.count() == 0 {
                    w.1 += 1;
                } else {
                    w.1 = 0;
                }
                if (w.0 > 0.5 && w.1 > 10) || w.0 > 45.0 {
                    commands.remove_resource::<Warmup>();
                    next.set(GameState::Playing);
                }
            } else if let Some(n) = nacht_assets {
                commands.insert_resource(NachtActive);
                commands.insert_resource(LevelRes(n.level.clone()));
                commands.run_system_cached(start_session);
                commands.run_system_cached(nacht::spawn_scene);
                commands.run_system_cached(nacht::spawn_dynamic);
                commands.insert_resource(Warmup(0.0, 0));
            } else if error.0.is_some() && keys.just_pressed(KeyCode::Escape) {
                next.set(GameState::MainMenu);
            }
        }
    }
}

/// Per-map session state once the level is known.
#[allow(clippy::too_many_arguments)]
fn start_session(
    level: Res<LevelRes>,
    nacht_assets: Option<Res<NachtAssets>>,
    active: Option<Res<NachtActive>>,
    mut boards: ResMut<Boards>,
    mut world: ResMut<World>,
    mut player_q: Query<(&mut Transform, &mut PlayerCtl), With<Player>>,
    mut ambient: ResMut<AmbientLight>,
    mut clear: ResMut<ClearColor>,
) {
    *boards = Boards(level.0.windows.iter().map(|w| w.boards).collect());
    *world = World::new(&level.0);
    match (active, nacht_assets) {
        (Some(_), Some(n)) => {
            world.mesh = Some(n.collision.clone());
            world.graph = Some(n.nav.clone());
            // Routes from the field outside to every window.
            let mesh = n.collision.clone();
            world.window_fields = (0..level.0.windows.len())
                .map(|wi| {
                    let o = zombies::outside_of(&level.0, wi);
                    let g = zombies::ground_y(&world, o.x, o.z, o.y);
                    let here = zm_core::geom::V3::new(o.x, g, o.z);
                    let node = n.nav.nearest(here, |p| {
                        mesh.line_clear(zm_core::geom::V3::new(here.x, here.y + 0.8, here.z), zm_core::geom::V3::new(p.x, p.y + 0.8, p.z))
                    });
                    node.map(|nd| n.nav.field(nd, &[])).unwrap_or_default()
                })
                .collect();
        }
        _ => {
            ambient.color = Color::srgb(0.6, 0.65, 0.8);
            ambient.brightness = 120.0;
            clear.0 = Color::srgb(0.02, 0.025, 0.04);
        }
    }
    player::reset_player(&mut player_q, &level);
}
