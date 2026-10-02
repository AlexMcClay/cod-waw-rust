//! Weapon table.
//!
//! Every number is World at War's own: at load time each weapon reads its
//! definition from the player's install (the `weapons/sp/<name>` weapon file
//! in the IWD archives, then the compiled `WeaponDef` in the map's zone,
//! which is what the game itself uses; see [`WeaponDef::apply_weapon_file`]).
//! The built-in values in [`crate::weapon_defaults`] are the same numbers,
//! used only when there is no install. Per-weapon values and their sources
//! are listed in `research/gameplay/WEAPONS.md`.
//!
//! Units: distances in metres, times in seconds, angles in degrees (weapon
//! files use inches; [`UNITS_TO_M`] converts).

use crate::weaponfile::WeaponFile;

/// Engine units in weapon files are inches; the game world is in metres.
pub const UNITS_TO_M: f32 = 0.0254;

/// Hit locations in the engine's `hitLocation_t` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HitLoc {
    None,
    Helmet,
    Head,
    Neck,
    TorsoUpper,
    TorsoLower,
    RightArmUpper,
    LeftArmUpper,
    RightArmLower,
    LeftArmLower,
    RightHand,
    LeftHand,
    RightLegUpper,
    LeftLegUpper,
    RightLegLower,
    LeftLegLower,
    RightFoot,
    LeftFoot,
    Gun,
}

pub const HITLOC_COUNT: usize = 19;

impl HitLoc {
    pub const ALL: [HitLoc; HITLOC_COUNT] = [
        HitLoc::None,
        HitLoc::Helmet,
        HitLoc::Head,
        HitLoc::Neck,
        HitLoc::TorsoUpper,
        HitLoc::TorsoLower,
        HitLoc::RightArmUpper,
        HitLoc::LeftArmUpper,
        HitLoc::RightArmLower,
        HitLoc::LeftArmLower,
        HitLoc::RightHand,
        HitLoc::LeftHand,
        HitLoc::RightLegUpper,
        HitLoc::LeftLegUpper,
        HitLoc::RightLegLower,
        HitLoc::LeftLegLower,
        HitLoc::RightFoot,
        HitLoc::LeftFoot,
        HitLoc::Gun,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The weapon-file key of this location's damage multiplier.
    pub fn key(self) -> &'static str {
        [
            "locNone",
            "locHelmet",
            "locHead",
            "locNeck",
            "locTorsoUpper",
            "locTorsoLower",
            "locRightArmUpper",
            "locLeftArmUpper",
            "locRightArmLower",
            "locLeftArmLower",
            "locRightHand",
            "locLeftHand",
            "locRightLegUpper",
            "locLeftLegUpper",
            "locRightLegLower",
            "locLeftLegLower",
            "locRightFoot",
            "locLeftFoot",
            "locGun",
        ][self.index()]
    }

    /// The name scripts see (`self.damageLocation`).
    pub fn script_name(self) -> &'static str {
        [
            "none",
            "helmet",
            "head",
            "neck",
            "torso_upper",
            "torso_lower",
            "right_arm_upper",
            "left_arm_upper",
            "right_arm_lower",
            "left_arm_lower",
            "right_hand",
            "left_hand",
            "right_leg_upper",
            "left_leg_upper",
            "right_leg_lower",
            "left_leg_lower",
            "right_foot",
            "left_foot",
            "gun",
        ][self.index()]
    }

    /// Head or helmet: the zombie script's headshot bonus locations.
    pub fn is_head(self) -> bool {
        matches!(self, HitLoc::Head | HitLoc::Helmet)
    }

    /// Head, helmet or neck: where a hit can gib the head (`head_should_gib`).
    pub fn gibs_head(self) -> bool {
        matches!(self, HitLoc::Head | HitLoc::Helmet | HitLoc::Neck)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireMode {
    Semi,
    Auto,
    /// Fires this many rounds per trigger pull.
    Burst(u32),
}

impl FireMode {
    /// From a weapon file's `fireType`.
    pub fn parse(s: &str) -> Option<FireMode> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "full auto" => Some(FireMode::Auto),
            "single shot" => Some(FireMode::Semi),
            _ => s.split('-').next().and_then(|n| n.parse().ok()).filter(|_| s.contains("burst")).map(FireMode::Burst),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pistol,
    Rifle,
    Smg,
    Shotgun,
    Lmg,
    /// A projectile with an explosion at the impact point (the Ray Gun).
    Wonder,
}

/// `penetrateType`: how deep a bullet can go through a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Penetrate {
    None,
    Small,
    Medium,
    Large,
}

impl Penetrate {
    pub fn parse(s: &str) -> Option<Penetrate> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Penetrate::None),
            "small" => Some(Penetrate::Small),
            "medium" => Some(Penetrate::Medium),
            "large" => Some(Penetrate::Large),
            _ => None,
        }
    }
}

/// How far (inches) a bullet goes through flesh, by [`Penetrate`] type:
/// `small_flesh`, `medium_flesh`, `large_flesh` of the game's
/// `info/bullet_penetration_sp` table (common.ff). Replaced from the install
/// when the zone is read ([`parse_penetration_table`]).
pub const FLESH_PENETRATION: [f32; 4] = [0.0, 32.0, 96.0, 128.0];

/// The `flesh` depths (inches; none, small, medium, large) of a
/// `BULLET_PEN_TABLE\small_flesh\32\...` raw file.
pub fn parse_penetration_table(text: &str) -> Option<[f32; 4]> {
    let mut parts = text.split('\\');
    if !parts.next()?.trim().eq_ignore_ascii_case("BULLET_PEN_TABLE") {
        return None;
    }
    let rest: Vec<&str> = parts.collect();
    let mut out = [0.0; 4];
    let mut found = 0;
    for kv in rest.chunks(2).filter(|kv| kv.len() == 2) {
        let i = match kv[0].trim() {
            "small_flesh" => 1,
            "medium_flesh" => 2,
            "large_flesh" => 3,
            _ => continue,
        };
        out[i] = kv[1].trim().parse().ok()?;
        found += 1;
    }
    (found == 3).then_some(out)
}

/// Hip and aim-down-the-sights spread (degrees, half-angle of the cone).
///
/// The engine keeps an aim-spread scale 0..1 per player: firing adds
/// `fire_add`, moving adds `move_add` per second, otherwise it decays at
/// `decay` per second (times the crouched/prone factors). The hip spread is
/// `min + (max - min) * scale` for the stance; aiming lerps to `ads`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spread {
    pub stand: (f32, f32),
    pub ducked: (f32, f32),
    pub prone: (f32, f32),
    pub ads: f32,
    pub fire_add: f32,
    pub move_add: f32,
    pub turn_add: f32,
    pub decay: f32,
    pub ducked_decay: f32,
    pub prone_decay: f32,
}

impl Default for Spread {
    fn default() -> Self {
        Spread { stand: (2.0, 5.0), ducked: (1.5, 4.0), prone: (1.0, 3.0), ads: 0.0, fire_add: 0.5, move_add: 4.0, turn_add: 0.0, decay: 4.0, ducked_decay: 1.0, prone_decay: 1.0 }
    }
}

/// Stance for spread purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpreadStance {
    Stand,
    Ducked,
    Prone,
}

impl Spread {
    /// Cone half-angle for a stance, spread scale (0..1) and ADS fraction.
    pub fn cone(&self, stance: SpreadStance, scale: f32, ads: f32) -> f32 {
        let (lo, hi) = match stance {
            SpreadStance::Stand => self.stand,
            SpreadStance::Ducked => self.ducked,
            SpreadStance::Prone => self.prone,
        };
        let hip = lo + (hi - lo) * scale.clamp(0.0, 1.0);
        let ads = ads.clamp(0.0, 1.0);
        hip * (1.0 - ads) + self.ads * ads
    }

    /// Next aim-spread scale after `dt` seconds (`moving`: 0..1 of full speed).
    pub fn update_scale(&self, scale: f32, stance: SpreadStance, moving: f32, turning: f32, dt: f32) -> f32 {
        let increase = self.move_add * moving + self.turn_add * turning;
        let next = if increase > 0.0 {
            scale + increase * dt
        } else {
            let k = match stance {
                SpreadStance::Stand => 1.0,
                SpreadStance::Ducked => self.ducked_decay,
                SpreadStance::Prone => self.prone_decay,
            };
            scale - self.decay * k * dt
        };
        next.clamp(0.0, 1.0)
    }
}

/// Reload timings. A normal reload adds the magazine part-way through
/// (`add_time`); a segmented one (shotgun, scoped Kar98k) loads round by
/// round: start (adding `start_add` rounds at `start_add_time`), a loop of
/// `time` per `ammo_add` rounds, then the end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reload {
    pub time: f32,
    pub empty_time: f32,
    pub add_time: f32,
    pub empty_add_time: f32,
    pub start_time: f32,
    pub start_add_time: f32,
    pub end_time: f32,
    pub segmented: bool,
    pub ammo_add: u32,
    pub start_add: u32,
    pub no_partial: bool,
}

impl Default for Reload {
    fn default() -> Self {
        Reload { time: 2.5, empty_time: 0.0, add_time: 0.0, empty_add_time: 0.0, start_time: 0.0, start_add_time: 0.0, end_time: 0.0, segmented: false, ammo_add: 1, start_add: 0, no_partial: false }
    }
}

impl Reload {
    /// Length of a normal (non-segmented) reload.
    pub fn duration(&self, empty: bool) -> f32 {
        if empty && self.empty_time > 0.0 {
            self.empty_time
        } else {
            self.time
        }
    }

    /// When the magazine goes in, from the start of a normal reload. The
    /// game's files leave `reloadEmptyAddTime` at 0 and give the empty
    /// reload's add time as `reloadStartAddTime` (the IW3 convention); with
    /// neither, the normal add time; an unset or too-late time means at the
    /// end.
    pub fn ammo_in_at(&self, empty: bool) -> f32 {
        let d = self.duration(empty);
        let t = if empty && self.empty_time > 0.0 {
            [self.empty_add_time, self.start_add_time, self.add_time].into_iter().find(|t| *t > 0.0).unwrap_or(d)
        } else {
            self.add_time
        };
        if t <= 0.0 || t > d {
            d
        } else {
            t
        }
    }
}

#[derive(Debug, Clone)]
pub struct WeaponDef {
    pub id: &'static str,
    pub name: String,
    pub kind: Kind,
    /// The game's weapon name (`weapons/sp/<file>` and the zone WeaponDef).
    pub weapon_file: Option<&'static str>,
    /// Damage up to `near` metres, `min_damage` from `far`, linear between.
    pub damage: f32,
    pub min_damage: f32,
    pub near: f32,
    pub far: f32,
    /// Damage multiplier per [`HitLoc`].
    pub loc_mult: [f32; HITLOC_COUNT],
    pub melee_damage: f32,
    /// Seconds a knife attack takes, and when in it the blow lands.
    pub melee_time: f32,
    pub melee_delay: f32,
    pub penetrate: Penetrate,
    /// Inches of flesh a bullet goes through (from `penetrate`).
    pub flesh_penetration: f32,
    pub mode: FireMode,
    /// Seconds between shots (`fireTime`).
    pub fire_interval: f32,
    /// Bolt-action or pump: after a shot that leaves a round in the clip,
    /// `rechamber_time` more seconds pass before the next one.
    pub rechamber: bool,
    pub rechamber_time: f32,
    pub clip: u32,
    /// Most rounds carried besides the clip (`maxAmmo`).
    pub max_ammo: u32,
    /// Total rounds when first given (`startAmmo`, clip included).
    pub start_ammo: u32,
    pub reload: Reload,
    pub pellets: u32,
    pub spread: Spread,
    /// Seconds to raise the sights / lower them.
    pub ads_in_time: f32,
    pub ads_out_time: f32,
    /// Field of view while aimed (the game's `cg_fov` is 65).
    pub ads_zoom_fov: f32,
    pub move_speed_scale: f32,
    pub ads_move_speed_scale: f32,
    pub sprint_duration_scale: f32,
    pub raise_time: f32,
    pub drop_time: f32,
    pub first_raise_time: f32,
    /// Explosion at the impact point (Ray Gun): radius in metres, damage at
    /// the centre and at the edge.
    pub splash_radius: f32,
    pub splash_inner: f32,
    pub splash_damage: f32,
    /// Projectile speed in metres per second (0 = hitscan).
    pub projectile_speed: f32,
    /// Recoil kick (mean hip view kick pitch / 40).
    pub kick: f32,
}

impl WeaponDef {
    fn new(id: &'static str, name: &str, kind: Kind, file: &'static str) -> Self {
        let mut loc_mult = [1.0; HITLOC_COUNT];
        loc_mult[HitLoc::Gun.index()] = 0.0;
        Self {
            id,
            name: name.to_string(),
            kind,
            weapon_file: Some(file),
            damage: 50.0,
            min_damage: 0.0,
            // The engine's defaults: no falloff.
            near: 999_999.0 * UNITS_TO_M,
            far: 999_999.0 * UNITS_TO_M,
            loc_mult,
            melee_damage: 150.0,
            melee_time: 0.5,
            melee_delay: 0.05,
            penetrate: Penetrate::None,
            flesh_penetration: 0.0,
            mode: FireMode::Auto,
            fire_interval: 0.1,
            rechamber: false,
            rechamber_time: 0.0,
            clip: 10,
            max_ammo: 100,
            start_ammo: 100,
            reload: Reload::default(),
            pellets: 1,
            spread: Spread::default(),
            ads_in_time: 0.25,
            ads_out_time: 0.25,
            ads_zoom_fov: 50.0,
            move_speed_scale: 1.0,
            ads_move_speed_scale: 1.0,
            sprint_duration_scale: 1.0,
            raise_time: 0.5,
            drop_time: 0.4,
            first_raise_time: 0.8,
            splash_radius: 0.0,
            splash_inner: 0.0,
            splash_damage: 0.0,
            projectile_speed: 0.0,
            kick: 1.0,
        }
    }

    /// Built-in values (the game's, see [`crate::weapon_defaults`]).
    fn with(mut self, defaults: &str) -> Self {
        let wf = WeaponFile::parse(&format!("WEAPONFILE\\{defaults}")).expect("built-in weapon defaults");
        self.apply_weapon_file(&wf);
        self
    }

    /// Damage at `dist` metres before hit-location multipliers.
    pub fn damage_at(&self, dist: f32) -> f32 {
        if dist <= self.near || self.far <= self.near {
            self.damage
        } else if dist >= self.far {
            self.min_damage
        } else {
            let t = (dist - self.near) / (self.far - self.near);
            self.damage + (self.min_damage - self.damage) * t
        }
    }

    pub fn location_multiplier(&self, loc: HitLoc) -> f32 {
        self.loc_mult[loc.index()]
    }

    /// One bullet's damage at `dist` metres hitting `loc`. Projectile
    /// impacts (Ray Gun) are not scaled by location.
    pub fn bullet_damage(&self, dist: f32, loc: HitLoc) -> f32 {
        if self.is_projectile() {
            self.damage
        } else {
            self.damage_at(dist) * self.location_multiplier(loc)
        }
    }

    pub fn is_projectile(&self) -> bool {
        self.projectile_speed > 0.0 || self.kind == Kind::Wonder
    }

    /// How far a bullet (or a shotgun pellet) reaches: pellets stop at the
    /// minimum-damage range, bullets fly on.
    pub fn bullet_range(&self, default: f32) -> f32 {
        if self.pellets > 1 && self.far > 0.0 {
            self.far.min(default)
        } else {
            default
        }
    }

    /// Splash damage `dist` metres from the explosion (linear from the
    /// inner damage at the centre to the outer at the edge), if in range.
    pub fn splash_at(&self, dist: f32) -> Option<f32> {
        if self.splash_radius <= 0.0 || dist > self.splash_radius {
            return None;
        }
        let t = (dist / self.splash_radius).clamp(0.0, 1.0);
        Some(self.splash_inner + (self.splash_damage - self.splash_inner) * t)
    }

    /// Seconds from one shot to the next: the fire time, plus the rechamber
    /// when a bolt-action/pump gun has a round left to chamber.
    pub fn shot_cycle(&self, rounds_left: u32) -> f32 {
        let rechamber = if self.rechamber && rounds_left > 0 { self.rechamber_time } else { 0.0 };
        self.fire_interval.max(0.0) + rechamber
    }

    /// Rounds a reload can take now.
    pub fn can_reload(&self, clip: u32, reserve: u32) -> bool {
        reserve > 0 && clip < self.clip
    }

    /// Clip and reserve when the weapon is first given (the box, a wall):
    /// the scripts follow `GiveWeapon` with `GiveMaxAmmo`, so a full clip
    /// and a full reserve.
    pub fn full_ammo(&self) -> (u32, u32) {
        (self.clip, self.max_ammo)
    }

    /// Clip and reserve for a spawn loadout (`startAmmo` in total).
    pub fn start_ammo_split(&self) -> (u32, u32) {
        let total = if self.start_ammo > 0 { self.start_ammo } else { self.clip + self.max_ammo };
        let clip = self.clip.min(total);
        (clip, (total - clip).min(self.max_ammo))
    }

    /// Override stats from a weapon file in the user's install, or from a
    /// zone WeaponDef presented the same way. Returns how many fields were
    /// applied.
    pub fn apply_weapon_file(&mut self, wf: &WeaponFile) -> usize {
        let mut n = 0;
        // Numbers that must be positive vs. ones where 0 is meaningful.
        fn pos(n: &mut usize, dst: &mut f32, v: Option<f32>) {
            if let Some(v) = v.filter(|v| v.is_finite() && *v > 0.0) {
                *dst = v;
                *n += 1;
            }
        }
        fn nonneg(n: &mut usize, dst: &mut f32, v: Option<f32>) {
            if let Some(v) = v.filter(|v| v.is_finite() && *v >= 0.0) {
                *dst = v;
                *n += 1;
            }
        }
        fn count(n: &mut usize, dst: &mut u32, v: Option<u32>, min: u32) {
            if let Some(v) = v.filter(|v| *v >= min) {
                *dst = v;
                *n += 1;
            }
        }
        fn flag(n: &mut usize, dst: &mut bool, v: Option<&str>) {
            if let Some(v) = v {
                *dst = v.trim() == "1";
                *n += 1;
            }
        }
        let m = |k: &str| wf.f32(k).map(|u| u * UNITS_TO_M);

        pos(&mut n, &mut self.damage, wf.f32("damage"));
        nonneg(&mut n, &mut self.min_damage, wf.f32("minDamage"));
        pos(&mut n, &mut self.near, m("maxDamageRange"));
        pos(&mut n, &mut self.far, m("minDamageRange"));
        for loc in HitLoc::ALL {
            nonneg(&mut n, &mut self.loc_mult[loc.index()], wf.f32(loc.key()));
        }
        pos(&mut n, &mut self.melee_damage, wf.f32("meleeDamage"));
        pos(&mut n, &mut self.melee_time, wf.f32("meleeTime"));
        nonneg(&mut n, &mut self.melee_delay, wf.f32("meleeDelay"));
        if let Some(p) = wf.get("penetrateType").and_then(Penetrate::parse) {
            self.penetrate = p;
            self.flesh_penetration = FLESH_PENETRATION[p as usize];
            n += 1;
        }
        if let Some(mode) = wf.get("fireType").and_then(FireMode::parse) {
            self.mode = mode;
            n += 1;
        }
        pos(&mut n, &mut self.fire_interval, wf.f32("fireTime"));
        flag(&mut n, &mut self.rechamber, wf.get("boltAction"));
        nonneg(&mut n, &mut self.rechamber_time, wf.f32("rechamberTime"));
        count(&mut n, &mut self.clip, wf.u32("clipSize"), 1);
        count(&mut n, &mut self.max_ammo, wf.u32("maxAmmo"), 0);
        count(&mut n, &mut self.start_ammo, wf.u32("startAmmo"), 0);
        count(&mut n, &mut self.pellets, wf.u32("shotCount"), 1);

        let r = &mut self.reload;
        pos(&mut n, &mut r.time, wf.f32("reloadTime"));
        nonneg(&mut n, &mut r.empty_time, wf.f32("reloadEmptyTime"));
        nonneg(&mut n, &mut r.add_time, wf.f32("reloadAddTime"));
        nonneg(&mut n, &mut r.empty_add_time, wf.f32("reloadEmptyAddTime"));
        nonneg(&mut n, &mut r.start_time, wf.f32("reloadStartTime"));
        nonneg(&mut n, &mut r.start_add_time, wf.f32("reloadStartAddTime"));
        nonneg(&mut n, &mut r.end_time, wf.f32("reloadEndTime"));
        flag(&mut n, &mut r.segmented, wf.get("segmentedReload"));
        flag(&mut n, &mut r.no_partial, wf.get("noPartialReload"));
        count(&mut n, &mut r.ammo_add, wf.u32("reloadAmmoAdd"), 1);
        count(&mut n, &mut r.start_add, wf.u32("reloadStartAdd"), 0);

        let s = &mut self.spread;
        nonneg(&mut n, &mut s.stand.0, wf.f32("hipSpreadStandMin"));
        nonneg(&mut n, &mut s.stand.1, wf.f32("hipSpreadMax"));
        nonneg(&mut n, &mut s.ducked.0, wf.f32("hipSpreadDuckedMin"));
        nonneg(&mut n, &mut s.ducked.1, wf.f32("hipSpreadDuckedMax"));
        nonneg(&mut n, &mut s.prone.0, wf.f32("hipSpreadProneMin"));
        nonneg(&mut n, &mut s.prone.1, wf.f32("hipSpreadProneMax"));
        nonneg(&mut n, &mut s.ads, wf.f32("adsSpread"));
        nonneg(&mut n, &mut s.fire_add, wf.f32("hipSpreadFireAdd"));
        nonneg(&mut n, &mut s.move_add, wf.f32("hipSpreadMoveAdd"));
        nonneg(&mut n, &mut s.turn_add, wf.f32("hipSpreadTurnAdd"));
        nonneg(&mut n, &mut s.decay, wf.f32("hipSpreadDecayRate"));
        nonneg(&mut n, &mut s.ducked_decay, wf.f32("hipSpreadDuckedDecay"));
        nonneg(&mut n, &mut s.prone_decay, wf.f32("hipSpreadProneDecay"));

        pos(&mut n, &mut self.ads_in_time, wf.f32("adsTransInTime"));
        pos(&mut n, &mut self.ads_out_time, wf.f32("adsTransOutTime"));
        pos(&mut n, &mut self.ads_zoom_fov, wf.f32("adsZoomFov"));
        pos(&mut n, &mut self.move_speed_scale, wf.f32("moveSpeedScale"));
        pos(&mut n, &mut self.ads_move_speed_scale, wf.f32("adsMoveSpeedScale"));
        pos(&mut n, &mut self.sprint_duration_scale, wf.f32("sprintDurationScale"));
        pos(&mut n, &mut self.raise_time, wf.f32("raiseTime"));
        pos(&mut n, &mut self.drop_time, wf.f32("dropTime"));
        pos(&mut n, &mut self.first_raise_time, wf.f32("firstRaiseTime"));

        if let Some(r) = m("explosionRadius").filter(|r| *r > 0.0) {
            self.splash_radius = r;
            n += 1;
            nonneg(&mut n, &mut self.splash_inner, wf.f32("explosionInnerDamage"));
            nonneg(&mut n, &mut self.splash_damage, wf.f32("explosionOuterDamage"));
        }
        if wf.get("weaponType").is_some_and(|t| t.trim().eq_ignore_ascii_case("projectile")) {
            pos(&mut n, &mut self.projectile_speed, m("projectileSpeed"));
        }
        if let (Some(lo), Some(hi)) = (wf.f32("hipViewKickPitchMin"), wf.f32("hipViewKickPitchMax")) {
            let k = (lo.abs() + hi.abs()) * 0.5 / 40.0;
            if k > 0.0 {
                self.kick = k.clamp(0.3, 3.0);
                n += 1;
            }
        }

        if self.min_damage > self.damage {
            self.min_damage = self.damage;
        }
        if self.far < self.near {
            self.far = self.near;
        }
        n
    }
}

/// Index of the starting pistol in [`default_weapons`].
pub const START_PISTOL: usize = 0;

pub fn default_weapons() -> Vec<WeaponDef> {
    use crate::weapon_defaults as d;
    use Kind::*;
    vec![
        WeaponDef::new("m1911", "M1911", Pistol, "zombie_colt").with(d::ZOMBIE_COLT),
        WeaponDef::new("kar98k", "Kar98k", Rifle, "kar98k").with(d::KAR98K),
        WeaponDef::new("m1carbine", "M1A1 Carbine", Rifle, "m1carbine").with(d::M1CARBINE),
        WeaponDef::new("m1garand", "M1 Garand", Rifle, "m1garand").with(d::M1GARAND),
        WeaponDef::new("thompson", "Thompson", Smg, "thompson").with(d::THOMPSON),
        WeaponDef::new("mp40", "MP40", Smg, "mp40").with(d::MP40),
        WeaponDef::new("ppsh", "PPSh-41", Smg, "ppsh").with(d::PPSH),
        WeaponDef::new("stg44", "STG-44", Rifle, "stg44").with(d::STG44),
        WeaponDef::new("bar", "BAR", Lmg, "bar").with(d::BAR),
        WeaponDef::new("trenchgun", "Trench Gun", Shotgun, "shotgun").with(d::SHOTGUN),
        WeaponDef::new("doublebarrel", "Double-Barreled Shotgun", Shotgun, "doublebarrel").with(d::DOUBLEBARREL),
        // Variants found on Nacht der Untoten's walls and in its box.
        WeaponDef::new("doublebarrel_sawed_grip", "Sawed-Off Double-Barreled Shotgun", Shotgun, "doublebarrel_sawed_grip")
            .with(d::DOUBLEBARREL_SAWED_GRIP),
        WeaponDef::new("kar98k_scoped_zombie", "Scoped Kar98k", Rifle, "kar98k_scoped_zombie").with(d::KAR98K_SCOPED_ZOMBIE),
        WeaponDef::new("raypistol", "Ray Gun", Wonder, "ray_gun").with(d::RAY_GUN),
        WeaponDef::new("sw_357", ".357 Magnum", Pistol, "sw_357").with(d::SW_357),
        WeaponDef::new("gewehr43", "Gewehr 43", Rifle, "gewehr43").with(d::GEWEHR43),
        WeaponDef::new("springfield", "Springfield", Rifle, "springfield").with(d::SPRINGFIELD),
        WeaponDef::new("ptrs41_zombie", "PTRS-41", Rifle, "ptrs41_zombie").with(d::PTRS41_ZOMBIE),
        // The rifle grenade (its alt weapon, m7_launcher) is not modelled:
        // this is the Garand half.
        WeaponDef::new("m1garand_gl", "M1 Garand w/ Launcher", Rifle, "m1garand_gl").with(d::M1GARAND_GL),
        // Bipod LMGs, fired from the hip/sights (not deployed).
        WeaponDef::new("fg42_bipod", "FG42", Lmg, "fg42_bipod").with(d::FG42_BIPOD),
        WeaponDef::new("mg42_bipod", "MG42", Lmg, "mg42_bipod").with(d::MG42_BIPOD),
        WeaponDef::new("30cal_bipod", "Browning M1919", Lmg, "30cal_bipod").with(d::CAL30_BIPOD),
    ]
}

/// Looks a weapon up by id, also accepting the names maps use for them.
pub fn find(defs: &[WeaponDef], id: &str) -> Option<usize> {
    let id = match id {
        "shotgun" => "trenchgun",
        "colt" | "zombie_colt" => "m1911",
        "ray_gun" => "raypistol",
        other => other,
    };
    defs.iter().position(|d| d.id == id)
}

/// What Nacht der Untoten's box offers, by the game's weapon names: the
/// `include_weapons()` list of `nazi_zombie_prototype.gsc` that
/// `_zombiemode_weapons.gsc` registers (`m7_launcher` is included but never
/// added, so it is not here). The script picks uniformly among them. The
/// PPSh is not on Nacht (nor in its zone); it arrived with later maps.
pub const NACHT_CRATE: &[&str] = &[
    "sw_357",
    "m1carbine",
    "m1garand",
    "gewehr43",
    "stg44",
    "thompson",
    "mp40",
    "kar98k",
    "springfield",
    "ptrs41_zombie",
    "kar98k_scoped_zombie",
    "molotov",
    "stielhandgranate",
    "m1garand_gl",
    "m2_flamethrower_zombie",
    "doublebarrel",
    "doublebarrel_sawed_grip",
    "shotgun",
    "fg42_bipod",
    "mg42_bipod",
    "30cal_bipod",
    "bar",
    "panzerschrek",
    "ray_gun",
];

/// Weapons the mystery crate can roll, with relative weights. With a map's
/// list (`only`), its weapons we have, each equally likely as in the map's
/// script; without one, every weapon but the starting pistol, the wonder
/// weapon rarer.
pub fn crate_pool(defs: &[WeaponDef], only: Option<&[&str]>) -> Vec<(usize, u32)> {
    match only {
        Some(names) => {
            let mut pool: Vec<(usize, u32)> = names.iter().filter_map(|n| find(defs, n)).filter(|i| *i != START_PISTOL).map(|i| (i, 1)).collect();
            pool.sort_unstable();
            pool.dedup();
            pool
        }
        None => defs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != START_PISTOL)
            .map(|(i, d)| (i, if d.kind == Kind::Wonder { 2 } else { 10 }))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str) -> WeaponDef {
        let defs = default_weapons();
        defs[find(&defs, id).unwrap()].clone()
    }

    #[test]
    fn map_aliases() {
        let defs = default_weapons();
        assert_eq!(defs[find(&defs, "shotgun").unwrap()].id, "trenchgun");
        // Only bolt-action and pump guns cycle after a shot.
        let rechambers = |id: &str| defs[find(&defs, id).unwrap()].rechamber;
        assert!(rechambers("kar98k") && rechambers("trenchgun") && rechambers("springfield"));
        assert!(!rechambers("doublebarrel_sawed_grip") && !rechambers("thompson") && !rechambers("ptrs41_zombie"));
        for id in ["kar98k", "thompson", "bar", "m1carbine", "doublebarrel", "kar98k_scoped_zombie", "doublebarrel_sawed_grip", "sw_357", "30cal_bipod"] {
            assert!(find(&defs, id).is_some(), "{id}");
        }
        assert!(find(&defs, "stielhandgranate").is_none());
        let mut ids: Vec<&str> = defs.iter().map(|d| d.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), defs.len(), "ids are unique");
    }

    #[test]
    fn falloff() {
        let d = def("thompson");
        assert_eq!(d.damage_at(0.0), d.damage);
        assert_eq!(d.damage_at(1000.0), d.min_damage);
        let mid = d.damage_at((d.near + d.far) / 2.0);
        assert!((mid - (d.damage + d.min_damage) / 2.0).abs() < 1e-3);
        // Thompson: 120 to 800 units, 80 from 1800.
        assert_eq!(d.damage_at(800.0 * UNITS_TO_M - 0.01), 120.0);
        assert_eq!(d.damage_at(1800.0 * UNITS_TO_M + 0.01), 80.0);
        // No falloff without ranges (the Ray Gun's file has none).
        let r = def("raypistol");
        assert_eq!(r.damage_at(500.0), r.damage);
    }

    #[test]
    fn location_multipliers() {
        let k = def("kar98k");
        // Kar98k: head and neck 3.5, helmet 1, upper torso 1.8.
        assert_eq!(k.bullet_damage(1.0, HitLoc::Head), 350.0);
        assert_eq!(k.bullet_damage(1.0, HitLoc::Helmet), 100.0);
        assert_eq!(k.bullet_damage(1.0, HitLoc::TorsoUpper), 180.0);
        assert_eq!(k.bullet_damage(1.0, HitLoc::Gun), 0.0);
        let t = def("thompson");
        // Thompson: head 4, neck 1.
        assert_eq!(t.bullet_damage(1.0, HitLoc::Head), 480.0);
        assert_eq!(t.bullet_damage(1.0, HitLoc::Neck), 120.0);
        // The colt's feet: 0.35.
        let c = def("m1911");
        assert!((c.bullet_damage(1.0, HitLoc::LeftFoot) - 7.0).abs() < 1e-4);
        // Projectile impacts ignore the location.
        assert_eq!(def("raypistol").bullet_damage(3.0, HitLoc::LeftFoot), 1000.0);
        assert!(HitLoc::Helmet.is_head() && !HitLoc::Neck.is_head() && HitLoc::Neck.gibs_head());
        for (i, l) in HitLoc::ALL.iter().enumerate() {
            assert_eq!(l.index(), i);
        }
    }

    #[test]
    fn shotgun_pellets() {
        let s = def("trenchgun");
        assert_eq!(s.pellets, 8);
        assert_eq!(s.damage_at(2.0), 160.0);
        assert_eq!(s.damage_at(30.0), 15.0);
        // Pellets stop at the minimum-damage range (800 units).
        assert!((s.bullet_range(100.0) - 800.0 * UNITS_TO_M).abs() < 1e-3);
        assert_eq!(def("thompson").bullet_range(100.0), 100.0);
        assert!(s.reload.segmented && s.reload.start_add == 1 && s.reload.ammo_add == 1);
    }

    #[test]
    fn fire_cycles() {
        let k = def("kar98k");
        // fireTime 0.33 + rechamberTime 1.0; no rechamber on the last round.
        assert!((k.shot_cycle(4) - 1.33).abs() < 1e-4);
        assert!((k.shot_cycle(0) - 0.33).abs() < 1e-4);
        let t = def("thompson");
        assert_eq!(t.mode, FireMode::Auto);
        assert!((t.shot_cycle(10) - 0.08).abs() < 1e-6);
        assert_eq!(def("raypistol").mode, FireMode::Auto);
        assert_eq!(def("m1garand").mode, FireMode::Semi);
        assert_eq!(FireMode::parse("3-Round Burst"), Some(FireMode::Burst(3)));
    }

    #[test]
    fn reload_add_times() {
        let mp40 = def("mp40");
        assert_eq!(mp40.reload.duration(false), 2.3);
        assert_eq!(mp40.reload.duration(true), 2.9);
        assert_eq!(mp40.reload.ammo_in_at(false), 1.85);
        assert_eq!(mp40.reload.ammo_in_at(true), 1.85);
        // The Garand's empty reload (1.6 s) has no add time of its own and
        // the normal one (2.5) is past its end: at the end.
        let g = def("m1garand");
        assert_eq!(g.reload.ammo_in_at(true), 1.6);
        assert_eq!(g.reload.ammo_in_at(false), 2.5);
        // No empty add time but a normal one that fits: that one.
        assert_eq!(def("doublebarrel").reload.ammo_in_at(true), 2.65);
        // An add time past the end is the end (the .357: 3.5 of 3).
        assert_eq!(def("sw_357").reload.ammo_in_at(false), 3.0);
    }

    #[test]
    fn ammo() {
        let c = def("m1911");
        assert_eq!(c.start_ammo_split(), (8, 32));
        assert_eq!(c.full_ammo(), (8, 80));
        assert_eq!(def("thompson").full_ammo(), (20, 200));
    }

    #[test]
    fn spread_model() {
        let s = def("thompson").spread;
        assert_eq!(s.cone(SpreadStance::Stand, 0.0, 0.0), 1.5);
        assert_eq!(s.cone(SpreadStance::Stand, 1.0, 0.0), 6.0);
        assert_eq!(s.cone(SpreadStance::Stand, 1.0, 1.0), 0.0);
        let up = s.update_scale(0.0, SpreadStance::Stand, 1.0, 0.0, 0.1);
        assert!((up - 0.4).abs() < 1e-5);
        let down = s.update_scale(1.0, SpreadStance::Stand, 0.0, 0.0, 0.1);
        assert!((down - 0.6).abs() < 1e-5);
    }

    #[test]
    fn splash() {
        let r = def("raypistol");
        assert_eq!(r.splash_at(0.0), Some(1500.0));
        assert!((r.splash_at(r.splash_radius).unwrap() - 300.0).abs() < 1e-3);
        assert_eq!(r.splash_at(r.splash_radius + 0.1), None);
        assert!((r.splash_radius - 64.0 * UNITS_TO_M).abs() < 1e-4);
    }

    #[test]
    fn weapon_file_override() {
        let mut d = default_weapons().remove(START_PISTOL);
        let wf = WeaponFile::parse(
            "WEAPONFILE\\damage\\21\\minDamage\\20\\fireTime\\0.075\\clipSize\\8\\maxAmmo\\80\\fireType\\Single Shot\\maxDamageRange\\425\\minDamageRange\\1000\\locHead\\5\\reloadAddTime\\0",
        )
        .unwrap();
        assert!(d.apply_weapon_file(&wf) >= 10);
        assert_eq!(d.damage, 21.0);
        assert_eq!(d.mode, FireMode::Semi);
        assert!((d.near - 10.795).abs() < 0.01);
        assert_eq!(d.location_multiplier(HitLoc::Head), 5.0);
        assert_eq!(d.reload.add_time, 0.0);
    }

    #[test]
    fn penetration_table() {
        let t = parse_penetration_table("BULLET_PEN_TABLE\\small_bark\\0\\small_flesh\\32\\medium_flesh\\96\\large_flesh\\128\\large_wood\\192").unwrap();
        assert_eq!(t, FLESH_PENETRATION);
        assert!(parse_penetration_table("WEAPONFILE\\x\\1").is_none());
        assert_eq!(def("ptrs41_zombie").flesh_penetration, 128.0);
        assert_eq!(def("mp40").flesh_penetration, 32.0);
    }

    #[test]
    fn crate_pool_excludes_start_pistol() {
        let defs = default_weapons();
        let pool = crate_pool(&defs, None);
        assert!(pool.iter().all(|(i, _)| *i != START_PISTOL));
        assert!(find(&defs, "raypistol").is_some());
    }

    #[test]
    fn nacht_crate_matches_its_script() {
        let defs = default_weapons();
        let pool = crate_pool(&defs, Some(NACHT_CRATE));
        let ids: Vec<&str> = pool.iter().map(|(i, _)| defs[*i].id).collect();
        // Not on Nacht: the PPSh (later maps) and the starting pistol.
        assert!(!ids.contains(&"ppsh") && !ids.contains(&"m1911"));
        for id in [
            "raypistol",
            "trenchgun",
            "mp40",
            "thompson",
            "doublebarrel_sawed_grip",
            "kar98k_scoped_zombie",
            "sw_357",
            "gewehr43",
            "springfield",
            "ptrs41_zombie",
            "m1garand_gl",
            "fg42_bipod",
            "mg42_bipod",
            "30cal_bipod",
        ] {
            assert!(ids.contains(&id), "{id}");
        }
        assert!(pool.iter().all(|(_, w)| *w == 1));
    }
}
