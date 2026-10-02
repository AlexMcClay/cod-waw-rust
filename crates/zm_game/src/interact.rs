//! "Press F" interactions: rebuilding barricades, buying wall weapons and
//! ammo, clearing debris, and the mystery crate.

use crate::audio::{PlaySfx, Sfx};
use crate::player::Player;
use crate::weapons::{Gun, Loadout};
use crate::world::{spawn_board, CrateDisplay, CrateLid, Debris, Mats};
use crate::{earn, try_spend, v3, ActivePowerups, Boards, Defs, GameState, LevelRes, PointsEvent, Score, World};
use bevy::prelude::*;
use zm_core::rules;
use zm_core::weapons;

#[derive(Resource, Default)]
pub struct Prompt(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CrateState {
    #[default]
    Idle,
    Rolling {
        t: f32,
        result: usize,
        shown: usize,
        tick: f32,
    },
    Ready {
        def: usize,
        t: f32,
    },
}

#[derive(Resource, Default)]
pub struct MysteryCrate(pub CrateState);

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Prompt>()
            .init_resource::<MysteryCrate>()
            .add_systems(Update, (interact, crate_tick).chain().run_if(in_state(GameState::Playing)))
            .add_systems(Update, crate_visuals);
    }
}

enum Target {
    Window(usize),
    WallBuy(usize),
    Door(usize),
    Crate,
}

fn roll_crate(defs: &[weapons::WeaponDef], loadout: &Loadout) -> usize {
    let pool: Vec<(usize, u32)> =
        weapons::crate_pool(defs).into_iter().filter(|(i, _)| loadout.has(*i).is_none()).collect();
    let total: u32 = pool.iter().map(|p| p.1).sum();
    if total == 0 {
        return weapons::START_PISTOL;
    }
    let mut r = fastrand::u32(..total);
    for (i, w) in &pool {
        if r < *w {
            return *i;
        }
        r -= w;
    }
    pool[0].0
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn interact(
    (keys, time, level, defs, pu): (Res<ButtonInput<KeyCode>>, Res<Time>, Res<LevelRes>, Res<Defs>, Res<ActivePowerups>),
    (mut world, mut boards, mut loadout, mut score, mut prompt, mut mcrate, mut gun): (
        ResMut<World>,
        ResMut<Boards>,
        ResMut<Loadout>,
        ResMut<Score>,
        ResMut<Prompt>,
        ResMut<MysteryCrate>,
        ResMut<Gun>,
    ),
    player: Query<&Transform, With<Player>>,
    debris: Query<(Entity, &Debris)>,
    (mut sfx, mut points): (EventWriter<PlaySfx>, EventWriter<PointsEvent>),
    mut repair_timer: Local<f32>,
    mut commands: Commands,
    mats: Res<Mats>,
) {
    prompt.0.clear();
    let Ok(pt) = player.single() else { return };
    let me = Vec2::new(pt.translation.x, pt.translation.z);
    let eye = pt.translation.y;
    // Only things on the player's own floor: the real game's use triggers
    // are volumes, so a buy one storey up or down is out of reach.
    let level_with = |y: f32, tolerance: f32| (eye - y).abs() < tolerance;
    let level_ref = &level.0;

    // Find the nearest interactable.
    let mut best: Option<(f32, Target)> = None;
    let consider = |d: f32, t: Target, best: &mut Option<(f32, Target)>| {
        if best.as_ref().is_none_or(|b| d < b.0) {
            *best = Some((d, t));
        }
    };
    for (i, w) in level_ref.windows.iter().enumerate() {
        let (x, z) = w.inside_point();
        let d = me.distance(Vec2::new(x, z));
        if d < 1.6 && boards.0[i] < w.boards && level_with(w.center.y, 2.0) {
            consider(d, Target::Window(i), &mut best);
        }
    }
    for (i, wb) in level_ref.wall_buys.iter().enumerate() {
        let p = Vec2::new(wb.pos.x + wb.facing.0 * 0.6, wb.pos.z + wb.facing.1 * 0.6);
        let d = me.distance(p);
        if d < 1.4 && level_with(wb.pos.y, 1.6) {
            consider(d, Target::WallBuy(i), &mut best);
        }
    }
    for (i, door) in level_ref.doors.iter().enumerate() {
        if world.door_open[i] {
            continue;
        }
        let c = door.blocker.center();
        let d = me.distance(Vec2::new(c.x, c.z));
        let half_height = (door.blocker.max.y - door.blocker.min.y) * 0.5;
        if d < 2.3 && level_with(c.y, half_height + 1.4) {
            consider(d, Target::Door(i), &mut best);
        }
    }
    {
        let c = level_ref.crate_box.center();
        let d = me.distance(Vec2::new(c.x, c.z));
        if d < 1.9 && level_with(c.y, 2.0) {
            consider(d, Target::Crate, &mut best);
        }
    }

    let press = keys.just_pressed(KeyCode::KeyF);
    let hold = keys.pressed(KeyCode::KeyF);
    let Some((_, target)) = best else {
        *repair_timer = 0.0;
        return;
    };

    match target {
        Target::Window(i) => {
            prompt.0 = "Hold F to rebuild barrier".into();
            if hold {
                *repair_timer += time.delta_secs();
                if *repair_timer >= 0.7 {
                    *repair_timer = 0.0;
                    let n = boards.0[i];
                    if n < level_ref.windows[i].boards {
                        boards.0[i] = n + 1;
                        spawn_board(&mut commands, level_ref, &mats, i, n);
                        earn(&mut score, &mut points, &pu, rules::POINTS_BOARD);
                        sfx.write(PlaySfx::new(Sfx::BoardRepair));
                    }
                }
            } else {
                *repair_timer = 0.0;
            }
        }
        Target::WallBuy(i) => {
            let wb = &level_ref.wall_buys[i];
            let Some(def) = weapons::find(&defs.0, &wb.weapon_id) else { return };
            let name = &defs.0[def].name;
            let owned = loadout.has(def).is_some();
            let cost = if owned { wb.cost / 2 } else { wb.cost };
            prompt.0 = if owned {
                format!("Press F to buy ammo for {name} [Cost: {cost}]")
            } else {
                format!("Press F to buy {name} [Cost: {cost}]")
            };
            if press {
                if try_spend(&mut score, &mut points, cost) {
                    loadout.give(&defs.0, def);
                    gun.reload = None;
                    sfx.write(PlaySfx::new(if owned { Sfx::Purchase } else { Sfx::WallBuy }));
                } else {
                    sfx.write(PlaySfx::new(Sfx::Deny));
                }
            }
        }
        Target::Door(i) => {
            let door = &level_ref.doors[i];
            prompt.0 = format!("Press F to clear debris to the {} [Cost: {}]", door.name, door.cost);
            if press {
                if try_spend(&mut score, &mut points, door.cost) {
                    world.door_open[i] = true;
                    world.rebuild(level_ref);
                    for (e, d) in &debris {
                        if d.door == i {
                            commands.entity(e).try_despawn();
                        }
                    }
                    sfx.write(PlaySfx::new(Sfx::DoorOpen));
                    sfx.write(PlaySfx::new(Sfx::Purchase));
                } else {
                    sfx.write(PlaySfx::new(Sfx::Deny));
                }
            }
        }
        Target::Crate => match mcrate.0 {
            CrateState::Idle => {
                prompt.0 = format!("Press F for a random weapon [Cost: {}]", rules::CRATE_COST);
                if press {
                    if try_spend(&mut score, &mut points, rules::CRATE_COST) {
                        let result = roll_crate(&defs.0, &loadout);
                        mcrate.0 = CrateState::Rolling { t: 0.0, result, shown: result, tick: 0.0 };
                        sfx.write(PlaySfx::new(Sfx::CrateOpen));
                    } else {
                        sfx.write(PlaySfx::new(Sfx::Deny));
                    }
                }
            }
            CrateState::Rolling { .. } => {}
            CrateState::Ready { def, .. } => {
                prompt.0 = format!("Press F to take {}", defs.0[def].name);
                if press {
                    loadout.give(&defs.0, def);
                    gun.reload = None;
                    mcrate.0 = CrateState::Idle;
                    sfx.write(PlaySfx::new(Sfx::Purchase));
                }
            }
        },
    }
}

fn crate_tick(time: Res<Time>, defs: Res<Defs>, mut mcrate: ResMut<MysteryCrate>, mut sfx: EventWriter<PlaySfx>) {
    let dt = time.delta_secs();
    let pool = weapons::crate_pool(&defs.0);
    mcrate.0 = match mcrate.0 {
        CrateState::Rolling { t, result, shown, tick } => {
            let t = t + dt;
            let mut tick = tick - dt;
            let mut shown = shown;
            if tick <= 0.0 {
                // Slow the cycling down as the roll ends.
                tick = 0.05 + t * t * 0.012;
                shown = pool[fastrand::usize(..pool.len())].0;
            }
            if t >= 4.2 {
                sfx.write(PlaySfx::new(Sfx::CrateReady));
                CrateState::Ready { def: result, t: 0.0 }
            } else {
                CrateState::Rolling { t, result, shown, tick }
            }
        }
        CrateState::Ready { def, t } => {
            if t + dt > 12.0 {
                CrateState::Idle
            } else {
                CrateState::Ready { def, t: t + dt }
            }
        }
        s => s,
    };
}

#[allow(clippy::type_complexity)]
fn crate_visuals(
    time: Res<Time>,
    mcrate: Res<MysteryCrate>,
    defs: Res<Defs>,
    level: Res<LevelRes>,
    mut lid: Query<&mut Transform, (With<CrateLid>, Without<CrateDisplay>)>,
    mut disp: Query<(&mut Transform, &mut Visibility), (With<CrateDisplay>, Without<CrateLid>)>,
) {
    let s = v3(level.0.crate_box.size());
    let (open, shown, rise) = match mcrate.0 {
        CrateState::Idle => (0.0, None, 0.0),
        CrateState::Rolling { t, shown, .. } => ((t * 3.0).min(1.0), Some(shown), (t / 4.2).min(1.0)),
        CrateState::Ready { def, t } => (1.0, Some(def), 1.0 - (t / 12.0) * 0.6),
    };
    if let Ok(mut l) = lid.single_mut() {
        l.translation = Vec3::new(0.0, s.y * 0.925 + open * 0.15, -open * s.z * 0.4);
        l.rotation = Quat::from_rotation_x(-open * 1.2);
    }
    if let Ok((mut t, mut v)) = disp.single_mut() {
        match shown {
            Some(def) => {
                *v = Visibility::Visible;
                let len = match defs.0[def].kind {
                    weapons::Kind::Pistol | weapons::Kind::Wonder => 0.4,
                    weapons::Kind::Smg => 0.8,
                    _ => 1.1,
                };
                t.translation = Vec3::new(0.0, 0.8 + rise * 0.6, 0.0);
                t.scale = Vec3::new(len / 0.9, 1.0, 1.0);
                t.rotation = Quat::from_rotation_y(time.elapsed_secs() * 1.5);
            }
            None => *v = Visibility::Hidden,
        }
    }
}
