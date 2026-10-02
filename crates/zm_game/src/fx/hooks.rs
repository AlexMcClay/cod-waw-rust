//! Effects the zombie scripts play, hooked onto the game's state without
//! touching the gameplay systems: zombie eye glow, power-up glow and grab,
//! window boards breaking and being rebuilt, and the mystery box's light.

use super::{axes_from_forward, find_named, to_bevy, to_game, FxEvent, FxInstance, FxLibrary, Frame};
use crate::interact::{CrateState, MysteryCrate};
use crate::powerups::{Drop, PowerupGrabbed};
use crate::zombies::{Zombie, ZombieRig};
use crate::{Boards, Dynamic, GameState, LevelRes};
use bevy::prelude::*;
use std::collections::HashMap;
use zm_core::rules::Powerup;

/// The eye glow effect playing on a zombie.
#[derive(Component)]
pub struct EyeGlow(Entity);

/// `zombie_eye_glow`: `level._effect["eye_glow"]` on the left eyeball,
/// stopped when the zombie dies.
#[allow(clippy::type_complexity)]
pub fn eye_glow(
    mut commands: Commands,
    lib: Res<FxLibrary>,
    new: Query<Entity, Added<ZombieRig>>,
    glowing: Query<(Entity, &Zombie, &EyeGlow)>,
    mut instances: Query<&mut FxInstance>,
    children: Query<&Children>,
    names: Query<&Name>,
    globals: Query<&GlobalTransform>,
    mut pending: Local<Vec<(Entity, u8)>>,
) {
    pending.extend(new.iter().map(|e| (e, 0)));
    let Some(def) = lib.effect("eye_glow").cloned() else {
        pending.clear();
        return;
    };
    // Joints appear a frame after the rig; retry a few frames.
    pending.retain_mut(|(z, tries)| {
        *tries += 1;
        let Some(eye) = find_named(*z, "j_eyeball_le", &children, &names) else { return *tries < 5 };
        let Ok(g) = globals.get(eye) else { return *tries < 5 };
        let fx = commands.spawn((FxInstance::new(def.clone(), Frame::from_global(g), Some(eye), 0.0), Dynamic)).id();
        commands.entity(*z).insert(EyeGlow(fx));
        false
    });
    for (e, z, glow) in &glowing {
        if !z.alive() {
            if let Ok(mut i) = instances.get_mut(glow.0) {
                i.stop();
            }
            commands.entity(e).remove::<EyeGlow>();
        }
    }
}

fn script_name(p: Powerup) -> &'static str {
    match p {
        Powerup::MaxAmmo => "full_ammo",
        Powerup::InstaKill => "insta_kill",
        Powerup::DoublePoints => "double_points",
        Powerup::Nuke => "nuke",
        Powerup::Carpenter => "carpenter",
    }
}

/// `powerup_wobble` plays `powerup_on` on a drop; grabbing one plays
/// `powerup_grabbed` and `powerup_grabbed_wave`, and the nuke its own
/// effect (`add_zombie_powerup`'s fx).
pub fn powerups(
    mut commands: Commands,
    lib: Res<FxLibrary>,
    new: Query<Entity, Added<Drop>>,
    drops: Query<(Entity, &GlobalTransform, &Drop)>,
    mut grabbed: EventReader<PowerupGrabbed>,
    player: Query<&GlobalTransform, With<crate::player::Player>>,
    globals: Query<&GlobalTransform>,
    mut seen: Local<HashMap<Entity, (Powerup, Vec3)>>,
    mut fx: EventWriter<FxEvent>,
) {
    if let Some(def) = lib.effect("powerup_on") {
        for e in &new {
            let frame = globals.get(e).map(Frame::from_global).unwrap_or(Frame { origin: Vec3::ZERO, axes: Mat3::IDENTITY });
            commands.spawn((FxInstance::new(def.clone(), frame, Some(e), 0.0), Dynamic));
        }
    }
    for (e, g, d) in &drops {
        seen.insert(e, (d.kind, g.translation()));
    }
    let me = player.single().map(|p| p.translation()).unwrap_or_default();
    for PowerupGrabbed(kind) in grabbed.read() {
        let Some((&e, &(_, pos))) = seen.iter().filter(|(_, (k, _))| k == kind).min_by(|a, b| a.1 .1.distance(me).total_cmp(&b.1 .1.distance(me))) else { continue };
        seen.remove(&e);
        for key in ["powerup_grabbed", "powerup_grabbed_wave"] {
            fx.write(FxEvent::play(key, pos));
        }
        if *kind == Powerup::Nuke {
            if let Some(n) = lib.data.powerup_effects.get(script_name(*kind)) {
                fx.write(FxEvent::play(n.clone(), pos));
            }
        }
    }
    seen.retain(|e, _| drops.contains(*e));
}

/// Boards torn off or nailed back: `wood_chunk_destory` three times around
/// the board (`remove_chunk` / `replace_chunk`).
pub fn boards(
    boards: Res<Boards>,
    level: Res<LevelRes>,
    state: Res<State<GameState>>,
    mut last: Local<Vec<u8>>,
    mut fx: EventWriter<FxEvent>,
) {
    if *state.get() != GameState::Playing {
        last.clear();
        return;
    }
    if last.len() != boards.0.len() {
        *last = boards.0.clone();
        return;
    }
    for (w, (&now, was)) in boards.0.iter().zip(last.iter_mut()).enumerate() {
        if now == *was {
            continue;
        }
        let index = now.min(*was) as usize;
        let changed = now.abs_diff(*was) == 1;
        *was = now;
        if !changed {
            continue;
        }
        let Some(win) = level.0.windows.get(w) else { continue };
        let at = win.board_models.get(index).map(|b| crate::v3(b.1)).unwrap_or_else(|| crate::v3(win.center) + Vec3::Y * 1.2);
        let o = to_game(at);
        for k in 0..3 {
            let r = |n: f32| (fastrand::f32() * n).floor();
            let off = match k {
                0 => Vec3::ZERO,
                1 => Vec3::new(r(20.0), r(20.0), r(10.0)),
                _ => Vec3::new(r(40.0), r(40.0), r(20.0)),
            };
            fx.write(FxEvent::play("wood_chunk_destory", to_bevy(o + off)));
        }
    }
}

/// `treasure_chest_glowfx`: `chest_light` at the weapon spawn point, facing
/// down (its rays rise), while the box is open.
pub fn chest_light(
    mut commands: Commands,
    lib: Res<FxLibrary>,
    chest: Res<MysteryCrate>,
    nacht: Option<Res<crate::nacht::NachtAssets>>,
    mut instances: Query<&mut FxInstance>,
    mut current: Local<Option<Entity>>,
) {
    let open = !matches!(chest.0, CrateState::Idle);
    match (*current, open) {
        (None, true) => {
            let (Some(def), Some(c)) = (lib.effect("chest_light"), nacht.as_ref().and_then(|n| n.chest.as_ref())) else { return };
            let frame = Frame { origin: to_game(c.weapon.translation), axes: axes_from_forward(-Vec3::Z, Some(Vec3::X)) };
            *current = Some(commands.spawn((FxInstance::new(def.clone(), frame, None, 0.0), Dynamic)).id());
        }
        (Some(e), false) => {
            if let Ok(mut i) = instances.get_mut(e) {
                i.stop();
            }
            *current = None;
        }
        (Some(e), true) if instances.get(e).is_err() => *current = None,
        _ => {}
    }
}

/// The stand-in glowing sphere at the muzzle gives way to the weapon's own
/// flash effect once the game's effects are loaded (its light stays).
pub fn muzzle_sphere(mut commands: Commands, q: Query<Entity, (With<crate::weapons::MuzzleFlash>, With<Mesh3d>)>) {
    if !super::live() {
        return;
    }
    for e in &q {
        commands.entity(e).remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>)>();
    }
}
