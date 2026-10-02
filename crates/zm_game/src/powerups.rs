//! Kill bookkeeping and power-up drops.

use crate::audio::{AliasLoop, PlayAlias, PlaySfx, Sfx, ZoneSounds};
use crate::player::Player;
use crate::weapons::Loadout;
use crate::world::{spawn_board, Board, Mats};
use crate::zombies::{self, Zombie};
use crate::{earn, ActivePowerups, Banner, Boards, Defs, Dynamic, GameState, LevelRes, PointsEvent, Round, Score, ZombieKilled};
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use zm_core::rules::{self, Powerup};

/// A power-up was picked up (the HUD shows its text or flash).
#[derive(Event)]
pub struct PowerupGrabbed(pub Powerup);

/// The looping sound while a timed power-up runs.
#[derive(Component)]
struct PowerupLoop(Powerup);

#[derive(Component)]
pub struct Drop {
    pub kind: Powerup,
    pub ttl: f32,
    pub base_y: f32,
}

pub struct PowerupsPlugin;

impl Plugin for PowerupsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<PowerupGrabbed>()
            .add_systems(Update, on_killed)
            .add_systems(Update, (animate_drops, pickup, tick_timers).chain().run_if(in_state(GameState::Playing)));
    }
}

#[allow(clippy::too_many_arguments)]
fn on_killed(
    mut events: EventReader<ZombieKilled>,
    mut round: ResMut<Round>,
    mut score: ResMut<Score>,
    player: Query<&Transform, With<Player>>,
    (mut sfx, mut alias, zs): (EventWriter<PlaySfx>, EventWriter<PlayAlias>, Res<ZoneSounds>),
    nacht: Option<Res<crate::nacht::NachtActive>>,
    mut commands: Commands,
    mats: Res<Mats>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let ppos = player.single().map(|t| t.translation).unwrap_or_default();
    for ev in events.read() {
        round.0.on_zombie_killed();
        score.kills += 1;
        // Zombies die silently in Nacht; the stand-in is for the prototype.
        if zs.aliases.is_empty() {
            let vol = (1.0 - ev.pos.distance(ppos) / 25.0).clamp(0.1, 0.9);
            sfx.write(PlaySfx::at(Sfx::ZombieDeath, vol));
        }
        if ev.drop_allowed && round.0.should_drop(fastrand::f32()) {
            // Nacht has no carpenter.
            let pool: Vec<Powerup> = Powerup::ALL.into_iter().filter(|k| nacht.is_none() || *k != Powerup::Carpenter).collect();
            let kind = pool[fastrand::usize(..pool.len())];
            let at = Vec3::new(ev.pos.x, 0.0, ev.pos.z);
            let drop = spawn_drop(&mut commands, &mats, &mut meshes, kind, at);
            commands.entity(drop).insert(AliasLoop { alias: "spawn_powerup_loop".into(), volume: 1.0 });
            alias.write(PlayAlias::at("spawn_powerup", at + Vec3::Y).or(Sfx::PowerupSpawn));
        }
    }
}

fn spawn_drop(commands: &mut Commands, mats: &Mats, meshes: &mut Assets<Mesh>, kind: Powerup, at: Vec3) -> Entity {
    // Each power-up gets a distinct, simple glowing shape.
    let (mesh, mat, scale): (Handle<Mesh>, Handle<StandardMaterial>, Vec3) = match kind {
        Powerup::MaxAmmo => (mats.cube.clone(), mats.glow_green.clone(), Vec3::new(0.45, 0.3, 0.3)),
        Powerup::InstaKill => (mats.sphere.clone(), mats.glow_red.clone(), Vec3::splat(0.45)),
        Powerup::DoublePoints => (meshes.add(Torus::new(0.12, 0.24)), mats.glow_gold.clone(), Vec3::ONE),
        Powerup::Nuke => (meshes.add(Capsule3d::new(0.14, 0.35)), mats.glow_green.clone(), Vec3::ONE),
        Powerup::Carpenter => (mats.cube.clone(), mats.glow_blue.clone(), Vec3::new(0.15, 0.55, 0.15)),
    };
    let y = 0.9;
    let mut e = commands.spawn((
            Transform::from_translation(at + Vec3::Y * y),
            Visibility::default(),
            Drop { kind, ttl: rules::POWERUP_TTL, base_y: y },
            Dynamic,
        ));
    e.with_children(|p| {
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
    e.id()
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
    (mut alias, mut points, mut banner, mut killed): (EventWriter<PlayAlias>, EventWriter<PointsEvent>, EventWriter<Banner>, EventWriter<ZombieKilled>),
    loops: Query<&PowerupLoop>,
    mut grabbed: EventWriter<PowerupGrabbed>,
) {
    let Ok(pt) = player.single() else { return };
    let me = Vec2::new(pt.translation.x, pt.translation.z);
    for (e, t, d) in &drops {
        if me.distance(Vec2::new(t.translation.x, t.translation.z)) > 1.3 {
            continue;
        }
        commands.entity(e).try_despawn();
        alias.write(PlayAlias::at("powerup_grabbed", t.translation).after(0.1).or(Sfx::PowerupGrab));
        let mut start_loop = |kind: Powerup, name: &str| {
            if !loops.iter().any(|l| l.0 == kind) {
                commands.spawn((AliasLoop { alias: name.into(), volume: 1.0 }, PowerupLoop(kind), Dynamic));
            }
        };
        banner.write(Banner(format!("{}!", d.kind.label().to_uppercase())));
        grabbed.write(PowerupGrabbed(d.kind));
        match d.kind {
            Powerup::MaxAmmo => {
                loadout.refill_all(&defs.0);
                alias.write(PlayAlias::local("full_ammo").volume(0.7).or(Sfx::MaxAmmo));
            }
            Powerup::InstaKill => {
                pu.insta_kill = rules::POWERUP_DURATION;
                start_loop(Powerup::InstaKill, "insta_kill_loop");
                alias.write(PlayAlias::local("").or(Sfx::InstaKill));
            }
            Powerup::DoublePoints => {
                pu.double_points = rules::POWERUP_DURATION;
                start_loop(Powerup::DoublePoints, "double_point_loop");
                alias.write(PlayAlias::local("").or(Sfx::DoublePoints));
            }
            Powerup::Nuke => {
                alias.write(PlayAlias::local("nuke_flash").or(Sfx::Nuke));
                let mut dead = zombies::kill_all(&mut zq);
                dead.sort_by(|a, b| a.distance(pt.translation).total_cmp(&b.distance(pt.translation)));
                // Heads pop one after another, closest first.
                let mut when = 0.0;
                for pos in dead {
                    killed.write(ZombieKilled { pos, drop_allowed: false });
                    when += 0.1 + fastrand::f32() * 0.6;
                    alias.write(PlayAlias::at("nuked", pos + Vec3::Y).after(when));
                    alias.write(PlayAlias::at("zombie_head_gib", pos + Vec3::Y * 1.6).after(when));
                }
                earn(&mut score, &mut points, &pu, rules::POINTS_NUKE);
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
                alias.write(PlayAlias::local("").or(Sfx::Carpenter));
            }
        }
    }
}

fn tick_timers(time: Res<Time>, mut pu: ResMut<ActivePowerups>, loops: Query<(Entity, &PowerupLoop)>, mut alias: EventWriter<PlayAlias>, mut commands: Commands) {
    let dt = time.delta_secs();
    let mut ended = Vec::new();
    if pu.insta_kill > 0.0 && pu.insta_kill <= dt {
        ended.push((Powerup::InstaKill, "insta_kill"));
    }
    if pu.double_points > 0.0 && pu.double_points <= dt {
        ended.push((Powerup::DoublePoints, "points_loop_off"));
    }
    pu.insta_kill = (pu.insta_kill - dt).max(0.0);
    pu.double_points = (pu.double_points - dt).max(0.0);
    // The loop stops and the "worn off" sound plays.
    for (kind, sound) in ended {
        for (e, l) in &loops {
            if l.0 == kind {
                commands.entity(e).despawn();
            }
        }
        alias.write(PlayAlias::local(sound));
    }
}
