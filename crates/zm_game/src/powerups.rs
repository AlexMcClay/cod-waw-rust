//! Kill bookkeeping and power-up drops.

use crate::audio::{PlaySfx, Sfx};
use crate::player::Player;
use crate::weapons::Loadout;
use crate::world::{spawn_board, Board, Mats};
use crate::zombies::{self, Zombie};
use crate::{earn, ActivePowerups, Banner, Boards, Defs, Dynamic, GameState, LevelRes, PointsEvent, Round, Score, ZombieKilled};
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use zm_core::rules::{self, Powerup};

#[derive(Component)]
pub struct Drop {
    pub kind: Powerup,
    pub ttl: f32,
    pub base_y: f32,
}

pub struct PowerupsPlugin;

impl Plugin for PowerupsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, on_killed)
            .add_systems(Update, (animate_drops, pickup, tick_timers).chain().run_if(in_state(GameState::Playing)));
    }
}

#[allow(clippy::too_many_arguments)]
fn on_killed(
    mut events: EventReader<ZombieKilled>,
    mut round: ResMut<Round>,
    mut score: ResMut<Score>,
    player: Query<&Transform, With<Player>>,
    mut sfx: EventWriter<PlaySfx>,
    mut commands: Commands,
    mats: Res<Mats>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let ppos = player.single().map(|t| t.translation).unwrap_or_default();
    for ev in events.read() {
        round.0.on_zombie_killed();
        score.kills += 1;
        let vol = (1.0 - ev.pos.distance(ppos) / 25.0).clamp(0.1, 0.9);
        sfx.write(PlaySfx::at(Sfx::ZombieDeath, vol));
        if ev.drop_allowed && round.0.should_drop(fastrand::f32()) {
            let kind = Powerup::ALL[fastrand::usize(..Powerup::ALL.len())];
            spawn_drop(&mut commands, &mats, &mut meshes, kind, Vec3::new(ev.pos.x, 0.0, ev.pos.z));
            sfx.write(PlaySfx::new(Sfx::PowerupSpawn));
        }
    }
}

fn spawn_drop(commands: &mut Commands, mats: &Mats, meshes: &mut Assets<Mesh>, kind: Powerup, at: Vec3) {
    // Each power-up gets a distinct, simple glowing shape.
    let (mesh, mat, scale): (Handle<Mesh>, Handle<StandardMaterial>, Vec3) = match kind {
        Powerup::MaxAmmo => (mats.cube.clone(), mats.glow_green.clone(), Vec3::new(0.45, 0.3, 0.3)),
        Powerup::InstaKill => (mats.sphere.clone(), mats.glow_red.clone(), Vec3::splat(0.45)),
        Powerup::DoublePoints => (meshes.add(Torus::new(0.12, 0.24)), mats.glow_gold.clone(), Vec3::ONE),
        Powerup::Nuke => (meshes.add(Capsule3d::new(0.14, 0.35)), mats.glow_green.clone(), Vec3::ONE),
        Powerup::Carpenter => (mats.cube.clone(), mats.glow_blue.clone(), Vec3::new(0.15, 0.55, 0.15)),
    };
    let y = 0.9;
    commands
        .spawn((
            Transform::from_translation(at + Vec3::Y * y),
            Visibility::default(),
            Drop { kind, ttl: rules::POWERUP_TTL, base_y: y },
            Dynamic,
        ))
        .with_children(|p| {
            p.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_scale(scale), NotShadowCaster));
            if kind == Powerup::Carpenter {
                p.spawn((
                    Mesh3d(mats.cube.clone()),
                    MeshMaterial3d(mats.glow_blue.clone()),
                    Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(0.4, 0.12, 0.12)),
                    NotShadowCaster,
                ));
            }
            p.spawn((
                PointLight { color: Color::srgb(0.4, 1.0, 0.4), intensity: 40_000.0, range: 4.0, ..default() },
                Transform::default(),
            ));
        });
}

fn animate_drops(time: Res<Time>, mut commands: Commands, mut q: Query<(Entity, &mut Transform, &mut Visibility, &mut Drop)>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (e, mut tr, mut vis, mut d) in &mut q {
        d.ttl -= dt;
        if d.ttl <= 0.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        tr.rotation = Quat::from_rotation_y(t * 2.0);
        tr.translation.y = d.base_y + (t * 3.0).sin() * 0.1;
        *vis = if d.ttl < 6.0 && (d.ttl * if d.ttl < 3.0 { 10.0 } else { 5.0 }).sin() < 0.0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn pickup(
    mut commands: Commands,
    drops: Query<(Entity, &Transform, &Drop), Without<Player>>,
    player: Query<&Transform, With<Player>>,
    mut zq: Query<(Entity, &mut Transform, &mut Zombie), (Without<Player>, Without<Drop>)>,
    (defs, level, mats): (Res<Defs>, Res<LevelRes>, Res<Mats>),
    (mut loadout, mut pu, mut score, mut boards): (ResMut<Loadout>, ResMut<ActivePowerups>, ResMut<Score>, ResMut<Boards>),
    board_q: Query<&Board>,
    (mut sfx, mut points, mut banner, mut killed): (EventWriter<PlaySfx>, EventWriter<PointsEvent>, EventWriter<Banner>, EventWriter<ZombieKilled>),
) {
    let Ok(pt) = player.single() else { return };
    let me = Vec2::new(pt.translation.x, pt.translation.z);
    for (e, t, d) in &drops {
        if me.distance(Vec2::new(t.translation.x, t.translation.z)) > 1.3 {
            continue;
        }
        commands.entity(e).try_despawn();
        sfx.write(PlaySfx::new(Sfx::PowerupGrab));
        banner.write(Banner(format!("{}!", d.kind.label().to_uppercase())));
        match d.kind {
            Powerup::MaxAmmo => {
                loadout.refill_all(&defs.0);
                sfx.write(PlaySfx::new(Sfx::MaxAmmo));
            }
            Powerup::InstaKill => {
                pu.insta_kill = rules::POWERUP_DURATION;
                sfx.write(PlaySfx::new(Sfx::InstaKill));
            }
            Powerup::DoublePoints => {
                pu.double_points = rules::POWERUP_DURATION;
                sfx.write(PlaySfx::new(Sfx::DoublePoints));
            }
            Powerup::Nuke => {
                for pos in zombies::kill_all(&mut zq) {
                    killed.write(ZombieKilled { pos, drop_allowed: false });
                }
                earn(&mut score, &mut points, &pu, rules::POINTS_NUKE);
                sfx.write(PlaySfx::new(Sfx::Nuke));
            }
            Powerup::Carpenter => {
                for (wi, n) in boards.0.iter_mut().enumerate() {
                    let full = level.0.windows[wi].boards;
                    for idx in *n..full {
                        // Avoid duplicates if a board entity somehow survived.
                        if !board_q.iter().any(|b| b.window == wi && b.index == idx) {
                            spawn_board(&mut commands, &level.0, &mats, wi, idx);
                        }
                    }
                    *n = full;
                }
                earn(&mut score, &mut points, &pu, rules::POINTS_CARPENTER);
                sfx.write(PlaySfx::new(Sfx::Carpenter));
            }
        }
    }
}

fn tick_timers(time: Res<Time>, mut pu: ResMut<ActivePowerups>) {
    let dt = time.delta_secs();
    pu.insta_kill = (pu.insta_kill - dt).max(0.0);
    pu.double_points = (pu.double_points - dt).max(0.0);
}
