//! Player weapons: loadout, hitscan firing, reloading, knifing, and the
//! procedural first-person viewmodel.
//!
//! Every number comes from the weapon's definition ([`WeaponDef`], read from
//! the game's weapon files): damage falloff and per-hit-location
//! multipliers, bullet penetration through bodies, fire and rechamber
//! times, reloads that add the magazine part-way through (round by round
//! for the shotgun and the scoped Kar98k), the aim-spread model, and the
//! raise/drop times.

use crate::audio::{PlayAlias, PlaySfx, Sfx, ZoneSounds};
use crate::player::{Player, PlayerCtl, Stance};
use crate::world::Mats;
use crate::zombies::{self, Zombie};
use crate::{cursor_locked, earn, ActivePowerups, Defs, Dynamic, GameState, PointsEvent, Round, Score, World, ZombieKilled};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use zm_core::geom::V3;
use zm_core::rules::{self, KillKind};
use zm_core::weapons::{FireMode, HitLoc, Kind, SpreadStance, WeaponDef, START_PISTOL, UNITS_TO_M};

pub const MAX_SLOTS: usize = 2;
const KNIFE_RANGE: f32 = 1.9;
/// How far a bullet is traced (metres).
const BULLET_RANGE: f32 = 120.0;
/// The game's run speed (`g_speed` 190), for the moving part of the spread.
const RUN_SPEED: f32 = 190.0 * UNITS_TO_M;
/// The game's default `cg_fov`, which `adsZoomFov` is relative to.
pub const GAME_FOV: f32 = 65.0;

#[derive(Debug, Clone)]
pub struct Slot {
    pub def: usize,
    pub clip: u32,
    pub reserve: u32,
}

#[derive(Resource)]
pub struct Loadout {
    pub slots: Vec<Slot>,
    pub cur: usize,
}

impl Loadout {
    /// The spawn loadout: the starting pistol with its `startAmmo` (8 + 32
    /// for the zombie Colt).
    pub fn starting(defs: &[WeaponDef]) -> Self {
        let (clip, reserve) = defs[START_PISTOL].start_ammo_split();
        Loadout { slots: vec![Slot { def: START_PISTOL, clip, reserve }], cur: 0 }
    }

    pub fn current(&self) -> &Slot {
        &self.slots[self.cur]
    }

    pub fn has(&self, def: usize) -> Option<usize> {
        self.slots.iter().position(|s| s.def == def)
    }

    /// Give a weapon: refills if already owned, fills an empty slot, or
    /// replaces the one in hand. Like the scripts' `GiveWeapon` +
    /// `GiveMaxAmmo`: a full clip and a full reserve (`maxAmmo`).
    pub fn give(&mut self, defs: &[WeaponDef], def: usize) {
        let (clip, reserve) = defs[def].full_ammo();
        let full = Slot { def, clip, reserve };
        if let Some(i) = self.has(def) {
            self.slots[i] = full;
            self.cur = i;
        } else if self.slots.len() < MAX_SLOTS {
            self.slots.push(full);
            self.cur = self.slots.len() - 1;
        } else {
            self.slots[self.cur] = full;
        }
    }

    /// Max Ammo: every weapon's reserve to `maxAmmo` (`GiveMaxAmmo`; the
    /// clip is left as it is).
    pub fn refill_all(&mut self, defs: &[WeaponDef]) {
        for s in &mut self.slots {
            s.reserve = defs[s.def].max_ammo;
        }
    }
}

/// Which part of a reload is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadPhase {
    /// A whole-magazine reload (normal or empty).
    Full,
    /// Segmented reload: getting ready (may load rounds itself).
    Start,
    /// Segmented reload: one round (or `reloadAmmoAdd` rounds) in.
    Loop,
    /// Segmented reload: closing up (the pump/bolt).
    End,
}

/// A reload in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReloadState {
    pub phase: ReloadPhase,
    /// Seconds into this phase, and its length.
    pub t: f32,
    pub dur: f32,
    /// When in this phase the rounds go in (`None` once they have).
    pub add_at: Option<f32>,
    /// Fire was pressed during a segmented reload: stop after this phase.
    pub interrupt: bool,
}

impl ReloadState {
    fn new(phase: ReloadPhase, dur: f32, add_at: Option<f32>) -> Self {
        ReloadState { phase, t: 0.0, dur: dur.max(0.0), add_at, interrupt: false }
    }

    /// The first phase of a reload of `def` (`empty`: the clip is empty).
    pub fn begin(def: &WeaponDef, empty: bool) -> Self {
        let r = &def.reload;
        if !r.segmented {
            return Self::new(ReloadPhase::Full, r.duration(empty), Some(r.ammo_in_at(empty)));
        }
        if r.start_time > 0.0 {
            let at = (r.start_add > 0).then(|| if r.start_add_time > 0.0 { r.start_add_time.min(r.start_time) } else { r.start_time });
            Self::new(ReloadPhase::Start, r.start_time, at)
        } else {
            Self::looping(def)
        }
    }

    fn looping(def: &WeaponDef) -> Self {
        let r = &def.reload;
        let at = if r.add_time > 0.0 { r.add_time.min(r.time) } else { r.time };
        Self::new(ReloadPhase::Loop, r.time, Some(at))
    }

    fn end(def: &WeaponDef) -> Self {
        Self::new(ReloadPhase::End, def.reload.end_time, None)
    }

    /// 0..1 through the current phase.
    pub fn progress(&self) -> f32 {
        if self.dur > 0.0 {
            (self.t / self.dur).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}

#[derive(Resource, Default)]
pub struct Gun {
    /// Seconds until the next shot may be fired.
    pub cooldown: f32,
    pub reload: Option<ReloadState>,
    /// Seconds of weapon switch (drop + raise) left, and its total.
    pub switch: f32,
    pub switch_total: f32,
    pub knife_cd: f32,
    pub knife_anim: f32,
    pub kick: f32,
    pub flash: f32,
    pub hitmarker: f32,
    pub headmarker: bool,
    /// Seconds since the last shot (drives the fire/rechamber animation).
    pub since_shot: Option<f32>,
    /// The running reload started with an empty magazine.
    pub reload_empty: bool,
    /// The engine's aim-spread scale (0..1) and the resulting cone
    /// half-angle in degrees.
    pub spread_scale: f32,
    pub spread: f32,
    /// Rounds left in the current burst (burst-fire weapons).
    pub burst_left: u32,
}

/// The handling numbers of the weapon in hand, for the player controller
/// (movement speed, sprint and aiming): `moveSpeedScale` and friends.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct HeldWeapon {
    /// Multiplies the run speed (hip) / while aimed.
    pub move_speed_scale: f32,
    pub ads_move_speed_scale: f32,
    /// Multiplies how long a sprint lasts.
    pub sprint_duration_scale: f32,
    /// Seconds to aim down the sights / to lower them.
    pub ads_in_time: f32,
    pub ads_out_time: f32,
    /// Field of view while aimed, relative to the game's 65 degrees.
    pub ads_zoom_fov: f32,
}

impl Default for HeldWeapon {
    fn default() -> Self {
        HeldWeapon { move_speed_scale: 1.0, ads_move_speed_scale: 1.0, sprint_duration_scale: 1.0, ads_in_time: 0.25, ads_out_time: 0.25, ads_zoom_fov: GAME_FOV }
    }
}

impl HeldWeapon {
    pub fn of(def: &WeaponDef) -> Self {
        HeldWeapon {
            move_speed_scale: def.move_speed_scale,
            ads_move_speed_scale: def.ads_move_speed_scale,
            sprint_duration_scale: def.sprint_duration_scale,
            ads_in_time: def.ads_in_time,
            ads_out_time: def.ads_out_time,
            ads_zoom_fov: def.ads_zoom_fov,
        }
    }

    /// The aimed field of view for a hip field of view `base` (degrees):
    /// the game's zoom as a fraction of its own 65.
    pub fn ads_fov(&self, base: f32) -> f32 {
        base * self.ads_zoom_fov / GAME_FOV
    }
}

/// Whether the weapon's scope overlay covers the view (fully aimed with a
/// weapon that has one).
pub fn scope_shown(def: &WeaponDef, ctl: &PlayerCtl) -> bool {
    def.ads_overlay.is_some() && ctl.ads >= 0.999
}

/// The first-person rig (arms + gun) spawned for the weapon in hand.
#[derive(Component)]
pub struct ViewRigState {
    joints: Vec<crate::nacht::Joint>,
    weapon: &'static str,
}

/// Short-lived visual effects (sparks, blood).
#[derive(Component)]
pub struct Fx {
    pub ttl: f32,
    pub max: f32,
    pub vel: Vec3,
    pub size: f32,
}

#[derive(Resource, Default)]
pub struct Tracers(pub Vec<(Vec3, Vec3, f32, Color)>);

#[derive(Component)]
pub struct ViewModel {
    pub shown: Option<usize>,
    /// The real arms rig is in use (animations position the gun).
    pub rig: bool,
}

#[derive(Component)]
pub struct GunModel;

#[derive(Component)]
pub struct KnifeModel;

#[derive(Component)]
pub struct MuzzleFlash;

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gun>()
            .init_resource::<Tracers>()
            .init_resource::<HeldWeapon>()
            .add_systems(Startup, spawn_viewmodel.after(crate::player::spawn_player))
            .add_systems(
                Update,
                (switch_weapons, reload, aim_spread, fire, knife).chain().run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, ((update_viewmodel, animate_view_rig).chain(), update_fx, draw_tracers));
    }
}

fn switch_weapons(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut loadout: ResMut<Loadout>,
    mut gun: ResMut<Gun>,
    (defs, zs, mut alias): (Res<Defs>, Res<ZoneSounds>, EventWriter<PlayAlias>),
    mut held: Local<Option<usize>>,
) {
    // A weapon that arrived without a switch (bought, from the box) comes
    // up with its first-raise animation.
    let now = loadout.current().def;
    if held.is_some_and(|h| h != now) && gun.switch <= 0.0 {
        gun.switch = defs.0[now].first_raise_time;
        gun.switch_total = gun.switch;
        gun.cooldown = 0.0;
        gun.burst_left = 0;
    }
    *held = Some(now);
    let n = loadout.slots.len();
    if n < 2 {
        return;
    }
    let mut target = None;
    if keys.just_pressed(KeyCode::Digit1) {
        target = Some(0);
    } else if keys.just_pressed(KeyCode::Digit2) {
        target = Some(1);
    } else if keys.just_pressed(KeyCode::KeyQ) || scroll.delta.y.abs() > 0.1 {
        target = Some((loadout.cur + 1) % n);
    }
    if let Some(t) = target.filter(|t| *t != loadout.cur && *t < n) {
        // Put the old weapon away, then raise the new one.
        let old = &defs.0[loadout.slots[loadout.cur].def];
        let new = &defs.0[loadout.slots[t].def];
        loadout.cur = t;
        *held = Some(loadout.slots[t].def);
        gun.switch = old.drop_time + new.raise_time;
        gun.switch_total = gun.switch;
        gun.reload = None;
        gun.cooldown = 0.0;
        gun.burst_left = 0;
        if let Some(a) = weapon_sound(&zs, new.id, "raiseSoundPlayer", Sfx::Reload) {
            alias.write(a.volume(0.6));
        }
    }
}

/// Moves up to `n` rounds from the reserve into the clip.
fn load_rounds(slot: &mut Slot, clip_size: u32, n: u32) {
    let take = n.min(clip_size.saturating_sub(slot.clip)).min(slot.reserve);
    slot.clip += take;
    slot.reserve -= take;
}

fn reload(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    defs: Res<Defs>,
    mut loadout: ResMut<Loadout>,
    mut gun: ResMut<Gun>,
    zs: Res<ZoneSounds>,
    mut alias: EventWriter<PlayAlias>,
) {
    let dt = time.delta_secs();
    gun.switch = (gun.switch - dt).max(0.0);
    let cur = loadout.cur;
    let def = &defs.0[loadout.slots[cur].def];
    if let Some(r) = gun.reload {
        let (next, magazine_in) = step_reload(r, def, &mut loadout.slots[cur], dt);
        gun.reload = next;
        // With game data the reload anim's notetracks make the sounds.
        if magazine_in && !zs.weapons.contains_key(def.id) {
            alias.write(PlayAlias::local("").or(Sfx::Reload).volume(0.7));
        }
        return;
    }
    let slot = &loadout.slots[cur];
    let wants = keys.just_pressed(KeyCode::KeyR) || slot.clip == 0;
    if wants && def.can_reload(slot.clip, slot.reserve) && gun.switch <= 0.0 && gun.knife_anim <= 0.0 && gun.cooldown <= 0.0 {
        gun.reload_empty = slot.clip == 0;
        gun.reload = Some(ReloadState::begin(def, gun.reload_empty));
        gun.burst_left = 0;
        let field = if def.reload.segmented {
            "reloadStartSoundPlayer"
        } else if gun.reload_empty {
            "reloadEmptySoundPlayer"
        } else {
            "reloadSoundPlayer"
        };
        if let Some(a) = weapon_sound(&zs, def.id, field, Sfx::Reload) {
            alias.write(a.volume(0.5));
        }
    }
}

/// Advances a reload by `dt`, loading rounds into `slot` as they go in.
/// Returns the next state (`None` when done) and whether a whole magazine
/// just went in.
pub fn step_reload(mut r: ReloadState, def: &WeaponDef, slot: &mut Slot, dt: f32) -> (Option<ReloadState>, bool) {
    r.t += dt;
    let mut magazine_in = false;
    if r.add_at.is_some_and(|at| r.t >= at) {
        r.add_at = None;
        let n = match r.phase {
            ReloadPhase::Full => def.clip,
            ReloadPhase::Start => def.reload.start_add,
            ReloadPhase::Loop => def.reload.ammo_add,
            ReloadPhase::End => 0,
        };
        load_rounds(slot, def.clip, n);
        magazine_in = r.phase == ReloadPhase::Full;
    }
    if r.t < r.dur {
        return (Some(r), magazine_in);
    }
    let more = slot.clip < def.clip && slot.reserve > 0 && !r.interrupt;
    let next = match r.phase {
        ReloadPhase::Full | ReloadPhase::End => None,
        ReloadPhase::Start | ReloadPhase::Loop if more => Some(ReloadState::looping(def)),
        ReloadPhase::Start | ReloadPhase::Loop => Some(ReloadState::end(def)),
    };
    (next, magazine_in)
}

/// A sound a weapon definition names (`field`), or the procedural stand-in
/// when the weapon has no game data (prototype map without an install).
pub fn weapon_sound(zs: &ZoneSounds, weapon: &str, field: &str, fallback: Sfx) -> Option<PlayAlias> {
    if zs.weapons.contains_key(weapon) {
        zs.weapon_field(weapon, field).map(PlayAlias::local)
    } else {
        Some(PlayAlias::local("").or(fallback))
    }
}

/// Random direction inside a cone around `dir` the way the engine picks it:
/// a random angle and a uniformly random radius (so shots bunch towards
/// the centre).
fn spread_dir(dir: Vec3, right: Vec3, up: Vec3, half_angle_deg: f32) -> Vec3 {
    if half_angle_deg <= 0.0 {
        return dir;
    }
    let r = half_angle_deg.to_radians().tan() * fastrand::f32();
    let a = fastrand::f32() * std::f32::consts::TAU;
    (dir + right * (a.cos() * r) + up * (a.sin() * r)).normalize()
}

fn to_v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

/// Distance to the first solid along the ray.
pub fn wall_distance(world: &World, origin: Vec3, dir: Vec3, max: f32) -> f32 {
    let (o, d) = (to_v3(origin), to_v3(dir));
    let solids = world.bullet_solids.iter().filter_map(|s| s.ray_hit(o, d, max)).fold(max, f32::min);
    match &world.mesh {
        Some(m) => m.raycast(o, d, solids).map_or(solids, |h| h.t),
        None => solids,
    }
}

fn spread_stance(s: Stance) -> SpreadStance {
    match s {
        Stance::Stand => SpreadStance::Stand,
        Stance::Crouch => SpreadStance::Ducked,
        Stance::Prone => SpreadStance::Prone,
    }
}

/// Inches of flesh a bullet passes through at a hit location (used up from
/// the weapon's `penetrateType` depth to go on to the next body).
fn flesh_thickness(loc: HitLoc) -> f32 {
    match loc {
        HitLoc::TorsoUpper | HitLoc::TorsoLower => 12.0,
        HitLoc::Head | HitLoc::Helmet => 8.0,
        HitLoc::RightLegUpper | HitLoc::LeftLegUpper => 7.0,
        _ => 4.0,
    }
}

/// The engine's aim-spread scale (grows while moving, decays at rest) and
/// the handling numbers of the weapon in hand.
fn aim_spread(
    time: Res<Time>,
    defs: Res<Defs>,
    loadout: Res<Loadout>,
    mut gun: ResMut<Gun>,
    mut held: ResMut<HeldWeapon>,
    player: Query<&PlayerCtl, With<Player>>,
    mut last_yaw: Local<Option<f32>>,
) {
    let dt = time.delta_secs();
    let def = &defs.0[loadout.current().def];
    let h = HeldWeapon::of(def);
    if *held != h {
        *held = h;
    }
    let Ok(ctl) = player.single() else { return };
    let stance = spread_stance(ctl.stance);
    let moving = if ctl.on_ground { (ctl.vel.length() / RUN_SPEED).clamp(0.0, 1.0) } else { 1.0 };
    // Turning: a fraction of a quick (180 deg/s) turn.
    let turning = match *last_yaw {
        Some(y) if dt > 0.0 => ((ctl.yaw - y).abs() / dt / std::f32::consts::PI).clamp(0.0, 1.0),
        _ => 0.0,
    };
    *last_yaw = Some(ctl.yaw);
    gun.spread_scale = def.spread.update_scale(gun.spread_scale, stance, moving, turning, dt);
    gun.spread = def.spread.cone(stance, gun.spread_scale, ctl.ads);
}

#[allow(clippy::type_complexity)]
fn fire(
    (mouse, time, windows): (Res<ButtonInput<MouseButton>>, Res<Time>, Query<&Window, With<PrimaryWindow>>),
    (defs, world, pu, round): (Res<Defs>, Res<World>, Res<ActivePowerups>, Res<Round>),
    (mut loadout, mut gun, mut score, mut tracers): (ResMut<Loadout>, ResMut<Gun>, ResMut<Score>, ResMut<Tracers>),
    mut player: Query<(&Transform, &mut PlayerCtl), With<Player>>,
    mut zq: Query<(Entity, &Transform, &mut Zombie), Without<Player>>,
    (mut sfx, mut points, mut killed): (EventWriter<PlaySfx>, EventWriter<PointsEvent>, EventWriter<ZombieKilled>),
    (zs, mut alias, mut fx): (Res<ZoneSounds>, EventWriter<PlayAlias>, EventWriter<crate::fx::FxEvent>),
    mut commands: Commands,
    mats: Res<Mats>,
) {
    let dt = time.delta_secs();
    gun.cooldown = (gun.cooldown - dt).max(0.0);
    if let Some(t) = gun.since_shot.as_mut() {
        *t += dt;
    }
    gun.kick = (gun.kick - dt * 6.0).max(0.0);
    gun.flash = (gun.flash - dt).max(0.0);
    gun.hitmarker = (gun.hitmarker - dt).max(0.0);

    if !cursor_locked(&windows) || gun.switch > 0.0 || gun.knife_anim > 0.0 {
        gun.burst_left = 0;
        return;
    }
    let cur = loadout.cur;
    let def = defs.0[loadout.slots[cur].def].clone();
    if let Some(r) = gun.reload.as_mut() {
        // A shot cuts a round-by-round reload short (after the closing
        // pump/bolt); a magazine reload has to finish.
        if mouse.just_pressed(MouseButton::Left) && loadout.slots[cur].clip > 0 && matches!(r.phase, ReloadPhase::Start | ReloadPhase::Loop) {
            gun.reload = Some(ReloadState { interrupt: true, ..ReloadState::end(&def) });
        }
        return;
    }
    let trigger = match def.mode {
        FireMode::Auto => mouse.pressed(MouseButton::Left),
        FireMode::Semi => mouse.just_pressed(MouseButton::Left),
        FireMode::Burst(n) => {
            if mouse.just_pressed(MouseButton::Left) && gun.burst_left == 0 {
                gun.burst_left = n;
            }
            gun.burst_left > 0
        }
    };
    if !trigger || gun.cooldown > 0.0 {
        return;
    }
    if loadout.slots[cur].clip == 0 {
        gun.burst_left = 0;
        if mouse.just_pressed(MouseButton::Left) {
            if let Some(a) = weapon_sound(&zs, def.id, "emptyFireSoundPlayer", Sfx::DryFire) {
                alias.write(a.volume(0.6));
            }
        }
        return;
    }
    let Ok((cam, mut ctl)) = player.single_mut() else { return };
    loadout.slots[cur].clip -= 1;
    let left = loadout.slots[cur].clip;
    gun.burst_left = gun.burst_left.saturating_sub(1);
    gun.cooldown = def.shot_cycle(left).max(0.04);
    gun.since_shot = Some(0.0);
    gun.kick = 1.0;
    gun.flash = 0.05;
    fx.write(crate::fx::FxEvent::MuzzleFlash { weapon: def.id });
    ctl.recoil += def.kick * 0.012 * (1.0 - 0.5 * ctl.ads);
    // The last round may have its own sound (the Garand's ping).
    match zs.weapon_field(def.id, "fireLastSoundPlayer").filter(|_| left == 0) {
        Some(last) => {
            alias.write(PlayAlias::local(last));
        }
        None => {
            sfx.write(PlaySfx::weapon(Sfx::for_weapon(def.kind), def.id));
        }
    }

    let origin = cam.translation;
    let fwd = cam.forward().as_vec3();
    let right = cam.right().as_vec3();
    let up = cam.up().as_vec3();
    let muzzle = origin + right * 0.2 - up * 0.14 + fwd * 0.7;
    // The cone for this shot, then the shot itself widens the next one.
    let cone = gun.spread;
    gun.spread_scale = (gun.spread_scale + def.spread.fire_add).min(1.0);
    let insta = pu.insta_kill > 0.0;
    let round_no = round.0.round.max(1);
    // Pistol-class weapons never gib heads (`head_should_gib`).
    let gibs = !matches!(def.kind, Kind::Pistol | Kind::Wonder);
    let mut any_hit = false;
    let mut any_head = false;

    for _ in 0..def.pellets.max(1) {
        let dir = spread_dir(fwd, right, up, cone);
        let max = def.bullet_range(BULLET_RANGE);
        let wall_t = wall_distance(&world, origin, dir, max);
        // Every body along the ray, nearest first: the bullet goes through
        // as many as its penetration depth allows, losing damage as it goes.
        let mut hits: Vec<(f32, HitLoc, Entity)> = zq
            .iter()
            .filter(|(_, _, z)| z.alive())
            .filter_map(|(e, t, z)| zombies::hit_location(origin, dir, t, z, wall_t).map(|(d, loc)| (d, loc, e)))
            .collect();
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut remaining = 1.0;
        let mut end_t = wall_t;
        for (d, loc, e) in hits {
            let Ok((_, _, mut z)) = zq.get_mut(e) else { continue };
            any_hit = true;
            any_head |= loc.is_head();
            let dmg = def.bullet_damage(d, loc) * remaining;
            let at = origin + dir * d;
            let lethal = z.hp <= dmg;
            let mut dead = zombies::apply_damage(&mut z, dmg, insta);
            if !dead && def.is_projectile() {
                // The zombie damage script adds `round * RandomInt(100, 500)`
                // to projectile hits (RandomInt takes one argument).
                dead = zombies::apply_damage(&mut z, round_no as f32 * fastrand::u32(0..100) as f32, false);
            }
            if dead {
                // Kill bonus by hit location: head 100, neck 70, torso 60, limbs 50.
                let kind = if def.is_projectile() { KillKind::Explosive } else { KillKind::from_hit(loc) };
                if loc.is_head() {
                    score.headshots += 1;
                }
                if gibs && loc.gibs_head() {
                    // The head pops.
                    alias.write(PlayAlias::at("zombie_head_gib", at).or(Sfx::Headshot).volume(0.8));
                }
                earn(&mut score, &mut points, &pu, rules::kill_points_with(kind, insta, lethal));
                killed.write(ZombieKilled { pos: at, drop_allowed: true });
                spawn_burst(&mut commands, &mats, at, &mats.blood, 10, 2.5);
            } else {
                earn(&mut score, &mut points, &pu, rules::POINTS_HIT);
                spawn_burst(&mut commands, &mats, at, &mats.blood, 4, 1.5);
            }
            fx.write(crate::fx::FxEvent::FleshImpact { weapon: def.id, pos: at, dir, head: loc.is_head(), fatal: dead });
            remaining -= if def.flesh_penetration > 0.0 { flesh_thickness(loc) / def.flesh_penetration } else { 1.0 };
            if def.is_projectile() || remaining <= 0.0 {
                end_t = d;
                break;
            }
        }
        let end = origin + dir * end_t;
        let color = if def.kind == Kind::Wonder { Color::srgb(0.3, 1.0, 0.4) } else { Color::srgba(1.0, 0.9, 0.6, 0.6) };
        tracers.0.push((muzzle, end, if def.kind == Kind::Wonder { 0.12 } else { 0.035 }, color));
        if end_t >= wall_t && end_t < max {
            spawn_burst(&mut commands, &mats, end - dir * 0.05, &mats.spark, 3, 2.0);
            fx.write(crate::fx::FxEvent::BulletImpact { weapon: def.id, pos: end, dir });
        }

        // The explosion where a projectile lands (Ray Gun): from the inner
        // damage at the centre to the outer at the edge of the radius, to
        // every body in view of it.
        if def.splash_radius > 0.0 {
            let at = end - dir * 0.05;
            spawn_burst(&mut commands, &mats, at, &mats.glow_green, 14, 4.0);
            fx.write(crate::fx::FxEvent::Explosion { weapon: def.id, pos: at });
            for (_, t, mut z) in zq.iter_mut() {
                if !z.alive() {
                    continue;
                }
                let feet = t.translation;
                let head = feet + Vec3::Y * 1.63 * z.scale;
                let near = closest_on_segment(at, feet, head);
                let dist = near.distance(at);
                let Some(dmg) = def.splash_at(dist) else { continue };
                if dist > 0.05 {
                    let to = (near - at) / dist;
                    if wall_distance(&world, at, to, dist) < dist - 0.05 {
                        continue;
                    }
                }
                let mut dead = zombies::apply_damage(&mut z, dmg, insta);
                if !dead {
                    dead = zombies::apply_damage(&mut z, round_no as f32 * fastrand::u32(0..100) as f32, false);
                }
                if dead {
                    earn(&mut score, &mut points, &pu, rules::kill_points(KillKind::Explosive));
                    killed.write(ZombieKilled { pos: feet, drop_allowed: true });
                } else {
                    earn(&mut score, &mut points, &pu, rules::POINTS_HIT);
                }
                any_hit = true;
            }
        }
    }
    if any_hit {
        gun.hitmarker = 0.12;
        gun.headmarker = any_head;
        // Nacht has no hit sound; keep the stand-in for the prototype map.
        if zs.aliases.is_empty() {
            sfx.write(PlaySfx::at(if any_head { Sfx::Headshot } else { Sfx::Hit }, 0.5));
        }
    }
}

fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    a + ab * t
}

#[allow(clippy::type_complexity)]
fn knife(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    pu: Res<ActivePowerups>,
    mut gun: ResMut<Gun>,
    mut score: ResMut<Score>,
    player: Query<&Transform, With<Player>>,
    mut zq: Query<(&Transform, &mut Zombie), Without<Player>>,
    (mut points, mut killed): (EventWriter<PointsEvent>, EventWriter<ZombieKilled>),
    (zs, defs, loadout, mut alias, mut fx): (Res<ZoneSounds>, Res<Defs>, Option<Res<Loadout>>, EventWriter<PlayAlias>, EventWriter<crate::fx::FxEvent>),
    mut commands: Commands,
    mats: Res<Mats>,
) {
    let dt = time.delta_secs();
    gun.knife_cd = (gun.knife_cd - dt).max(0.0);
    gun.knife_anim = (gun.knife_anim - dt).max(0.0);
    if !cursor_locked(&windows) || !(keys.just_pressed(KeyCode::KeyV) || keys.just_pressed(KeyCode::KeyE)) || gun.knife_cd > 0.0 {
        return;
    }
    let Ok(cam) = player.single() else { return };
    let held_def = loadout.map(|l| &defs.0[l.current().def]);
    // `meleeTime` (0.5 s for every weapon on Nacht) until the next swing.
    gun.knife_cd = held_def.map_or(0.65, |d| d.melee_time.max(0.35));
    gun.knife_anim = 0.35;
    gun.reload = None;
    gun.burst_left = 0;
    let melee_damage = held_def.map_or(150.0, |d| d.melee_damage);
    let held = held_def.map_or("", |d| d.id);
    if let Some(a) = weapon_sound(&zs, held, "meleeSwipeSoundPlayer", Sfx::Knife) {
        alias.write(a.volume(0.8));
    }
    let fwd = cam.forward().as_vec3();
    let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
    let me = Vec3::new(cam.translation.x, 0.0, cam.translation.z);
    let target = zq
        .iter_mut()
        .filter(|(_, z)| z.alive())
        .filter_map(|(t, z)| {
            let p = Vec3::new(t.translation.x, 0.0, t.translation.z);
            let d = p.distance(me);
            let facing = (p - me).normalize_or_zero().dot(flat);
            (d < KNIFE_RANGE && facing > 0.55).then_some((d, t.translation, z))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, pos, mut z)) = target {
        let hit_point = pos + Vec3::Y * 1.2;
        alias.write(PlayAlias::at("melee_hit", hit_point));
        let lethal = z.hp <= melee_damage;
        let fatal = zombies::apply_damage(&mut z, melee_damage, pu.insta_kill > 0.0);
        fx.write(crate::fx::FxEvent::FleshImpact { weapon: held, pos: hit_point, dir: fwd, head: false, fatal });
        if fatal {
            earn(&mut score, &mut points, &pu, rules::kill_points_with(KillKind::Melee, pu.insta_kill > 0.0, lethal));
            killed.write(ZombieKilled { pos, drop_allowed: true });
            spawn_burst(&mut commands, &mats, hit_point, &mats.blood, 10, 2.5);
        } else {
            earn(&mut score, &mut points, &pu, rules::POINTS_HIT);
            spawn_burst(&mut commands, &mats, hit_point, &mats.blood, 5, 1.5);
        }
        gun.hitmarker = 0.12;
        gun.headmarker = false;
    }
}

pub fn spawn_burst(commands: &mut Commands, mats: &Mats, at: Vec3, mat: &Handle<StandardMaterial>, n: usize, speed: f32) {
    // The game's own effects replace these stand-ins when they are loaded.
    if crate::fx::live() {
        return;
    }
    for _ in 0..n {
        let v = Vec3::new(fastrand::f32() - 0.5, fastrand::f32() * 0.8, fastrand::f32() - 0.5).normalize_or_zero() * speed * (0.4 + fastrand::f32());
        let size = 0.03 + fastrand::f32() * 0.05;
        commands.spawn((
            Mesh3d(mats.sphere.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            Fx { ttl: 0.5, max: 0.5, vel: v, size },
            NotShadowCaster,
            Dynamic,
        ));
    }
}

fn update_fx(time: Res<Time>, mut commands: Commands, mut q: Query<(Entity, &mut Transform, &mut Fx)>) {
    let dt = time.delta_secs();
    for (e, mut t, mut fx) in &mut q {
        fx.ttl -= dt;
        if fx.ttl <= 0.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        fx.vel.y -= 9.0 * dt;
        t.translation += fx.vel * dt;
        if t.translation.y < 0.02 {
            t.translation.y = 0.02;
            fx.vel = Vec3::ZERO;
        }
        t.scale = Vec3::splat(fx.size * (fx.ttl / fx.max).max(0.2));
    }
}

fn draw_tracers(time: Res<Time>, mut tracers: ResMut<Tracers>, mut gizmos: Gizmos) {
    let dt = time.delta_secs();
    for (a, b, _, c) in &tracers.0 {
        gizmos.line(*a, *b, *c);
    }
    tracers.0.iter_mut().for_each(|t| t.2 -= dt);
    tracers.0.retain(|t| t.2 > 0.0);
}

fn spawn_viewmodel(mut commands: Commands, player: Query<Entity, With<Player>>, mats: Res<Mats>, mut meshes: ResMut<Assets<Mesh>>) {
    let Ok(cam) = player.single() else { return };
    let root = commands
        .spawn((Transform::from_xyz(0.22, -0.2, -0.45), Visibility::default(), ViewModel { shown: None, rig: false }))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.02, 0.025, 0.28))),
                MeshMaterial3d(mats.gun_metal.clone()),
                Transform::from_xyz(-0.08, -0.02, -0.1),
                Visibility::Hidden,
                KnifeModel,
                NotShadowCaster,
            ));
            p.spawn((
                PointLight { color: Color::srgb(1.0, 0.7, 0.3), intensity: 0.0, range: 6.0, ..default() },
                Mesh3d(mats.sphere.clone()),
                MeshMaterial3d(mats.spark.clone()),
                Transform::from_xyz(0.0, 0.02, -0.55).with_scale(Vec3::ZERO),
                MuzzleFlash,
                NotShadowCaster,
            ));
        })
        .id();
    commands.entity(cam).add_child(root);
}

fn build_gun(p: &mut ChildSpawnerCommands, def: &WeaponDef, mats: &Mats) {
    // Parts are authored at full size; the GunModel parent scales them down.
    let part = |p: &mut ChildSpawnerCommands, size: Vec3, pos: Vec3, m: &Handle<StandardMaterial>| {
        p.spawn((
            Mesh3d(mats.cube.clone()),
            MeshMaterial3d(m.clone()),
            Transform::from_translation(pos).with_scale(size),
            NotShadowCaster,
        ));
    };
    let (len, stock, mag) = match def.kind {
        Kind::Pistol => (0.18, false, false),
        Kind::Smg => (0.42, true, true),
        Kind::Shotgun => (0.62, true, false),
        Kind::Lmg => (0.68, true, true),
        Kind::Rifle => (0.7, true, def.clip > 8),
        Kind::Wonder => (0.26, false, false),
    };
    if def.kind == Kind::Wonder {
        part(p, Vec3::new(0.08, 0.1, 0.2), Vec3::new(0.0, 0.0, -0.05), &mats.gun_metal);
        part(p, Vec3::new(0.05, 0.12, 0.06), Vec3::new(0.0, -0.09, 0.03), &mats.gun_wood);
        for i in 0..3 {
            part(p, Vec3::new(0.1, 0.1, 0.02), Vec3::new(0.0, 0.0, -0.17 - i as f32 * 0.05), &mats.glow_green);
        }
        part(p, Vec3::new(0.03, 0.03, 0.12), Vec3::new(0.0, 0.0, -0.3), &mats.glow_red);
        return;
    }
    part(p, Vec3::new(0.06, 0.08, len * 0.55), Vec3::new(0.0, 0.0, -len * 0.2), &mats.gun_metal);
    part(p, Vec3::new(0.025, 0.025, len * 0.5), Vec3::new(0.0, 0.02, -len * 0.62), &mats.gun_metal);
    part(p, Vec3::new(0.05, 0.04, len * 0.45), Vec3::new(0.0, -0.04, -len * 0.4), &mats.gun_wood);
    part(p, Vec3::new(0.045, 0.1, 0.05), Vec3::new(0.0, -0.08, 0.02), &mats.gun_wood);
    if stock {
        part(p, Vec3::new(0.05, 0.09, 0.24), Vec3::new(0.0, -0.03, 0.18), &mats.gun_wood);
    }
    if mag {
        part(p, Vec3::new(0.035, 0.14, 0.05), Vec3::new(0.0, -0.11, -len * 0.25), &mats.gun_metal);
    }
    if def.kind == Kind::Shotgun && def.clip == 2 {
        part(p, Vec3::new(0.025, 0.025, len * 0.5), Vec3::new(0.03, 0.02, -len * 0.62), &mats.gun_metal);
    }
}

#[allow(clippy::type_complexity)]
fn update_viewmodel(
    mut commands: Commands,
    time: Res<Time>,
    defs: Res<Defs>,
    loadout: Option<Res<Loadout>>,
    gun: Res<Gun>,
    mats: Res<Mats>,
    view_models: Res<crate::nacht::ViewModels>,
    view_rig: Res<crate::nacht::ViewRig>,
    player: Query<&PlayerCtl, With<Player>>,
    mut vm: Query<(Entity, &mut Transform, &mut ViewModel, &mut Visibility), (Without<KnifeModel>, Without<MuzzleFlash>)>,
    models: Query<Entity, With<GunModel>>,
    mut knife: Query<(&mut Transform, &mut Visibility), (With<KnifeModel>, Without<MuzzleFlash>, Without<ViewModel>)>,
    mut flash: Query<(Entity, &mut Transform, &mut PointLight), (With<MuzzleFlash>, Without<KnifeModel>, Without<ViewModel>)>,
) {
    let Some(loadout) = loadout else { return };
    let Ok((root, mut t, mut vm, mut vis)) = vm.single_mut() else { return };
    let Ok(ctl) = player.single() else { return };
    let def_idx = loadout.current().def;
    // Scoped weapons show their scope overlay instead of the gun once fully
    // aimed (the HUD draws it).
    let scoped = scope_shown(&defs.0[def_idx], ctl);
    vis.set_if_neq(if scoped { Visibility::Hidden } else { Visibility::Inherited });
    if vm.shown != Some(def_idx) || ((view_models.is_changed() || view_rig.is_changed()) && !view_models.0.is_empty()) {
        // Keep the muzzle flash: it may be attached to the old gun's flash tag.
        if let Ok((fe, ..)) = flash.single() {
            commands.entity(fe).insert(ChildOf(root));
        }
        for m in &models {
            commands.entity(m).try_despawn();
        }
        let def = defs.0[def_idx].clone();
        vm.shown = Some(def_idx);
        // The game's arms with the weapon on them, animated.
        if let (Some(arms), Some(gun_part)) = (&view_rig.arms, view_rig.guns.get(def.id)) {
            let rig_root = commands
                .spawn((
                    Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                    Visibility::default(),
                    GunModel,
                    ChildOf(root),
                ))
                .id();
            let mut joints = Vec::new();
            crate::nacht::spawn_part(&mut commands, arms, &mut joints, rig_root, rig_root);
            let tag_weapon = joints.iter().find(|j| j.0 == "tag_weapon").map(|j| j.1).unwrap_or(rig_root);
            crate::nacht::spawn_part(&mut commands, gun_part, &mut joints, tag_weapon, rig_root);
            if let (Some(tag_flash), Ok((fe, ..))) = (joints.iter().find(|j| j.0 == "tag_flash").map(|j| j.1), flash.single()) {
                commands.entity(fe).insert(ChildOf(tag_flash));
            }
            commands.entity(rig_root).insert(ViewRigState { joints, weapon: def.id });
            vm.rig = true;
        } else {
            vm.rig = false;
        }
        let mats_ref: &Mats = &mats;
        let real = if vm.rig { None } else { view_models.0.get(def.id).cloned() };
        let rig = vm.rig;
        commands.entity(root).with_children(|p| match real {
            // The game's own gun model: grip at the origin, barrel along +X.
            Some(parts) => {
                p.spawn((
                    Transform::from_xyz(0.0, -0.02, 0.06).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                    Visibility::default(),
                    GunModel,
                ))
                .with_children(|g| {
                    for (mesh, mat) in parts {
                        g.spawn((Mesh3d(mesh), MeshMaterial3d(mat), NotShadowCaster));
                    }
                });
            }
            None if !rig => {
                p.spawn((Transform::from_scale(Vec3::splat(0.8)), Visibility::default(), GunModel)).with_children(|g| build_gun(g, &def, mats_ref));
            }
            None => {}
        });
    }

    if vm.rig {
        // Animations place the arms; only add a little walk bob and recoil.
        let tt = time.elapsed_secs();
        let sway = if ctl.moving { 1.0 - 0.8 * ctl.ads } else { 0.25 };
        t.translation = Vec3::new((ctl.bob * 0.5).sin() * 0.008, (ctl.bob).sin().abs() * -0.008 + (tt * 1.6).sin() * 0.002, gun.kick * 0.02) * sway.max(0.3);
        t.rotation = Quat::from_rotation_x(gun.kick * 0.03);
        if let Ok((_, mut kv)) = knife.single_mut() {
            *kv = Visibility::Hidden;
        }
        if let Ok((_, mut ft, mut fl)) = flash.single_mut() {
            let on = gun.flash > 0.0;
            // The game's own flash effect brings its light when effects are
            // loaded; this stand-in is for the prototype map only.
            fl.intensity = if on && !crate::fx::live() { 120_000.0 } else { 0.0 };
            ft.translation = Vec3::ZERO;
            ft.scale = if on && !crate::fx::live() { Vec3::splat(0.07 + fastrand::f32() * 0.05) } else { Vec3::ZERO };
        }
        return;
    }
    let hip = Vec3::new(0.21, -0.19, -0.44);
    let ads = Vec3::new(0.0, -0.13, -0.36);
    let mut pos = hip.lerp(ads, ctl.ads);
    let mut rot = Quat::IDENTITY;
    let tt = time.elapsed_secs();
    let sway = if ctl.moving { 1.0 - 0.8 * ctl.ads } else { 0.25 };
    pos += Vec3::new((ctl.bob * 0.5).sin() * 0.012, (ctl.bob).sin().abs() * -0.012, 0.0) * sway;
    pos.y += (tt * 1.6).sin() * 0.003;
    pos.z += gun.kick * 0.05;
    rot *= Quat::from_rotation_x(gun.kick * 0.08);
    if ctl.sprinting {
        pos += Vec3::new(-0.05, -0.06, 0.05);
        rot *= Quat::from_euler(EulerRot::XYZ, -0.3, 0.5, 0.2);
    }
    if let Some(r) = gun.reload {
        let k = match r.phase {
            ReloadPhase::Full => (std::f32::consts::PI * r.progress()).sin(),
            ReloadPhase::Start => (std::f32::consts::FRAC_PI_2 * r.progress()).sin(),
            ReloadPhase::Loop => 1.0,
            ReloadPhase::End => (std::f32::consts::FRAC_PI_2 * (1.0 - r.progress())).sin(),
        };
        pos.y -= 0.12 * k;
        rot *= Quat::from_euler(EulerRot::XYZ, -0.6 * k, 0.0, 0.5 * k);
    }
    if gun.switch > 0.0 {
        pos.y -= (gun.switch / gun.switch_total.max(gun.switch).max(0.01)) * 0.27;
    }
    if gun.knife_anim > 0.0 {
        pos += Vec3::new(0.1, -0.15, 0.1);
    }
    t.translation = pos;
    t.rotation = rot;

    if let Ok((mut kt, mut kv)) = knife.single_mut() {
        if gun.knife_anim > 0.0 {
            *kv = Visibility::Visible;
            let k = 1.0 - gun.knife_anim / 0.35;
            kt.translation = Vec3::new(-0.25 + k * 0.3, 0.05, -0.25 - (k * std::f32::consts::PI).sin() * 0.25);
            kt.rotation = Quat::from_rotation_y(0.8 - k * 1.6);
        } else {
            *kv = Visibility::Hidden;
        }
    }
    if let Ok((_, mut ft, mut fl)) = flash.single_mut() {
        let on = gun.flash > 0.0;
        fl.intensity = if on && !crate::fx::live() { 120_000.0 } else { 0.0 };
        let def = &defs.0[def_idx];
        let tip = match def.kind {
            Kind::Pistol => -0.22,
            Kind::Wonder => -0.38,
            Kind::Smg => -0.55,
            _ => -0.8,
        };
        ft.translation = Vec3::new(0.0, 0.02, tip);
        ft.scale = if on { Vec3::splat(0.07 + fastrand::f32() * 0.05) } else { Vec3::ZERO };
    }
}

/// Plays the weapon's own animations on the first-person arms: raise when
/// switching, fire (and the bolt rechamber), reload, knife, sprint and idle;
/// aiming down the sights follows the `ads_up` animation.
#[allow(clippy::too_many_arguments)]
fn animate_view_rig(
    time: Res<Time>,
    defs: Res<Defs>,
    loadout: Option<Res<Loadout>>,
    gun: Res<Gun>,
    view_rig: Res<crate::nacht::ViewRig>,
    player: Query<&PlayerCtl, With<Player>>,
    rig_q: Query<&ViewRigState>,
    mut tq: Query<&mut Transform>,
    (zs, mut alias, mut last): (Res<ZoneSounds>, EventWriter<PlayAlias>, Local<Option<(String, f32)>>),
) {
    let (Some(loadout), Ok(rig), Ok(ctl)) = (loadout, rig_q.single(), player.single()) else { return };
    let Some(anims) = view_rig.anims.get(rig.weapon) else { return };
    let def = &defs.0[loadout.current().def];
    let get = |slot: &str| anims.get(slot).cloned();
    let knife = view_rig.anims.get("__knife").and_then(|a| a.get("melee").cloned());
    // (clip, normalised progress 0..1)
    let progress = |c: &std::sync::Arc<crate::nacht::build::AnimClip>, secs: f32| if c.duration > 0.0 { (secs / c.duration).clamp(0.0, 1.0) } else { 1.0 };
    let empty = loadout.current().clip == 0;
    let choice = if gun.knife_anim > 0.0 {
        knife.map(|k| (k.clone(), 1.0 - gun.knife_anim / 0.35))
    } else if gun.switch > 0.0 {
        // The new weapon comes up over the last `raiseTime` of the switch
        // (the first part is the old one going down).
        let total = gun.switch_total.max(gun.switch).max(0.01);
        let raise = def.raise_time.min(total).max(0.01);
        let elapsed = total - gun.switch;
        get("raise").map(|c| (c, ((elapsed - (total - raise)) / raise).clamp(0.0, 1.0)))
    } else if let Some(r) = gun.reload {
        let c = match r.phase {
            ReloadPhase::Full if gun.reload_empty => get("reload_empty").or_else(|| get("reload")),
            ReloadPhase::Full | ReloadPhase::Loop => get("reload"),
            ReloadPhase::Start => get("reload_start"),
            ReloadPhase::End => get("reload_end"),
        };
        c.map(|c| (c, r.progress()))
    } else {
        // The shot that empties the clip has its own animation (the
        // Garand's clip flies out).
        let last = if empty { get("last_shot") } else { None };
        let fire = last.or_else(|| get(if ctl.ads > 0.5 { "ads_fire" } else { "fire" })).or_else(|| get("fire"));
        let rechamber = get("rechamber").filter(|_| def.rechamber);
        match (gun.since_shot, fire) {
            (Some(s), Some(f)) if s < f.duration => Some((f.clone(), progress(&f, s))),
            (Some(s), Some(f)) if rechamber.as_ref().is_some_and(|r| s < f.duration + r.duration) => {
                let r = rechamber.unwrap();
                let p = progress(&r, s - f.duration);
                Some((r, p))
            }
            _ if ctl.sprinting && get("sprint_loop").is_some() => {
                let c = get("sprint_loop").unwrap();
                let p = (time.elapsed_secs() / c.duration.max(0.01)).fract();
                Some((c, p))
            }
            _ => get(if empty { "empty_idle" } else { "idle" }).or_else(|| get("idle")).map(|c| (c, 0.0)),
        }
    };
    if let Some((clip, p)) = choice.as_ref() {
        // Notetrack sounds crossed since the last frame (a new clip starts
        // from its beginning; looping clips wrap).
        let p = p.clamp(0.0, 1.0);
        let from = match last.as_ref() {
            Some((name, q)) if *name == clip.name => Some(*q),
            _ => None,
        };
        for (note, t) in &clip.notify {
            let crossed = match from {
                Some(q) if p >= q => *t > q && *t <= p,
                Some(q) => *t > q || *t <= p,
                None => *t <= p && p > 0.0,
            };
            let crossed = crossed || (from.is_none() && *t == 0.0);
            if crossed {
                if let Some(a) = zs.notetrack(rig.weapon, note) {
                    alias.write(PlayAlias::local(a));
                }
            }
        }
        *last = Some((clip.name.clone(), p));
    }
    if let Some((clip, p)) = choice {
        let map = crate::nacht::track_map(&clip, &rig.joints);
        crate::nacht::pose_mapped(&clip, p.clamp(0.0, 1.0) * clip.numframes, &rig.joints, &map, &mut tq, None);
    }
    // Hip to sights: tag_torso follows the ADS animation by the aim amount.
    if let Some(ads) = get("ads_up") {
        let map = crate::nacht::track_map(&ads, &rig.joints);
        crate::nacht::pose_mapped(&ads, ctl.ads.clamp(0.0, 1.0) * ads.numframes, &rig.joints, &map, &mut tq, Some("tag_torso"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zm_core::weapons::{default_weapons, find};

    fn def(id: &str) -> WeaponDef {
        let defs = default_weapons();
        defs[find(&defs, id).unwrap()].clone()
    }

    /// Runs a reload to the end in `dt` steps; returns (seconds, clip after
    /// each step).
    fn run(def: &WeaponDef, slot: &mut Slot, dt: f32) -> (f32, Vec<u32>) {
        let mut r = Some(ReloadState::begin(def, slot.clip == 0));
        let (mut t, mut clips) = (0.0, Vec::new());
        while let Some(s) = r {
            r = step_reload(s, def, slot, dt).0;
            t += dt;
            clips.push(slot.clip);
            assert!(t < 30.0);
        }
        (t, clips)
    }

    #[test]
    fn magazine_goes_in_at_the_add_time() {
        let mp40 = def("mp40");
        let mut slot = Slot { def: 0, clip: 10, reserve: 100 };
        let (t, clips) = run(&mp40, &mut slot, 0.05);
        assert!((t - 2.3).abs() < 0.06, "{t}");
        // 1.85 s in (step 37) the clip is full; before, it is not.
        assert_eq!(clips[35], 10);
        assert_eq!(clips[37], 32);
        assert_eq!((slot.clip, slot.reserve), (32, 78));
        // Empty: 2.9 s.
        let mut slot = Slot { def: 0, clip: 0, reserve: 10 };
        let (t, _) = run(&mp40, &mut slot, 0.05);
        assert!((t - 2.9).abs() < 0.06, "{t}");
        assert_eq!((slot.clip, slot.reserve), (10, 0));
    }

    #[test]
    fn shotgun_loads_shell_by_shell() {
        let s = def("trenchgun");
        let mut slot = Slot { def: 0, clip: 3, reserve: 10 };
        let (t, _) = run(&s, &mut slot, 0.01);
        // Start 0.9 (one shell in), two loops of 0.6, end 0.95.
        assert!((t - (0.9 + 1.2 + 0.95)).abs() < 0.05, "{t}");
        assert_eq!((slot.clip, slot.reserve), (6, 7));
        // Firing during the loop goes to the end phase.
        let mut slot = Slot { def: 0, clip: 0, reserve: 10 };
        let r = ReloadState::begin(&s, true);
        let (r, _) = step_reload(r, &s, &mut slot, 0.95);
        assert_eq!((r.unwrap().phase, slot.clip), (ReloadPhase::Loop, 1));
        let end = ReloadState { interrupt: true, ..ReloadState::end(&s) };
        assert_eq!(end.phase, ReloadPhase::End);
        assert!((end.dur - 0.95).abs() < 1e-5);
    }

    #[test]
    fn reload_stops_when_reserve_runs_out() {
        let k = def("kar98k_scoped_zombie");
        let mut slot = Slot { def: 0, clip: 1, reserve: 2 };
        run(&k, &mut slot, 0.02);
        assert_eq!((slot.clip, slot.reserve), (3, 0));
    }

    #[test]
    fn handling_numbers() {
        let h = HeldWeapon::of(&def("ptrs41_zombie"));
        assert_eq!(h.move_speed_scale, 0.75);
        assert_eq!(h.ads_fov(65.0), 10.0);
        let t = HeldWeapon::of(&def("thompson"));
        assert!((t.ads_move_speed_scale - 1.3).abs() < 1e-6);
        assert!((t.ads_in_time - 0.22).abs() < 1e-6);
    }

    #[test]
    fn spawn_and_box_ammo() {
        let defs = default_weapons();
        let mut l = Loadout::starting(&defs);
        assert_eq!((l.current().clip, l.current().reserve), (8, 32));
        let t = find(&defs, "thompson").unwrap();
        l.give(&defs, t);
        assert_eq!((l.current().clip, l.current().reserve), (20, 200));
        l.slots[l.cur].reserve = 3;
        l.refill_all(&defs);
        assert_eq!(l.current().reserve, 200);
    }

    /// Every weapon reads its own file from a real install:
    /// `UNDEAD_WAW=<install> cargo test -p zm_game -- --ignored`.
    #[test]
    #[ignore]
    fn reads_real_weapon_files() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let iwd = waw_assets::Iwd::open(&std::path::Path::new(&root).join("main")).unwrap();
        let mut defs = default_weapons();
        let builtin = defs.clone();
        for d in defs.iter_mut() {
            let file = d.weapon_file.unwrap();
            let bytes = iwd.read(&format!("weapons/sp/{file}")).unwrap_or_else(|| panic!("{file}"));
            let wf = zm_core::weaponfile::WeaponFile::parse_bytes(&bytes).unwrap();
            assert!(d.apply_weapon_file(&wf) > 30, "{file}");
        }
        // The built-in table holds the same numbers as the files.
        for (a, b) in defs.iter().zip(&builtin) {
            assert_eq!(crate::audio::weapon_summary(a), crate::audio::weapon_summary(b));
        }
        let d = |id: &str| &defs[find(&defs, id).unwrap()];
        assert_eq!((d("trenchgun").damage, d("trenchgun").pellets), (160.0, 8));
        assert_eq!(d("kar98k").location_multiplier(HitLoc::Head), 3.5);
        assert_eq!(d("raypistol").mode, FireMode::Auto);
        assert_eq!(d("ptrs41_zombie").damage, 1000.0);
    }
}
