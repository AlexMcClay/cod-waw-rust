//! Round, zombie, scoring and power-up rules of World at War's zombie mode.
//!
//! Every number here is taken from the game's own scripts and data (the
//! research note `research/gameplay/ZOMBIE_MECHANICS.md` gives the source of
//! each one). The per-map values live in [`ZombieRules`]; [`ZombieRules::nacht`]
//! holds Nacht der Untoten's. At runtime the map's `set_zombie_var` calls and
//! `mp/zombiemode.csv` overrides can be applied on top with
//! [`ZombieRules::apply_vars`] (see [`parse_zombie_vars`] and
//! [`resolve_zombie_vars`]), the way the game's `set_zombie_var` does.
//!
//! The code is written from the behaviour, not translated: the GSC logic is
//! reproduced where it matters for the numbers (integer truncation, f32
//! maths, the order of updates between rounds).

use std::collections::HashMap;

/// Game units (inches) to metres.
pub const INCH: f32 = 0.0254;

// ---------------------------------------------------------------------------
// Constants kept for the rest of the game (Nacht values).

/// Player health without perks (`self.maxhealth`, code default 100).
pub const PLAYER_MAX_HEALTH: f32 = 100.0;
/// One zombie hit on the player: the zombie AI's weapon (`kar98k`,
/// `iMeleeDamage` 150) scaled by `player_meleeDamageMultiplier` = 100/250.
pub const ZOMBIE_HIT_DAMAGE: f32 = 60.0;
/// Delay before health comes back after a hit (Regular difficulty).
pub const REGEN_DELAY: f32 = 2.4;
/// Kept for callers that regenerate linearly: full health in 0.5 s.
pub const REGEN_PER_SEC: f32 = 200.0;
/// `SetAILimit( 24 )`: zombies alive at once.
pub const MAX_ALIVE: usize = 24;
/// Boards on the procedural bunker's windows (real maps read their own).
pub const BOARDS_PER_WINDOW: u8 = 6;
/// `zombie_between_round_time`.
pub const INTERMISSION_SECS: f32 = 10.0;

pub const POINTS_START: u32 = 500;
/// A non-lethal hit: `zombie_score_damage` 5, rounded up to 10.
pub const POINTS_HIT: u32 = 10;
pub const POINTS_KILL: u32 = 50;
pub const POINTS_HEADSHOT_KILL: u32 = 100;
pub const POINTS_MELEE_KILL: u32 = 130;
/// A repaired board (doubled while Double Points is on).
pub const POINTS_BOARD: u32 = 10;
/// The nuke's reward on later maps; Nacht's nuke gives nothing
/// (see [`ZombieRules::nuke_points`]).
pub const POINTS_NUKE: u32 = 400;
/// Carpenter's reward (Der Riese onwards; Nacht has no carpenter).
pub const POINTS_CARPENTER: u32 = 200;
pub const CRATE_COST: u32 = 950;
/// Double Points / Insta-Kill length.
pub const POWERUP_DURATION: f32 = 30.0;
/// How long a power-up stays on the ground: 15 s, then 11.5 s of blinking.
pub const POWERUP_TTL: f32 = 26.5;
pub const MAX_DROPS_PER_ROUND: u32 = 4;
/// Chance that a kill drops a power-up without the score trigger (3 in 100).
pub const DROP_CHANCE: f32 = 0.03;

// ---------------------------------------------------------------------------
// Per-map rules.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gait {
    Walk,
    Run,
    Sprint,
}

impl Gait {
    /// Ground speed in m/s with Nacht's animations (their root motion).
    pub fn speed(self) -> f32 {
        ZombieRules::nacht().gait_speed(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Powerup {
    MaxAmmo,
    InstaKill,
    DoublePoints,
    Nuke,
    Carpenter,
}

impl Powerup {
    pub const ALL: [Powerup; 5] = [Powerup::MaxAmmo, Powerup::InstaKill, Powerup::DoublePoints, Powerup::Nuke, Powerup::Carpenter];

    pub fn label(self) -> &'static str {
        match self {
            Powerup::MaxAmmo => "Max Ammo",
            Powerup::InstaKill => "Insta-Kill",
            Powerup::DoublePoints => "Double Points",
            Powerup::Nuke => "Nuke",
            Powerup::Carpenter => "Carpenter",
        }
    }

    /// The script name used by `include_powerup`.
    pub fn script_name(self) -> &'static str {
        match self {
            Powerup::MaxAmmo => "full_ammo",
            Powerup::InstaKill => "insta_kill",
            Powerup::DoublePoints => "double_points",
            Powerup::Nuke => "nuke",
            Powerup::Carpenter => "carpenter",
        }
    }
}

/// One zombie animation that matters for timing: its length and the
/// moments (seconds) of its gameplay notetracks (`fire` = a hit on the
/// player, `board` = a board comes off).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedAnim {
    pub name: &'static str,
    pub len: f32,
    pub events: &'static [f32],
}

/// The zombie melee anims (`level._zombie_melee`, one picked at random per
/// swing) with their `fire` notetracks (Nacht zone, 30 fps).
pub const NACHT_ATTACKS: [TimedAnim; 4] = [
    TimedAnim { name: "ai_zombie_attack_forward_v1", len: 70.0 / 30.0, events: &[29.0 / 30.0, 42.0 / 30.0] },
    TimedAnim { name: "ai_zombie_attack_forward_v2", len: 100.0 / 30.0, events: &[26.0 / 30.0, 42.0 / 30.0] },
    TimedAnim { name: "ai_zombie_attack_v1", len: 54.0 / 30.0, events: &[25.0 / 30.0, 39.0 / 30.0] },
    TimedAnim {
        name: "ai_zombie_attack_v2",
        len: 215.0 / 30.0,
        events: &[21.0 / 30.0, 70.0 / 30.0, 115.0 / 30.0, 134.0 / 30.0, 160.0 / 30.0],
    },
];

/// The board-pulling anims (one board each, `board` notetrack). The game
/// picks by board height (high above 70 units over the zombie's feet, low
/// under 40, else left/right at random).
pub const NACHT_TEARS: [TimedAnim; 4] = [
    TimedAnim { name: "ai_zombie_door_tear_high", len: 78.0 / 30.0, events: &[35.0 / 30.0] },
    TimedAnim { name: "ai_zombie_door_tear_left", len: 72.0 / 30.0, events: &[38.0 / 30.0] },
    TimedAnim { name: "ai_zombie_door_tear_right", len: 91.0 / 30.0, events: &[38.0 / 30.0] },
    TimedAnim { name: "ai_zombie_door_tear_low", len: 68.0 / 30.0, events: &[33.0 / 30.0] },
];

/// Everything that differs between zombie maps, with Nacht der Untoten's
/// values in [`ZombieRules::nacht`].
#[derive(Debug, Clone, PartialEq)]
pub struct ZombieRules {
    // --- round flow ---
    /// `zombie_between_round_time`: last kill to next round.
    pub between_round_time: f32,
    /// Round 1: from the round start to the first spawn (the intro chalk).
    pub first_round_spawn_wait: f32,
    /// Later rounds: from the round start to the first spawn (chalk fade).
    pub round_spawn_wait: f32,
    /// `zombie_spawn_delay` in round 1, its per-round factor and the floor
    /// it is clamped to before the factor is applied.
    pub spawn_delay_start: f32,
    pub spawn_delay_factor: f32,
    pub spawn_delay_floor: f32,
    /// `zombie_max_ai`: zombies per round before the per-player extra.
    pub max_ai: u32,
    /// `SetAILimit`: zombies alive at once.
    pub ai_limit: u32,
    /// `zombie_ai_per_player`.
    pub ai_per_player: u32,
    /// "Players" counted for the extra zombies when alone: 0 (only the
    /// others count) on Nacht to Shi No Numa, 0.5 on Der Riese.
    pub solo_player_factor: f32,
    /// Count multipliers for rounds 1, 2, 3 and 4.
    pub early_round_factor: [f32; 4],

    // --- zombie health ---
    pub health_start: i32,
    /// Added per round from round 2 up to `health_percent_from - 1`.
    pub health_increase: i32,
    /// Then `health += int(health * percent)` per round.
    pub health_increase_percent: f32,
    pub health_percent_from: u32,

    // --- movement ---
    /// `level.zombie_move_speed` in round 1, then `(round - 1) * step`.
    pub move_speed_start: i32,
    pub move_speed_step: i32,
    /// Each zombie rolls `RandomIntRange(speed, speed + spread)`.
    pub move_speed_spread: i32,
    /// Rolls up to these are walkers / runners; above, sprinters.
    pub walk_max: i32,
    pub run_max: i32,
    /// Ground speeds (m/s) of the walk / run / sprint anims.
    pub walk_speed: f32,
    pub run_speed: f32,
    pub sprint_speed: f32,

    // --- the player ---
    pub player_health: f32,
    pub zombie_hit_damage: f32,
    /// Zombies reach this far (centre to centre) with a swing, and start
    /// swinging at this distance.
    pub melee_range: f32,
    /// Zombies can hit through a window they are tearing (Verrückt on).
    pub attack_through_windows: bool,
    /// Regen (`_gameskill`, Regular): no regen for `regen_delay` after a
    /// hit; then full health at once, unless health is at or below
    /// `very_hurt_ratio`, in which case it waits `long_regen_time` after
    /// the hit and then climbs `regen_rate` (fraction of max) per second.
    pub regen_delay: f32,
    pub very_hurt_ratio: f32,
    pub long_regen_time: f32,
    pub regen_rate: f32,
    /// Invulnerability after a hit worth at least `worthy_damage_ratio` of
    /// max health: before the red overlay, when it first appears, after.
    pub invul_pre_shield: f32,
    pub invul_on_shield: f32,
    pub invul_post_shield: f32,
    pub worthy_damage_ratio: f32,

    // --- points ---
    pub score_start: u32,
    pub score_kill: u32,
    pub score_damage: u32,
    pub bonus_melee: u32,
    pub bonus_head: u32,
    pub bonus_neck: u32,
    pub bonus_torso: u32,
    pub bonus_burn: u32,
    /// Fire damage gives hit points at most this often (s).
    pub flame_point_delay: f32,
    /// Points per repaired board (doubled under Double Points).
    pub repair_points: u32,
    /// Repair reward cap per player and round: `min(step * round, max)`.
    pub repair_cap_step: u32,
    pub repair_cap_max: u32,
    /// Holding use: the first board after this, then one per interval.
    pub repair_first_delay: f32,
    pub repair_interval: f32,
    pub nuke_points: u32,
    pub carpenter_points: u32,
    /// Insta-kill kills give `damage + kill` points with no bonus.
    pub insta_kill_flat_points: bool,
    /// A second Double Points while one runs doubles the doubling.
    pub double_points_stack: bool,

    // --- power-ups ---
    pub powerups: Vec<Powerup>,
    pub powerup_time: f32,
    pub drop_increment: f32,
    pub drop_increment_factor: f32,
    pub drop_max_per_round: u32,
    /// `RandomInt(100)` at or below this drops without the score trigger.
    pub drop_random_max: u32,
    /// Seconds on the ground before blinking.
    pub powerup_solid_time: f32,
    /// Pick-up distance (feet to the power-up, which floats 40 units up).
    pub powerup_grab_radius: f32,
    pub powerup_height: f32,

    // --- tearing and attacking ---
    pub attacks: &'static [TimedAnim],
    pub tears: &'static [TimedAnim],
}

impl Default for ZombieRules {
    fn default() -> Self {
        Self::nacht()
    }
}

impl ZombieRules {
    /// Nacht der Untoten (`patch.ff` `maps/_zombiemode_prototype.gsc` and
    /// friends; see the research note for line numbers).
    pub fn nacht() -> Self {
        Self {
            between_round_time: 10.0,
            first_round_spawn_wait: 6.75,
            round_spawn_wait: 0.5,
            spawn_delay_start: 3.0,
            spawn_delay_factor: 0.95,
            spawn_delay_floor: 0.08,
            max_ai: 24,
            ai_limit: 24,
            ai_per_player: 6,
            solo_player_factor: 0.0,
            early_round_factor: [0.2, 0.4, 0.6, 0.8],

            health_start: 150,
            health_increase: 100,
            health_increase_percent: 0.1,
            health_percent_from: 10,

            move_speed_start: 1,
            move_speed_step: 8,
            move_speed_spread: 35,
            walk_max: 35,
            run_max: 70,
            // Root motion of ai_zombie_walk_v1..4 (38/39/37/48 u/s),
            // walk_fast_v1..3 (65/81/78), sprint_v1/v2 (142/139).
            walk_speed: 40.5 * INCH,
            run_speed: 74.7 * INCH,
            sprint_speed: 140.5 * INCH,

            player_health: PLAYER_MAX_HEALTH,
            zombie_hit_damage: ZOMBIE_HIT_DAMAGE,
            melee_range: 64.0 * INCH,
            attack_through_windows: false,
            regen_delay: 2.4,
            very_hurt_ratio: 0.2,
            long_regen_time: 5.0,
            regen_rate: 2.0,
            invul_pre_shield: 0.35,
            invul_on_shield: 0.5,
            invul_post_shield: 0.3,
            worthy_damage_ratio: 0.1,

            score_start: 500,
            score_kill: 50,
            score_damage: 5,
            bonus_melee: 80,
            bonus_head: 50,
            bonus_neck: 20,
            bonus_torso: 10,
            bonus_burn: 10,
            flame_point_delay: 0.5,
            repair_points: 10,
            repair_cap_step: 50,
            repair_cap_max: 500,
            repair_first_delay: 0.4,
            repair_interval: 1.0,
            nuke_points: 0,
            carpenter_points: 0,
            insta_kill_flat_points: true,
            double_points_stack: true,

            powerups: vec![Powerup::Nuke, Powerup::InstaKill, Powerup::DoublePoints, Powerup::MaxAmmo],
            powerup_time: POWERUP_DURATION,
            drop_increment: 2000.0,
            drop_increment_factor: 1.14,
            drop_max_per_round: MAX_DROPS_PER_ROUND,
            drop_random_max: 2,
            powerup_solid_time: 15.0,
            powerup_grab_radius: 64.0 * INCH,
            powerup_height: 40.0 * INCH,

            attacks: &NACHT_ATTACKS,
            tears: &NACHT_TEARS,
        }
    }

    /// Applies `level.zombie_vars` values by name (as resolved by
    /// [`resolve_zombie_vars`]). Unknown names are ignored.
    pub fn apply_vars(&mut self, vars: &HashMap<String, f32>) {
        let get = |k: &str| vars.get(k).copied();
        let u = |v: f32| v.max(0.0).round() as u32;
        if let Some(v) = get("zombie_between_round_time") {
            self.between_round_time = v;
        }
        if let Some(v) = get("zombie_spawn_delay") {
            self.spawn_delay_start = v;
        }
        if let Some(v) = get("zombie_health_increase") {
            self.health_increase = v as i32;
        }
        if let Some(v) = get("zombie_health_increase_percent") {
            self.health_increase_percent = v;
        }
        if let Some(v) = get("zombie_health_start") {
            self.health_start = v as i32;
        }
        if let Some(v) = get("zombie_max_ai") {
            self.max_ai = u(v);
        }
        if let Some(v) = get("zombie_ai_per_player") {
            self.ai_per_player = u(v);
        }
        if let Some(v) = get("zombie_score_start") {
            self.score_start = u(v);
        }
        if let Some(v) = get("zombie_score_kill") {
            self.score_kill = u(v);
        }
        if let Some(v) = get("zombie_score_damage") {
            self.score_damage = u(v);
        }
        if let Some(v) = get("zombie_score_bonus_melee") {
            self.bonus_melee = u(v);
        }
        if let Some(v) = get("zombie_score_bonus_head") {
            self.bonus_head = u(v);
        }
        if let Some(v) = get("zombie_score_bonus_neck") {
            self.bonus_neck = u(v);
        }
        if let Some(v) = get("zombie_score_bonus_torso") {
            self.bonus_torso = u(v);
        }
        if let Some(v) = get("zombie_score_bonus_burn") {
            self.bonus_burn = u(v);
        }
        if let Some(v) = get("zombie_flame_dmg_point_delay") {
            self.flame_point_delay = v / 1000.0;
        }
        if let Some(v) = get("zombie_powerup_drop_increment") {
            self.drop_increment = v;
        }
        if let Some(v) = get("zombie_powerup_drop_max_per_round") {
            self.drop_max_per_round = u(v);
        }
        if let Some(v) = get("zombie_powerup_point_doubler_time") {
            self.powerup_time = v;
        }
    }

    /// `level.zombie_health` in a round (1-based), with the game's integer
    /// maths. Past round 162 the 32-bit value wraps negative; the game then
    /// behaves as if zombies had round 1 health (CoD wiki), which is what
    /// this returns.
    pub fn zombie_health(&self, round: u32) -> i32 {
        let raw = self.zombie_health_raw(round);
        if raw <= 0 {
            self.health_start
        } else {
            raw
        }
    }

    /// The raw (possibly wrapped) `level.zombie_health`.
    pub fn zombie_health_raw(&self, round: u32) -> i32 {
        let mut h = self.health_start;
        for r in 2..=round.max(1) {
            h = self.next_health(h, r);
        }
        h
    }

    /// `ai_calculate_health` for the round that is starting.
    pub fn next_health(&self, h: i32, round: u32) -> i32 {
        if round >= self.health_percent_from {
            // int * float is f32 in the script VM; Int() truncates.
            let add = (h as f32 * self.health_increase_percent) as i32;
            h.wrapping_add(add)
        } else if round > 1 {
            h.wrapping_add(self.health_increase)
        } else {
            h
        }
    }

    /// Zombies spawned in a round for `players` players.
    pub fn zombies_in_round(&self, round: u32, players: u32) -> u32 {
        let round = round.max(1);
        let mut multiplier = (round as f32 / 5.0).max(1.0);
        if round >= 10 {
            multiplier *= round as f32 * 0.15;
        }
        let others = if players <= 1 { self.solo_player_factor } else { (players - 1) as f32 };
        let mut max = self.max_ai as i32 + (others * self.ai_per_player as f32 * multiplier) as i32;
        if round <= 4 {
            max = (max as f32 * self.early_round_factor[round as usize - 1]) as i32;
        }
        max.max(0) as u32
    }

    /// `zombie_spawn_delay` during a round: shrunk by 5% after each round,
    /// clamped up to the floor before shrinking.
    pub fn spawn_delay(&self, round: u32) -> f32 {
        let mut d = self.spawn_delay_start;
        for _ in 1..round.max(1) {
            d = self.next_spawn_delay(d);
        }
        d
    }

    pub fn next_spawn_delay(&self, d: f32) -> f32 {
        d.max(self.spawn_delay_floor) * self.spawn_delay_factor
    }

    /// `level.zombie_move_speed` during a round.
    pub fn move_speed(&self, round: u32) -> i32 {
        if round <= 1 {
            self.move_speed_start
        } else {
            (round as i32 - 1) * self.move_speed_step
        }
    }

    /// `set_run_speed` for a zombie spawned this round, from a uniform roll
    /// in [0, 1).
    pub fn pick_gait(&self, round: u32, roll: f32) -> Gait {
        let base = self.move_speed(round);
        let spread = self.move_speed_spread.max(1);
        let rand = base + ((roll.clamp(0.0, 0.999_999) * spread as f32) as i32).min(spread - 1);
        if rand <= self.walk_max {
            Gait::Walk
        } else if rand <= self.run_max {
            Gait::Run
        } else {
            Gait::Sprint
        }
    }

    /// Probabilities of walk / run / sprint in a round.
    pub fn gait_odds(&self, round: u32) -> [f32; 3] {
        let base = self.move_speed(round);
        let spread = self.move_speed_spread.max(1);
        let mut n = [0u32; 3];
        for rand in base..base + spread {
            let i = if rand <= self.walk_max { 0 } else if rand <= self.run_max { 1 } else { 2 };
            n[i] += 1;
        }
        n.map(|c| c as f32 / spread as f32)
    }

    pub fn gait_speed(&self, g: Gait) -> f32 {
        match g {
            Gait::Walk => self.walk_speed,
            Gait::Run => self.run_speed,
            Gait::Sprint => self.sprint_speed,
        }
    }

    /// The repair reward cap for a round.
    pub fn repair_cap(&self, round: u32) -> u32 {
        (self.repair_cap_step * round.max(1)).min(self.repair_cap_max)
    }

    /// Points for one lethal hit (before the Double Points scalar).
    pub fn kill_points(&self, kind: KillKind) -> u32 {
        let bonus = match kind {
            KillKind::Melee => self.bonus_melee,
            KillKind::Head => self.bonus_head,
            KillKind::Neck => self.bonus_neck,
            KillKind::Torso => self.bonus_torso,
            KillKind::Burn => self.bonus_burn,
            KillKind::Body | KillKind::Explosive => 0,
        };
        round_up_to_ten(self.score_kill + bonus)
    }

    /// Points for a non-lethal hit (hip or ADS, both round up to 10).
    pub fn hit_points(&self) -> u32 {
        round_up_to_ten(self.score_damage)
    }

    /// Points for a hit while Insta-Kill is on. A hit that would have
    /// killed anyway scores as a normal kill; otherwise the damage script
    /// awards the hit and Insta-Kill's own kill (no location bonus).
    pub fn insta_kill_points(&self, kind: KillKind, lethal_anyway: bool) -> u32 {
        if lethal_anyway || !self.insta_kill_flat_points {
            self.kill_points(kind)
        } else {
            self.hit_points() + round_up_to_ten(self.score_kill)
        }
    }

    /// Whether the power-up is drawn `age` seconds after it dropped
    /// (`powerup_timeout`: 15 s solid, then 40 hide/show steps of
    /// 0.5 s ×15, 0.25 s ×10, 0.1 s ×15). `None` once it is gone.
    pub fn powerup_visible(&self, age: f32) -> Option<bool> {
        if age < self.powerup_solid_time {
            return Some(true);
        }
        let mut t = self.powerup_solid_time;
        for i in 0..40 {
            let step = if i < 15 {
                0.5
            } else if i < 25 {
                0.25
            } else {
                0.1
            };
            if age < t + step {
                return Some(i % 2 == 0);
            }
            t += step;
        }
        None
    }

    /// Total time a power-up stays (15 + 11.5 s).
    pub fn powerup_ttl(&self) -> f32 {
        self.powerup_solid_time + 15.0 * 0.5 + 10.0 * 0.25 + 15.0 * 0.1
    }

    /// Horizontal pick-up radius for a player standing on the power-up's
    /// floor (the check is 3D from the feet to the floating model).
    pub fn powerup_grab_flat(&self) -> f32 {
        (self.powerup_grab_radius.powi(2) - self.powerup_height.powi(2)).max(0.0).sqrt()
    }
}

/// `round_up_to_ten`.
pub fn round_up_to_ten(score: u32) -> u32 {
    score.div_ceil(10) * 10
}

// ---------------------------------------------------------------------------
// Old free-function API (Nacht values).

/// Zombie hit points for a given round (1-based), Nacht.
pub fn zombie_health(round: u32) -> f32 {
    ZombieRules::nacht().zombie_health(round) as f32
}

/// Zombies spawned in a round for a solo player, Nacht.
pub fn zombies_in_round(round: u32) -> u32 {
    ZombieRules::nacht().zombies_in_round(round, 1)
}

/// Seconds between spawns, Nacht.
pub fn spawn_delay(round: u32) -> f32 {
    ZombieRules::nacht().spawn_delay(round)
}

/// Pick a gait from a uniform roll in [0,1), Nacht.
pub fn pick_gait(round: u32, roll: f32) -> Gait {
    ZombieRules::nacht().pick_gait(round, roll)
}

/// How the killing hit landed, for the kill bonus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillKind {
    /// Limbs or no hit location: no bonus.
    Body,
    /// `torso_upper` / `torso_lower`: +10.
    Torso,
    /// `neck`: +20.
    Neck,
    /// `head` / `helmet`: +50.
    Head,
    /// `MOD_MELEE`: +80.
    Melee,
    /// Grenade / projectile splash (location "none"): no bonus.
    Explosive,
    /// `MOD_BURNED`: +10.
    Burn,
}

/// Where a body hit landed, from its height above the feet (metres, for a
/// zombie of scale 1): legs up to the hips, torso to the shoulders, then the
/// neck below the head sphere.
pub fn body_location(height: f32, scale: f32) -> KillKind {
    let h = height / scale.max(0.1);
    if h < 0.86 {
        KillKind::Body
    } else if h < 1.40 {
        KillKind::Torso
    } else {
        KillKind::Neck
    }
}

/// Points for killing a zombie (excluding the per-hit points), Nacht.
pub fn kill_points(kind: KillKind) -> u32 {
    ZombieRules::nacht().kill_points(kind)
}

/// Apply the double-points multiplier.
pub fn scaled(points: u32, double: bool) -> u32 {
    if double {
        points * 2
    } else {
        points
    }
}

// ---------------------------------------------------------------------------
// The player's health.

/// The player's health with the single-player regeneration of `_gameskill`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerHealth {
    pub hp: f32,
    pub max: f32,
    /// Seconds since the last hit that hurt.
    pub since_hit: f32,
    /// Invulnerable for this long.
    pub invulnerable: f32,
    /// The red "very hurt" state.
    pub very_hurt: bool,
}

impl PlayerHealth {
    pub fn new(max: f32) -> Self {
        Self { hp: max, max, since_hit: 99.0, invulnerable: 0.0, very_hurt: false }
    }

    /// Takes a hit; returns true if it would down the player (damage at
    /// least the remaining health: zombie mode's game over when alone).
    pub fn hit(&mut self, rules: &ZombieRules, damage: f32) -> bool {
        if self.invulnerable > 0.0 || damage <= 0.0 {
            return false;
        }
        if damage >= self.hp {
            self.hp = 0.0;
            return true;
        }
        let before = self.hp / self.max;
        self.hp -= damage;
        self.since_hit = 0.0;
        let ratio = self.hp / self.max;
        let was_very_hurt = self.very_hurt;
        if ratio <= rules.very_hurt_ratio {
            self.very_hurt = true;
        }
        if before - ratio > rules.worthy_damage_ratio {
            self.invulnerable = if self.very_hurt && !was_very_hurt {
                rules.invul_on_shield
            } else if self.very_hurt {
                rules.invul_post_shield
            } else {
                rules.invul_pre_shield
            };
        }
        false
    }

    pub fn tick(&mut self, rules: &ZombieRules, dt: f32) {
        self.since_hit += dt;
        self.invulnerable = (self.invulnerable - dt).max(0.0);
        if self.hp >= self.max || self.since_hit < rules.regen_delay {
            return;
        }
        if !self.very_hurt {
            self.hp = self.max;
        } else if self.since_hit > rules.long_regen_time {
            self.hp = (self.hp + rules.regen_rate * self.max * dt).min(self.max);
        }
        if self.hp >= self.max {
            self.very_hurt = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Power-up drops.

/// `_zombiemode_powerups`: the score trigger, the per-round cap and the
/// shuffled cycle of power-ups.
#[derive(Debug, Clone)]
pub struct PowerupDrops {
    increment: f32,
    score_to_drop: f32,
    /// `zombie_drop_item`: the next kill may drop.
    pub armed: bool,
    pub this_round: u32,
    bag: Vec<Powerup>,
    next: usize,
}

impl PowerupDrops {
    pub fn new(rules: &ZombieRules, players: u32) -> Self {
        Self {
            increment: rules.drop_increment,
            score_to_drop: (players.max(1) * rules.score_start) as f32 + rules.drop_increment,
            armed: false,
            this_round: 0,
            bag: rules.powerups.clone(),
            // Shuffled before the first draw.
            next: usize::MAX,
        }
    }

    pub fn round_start(&mut self) {
        self.this_round = 0;
    }

    /// `watch_for_drop`: call with the players' total earned score.
    pub fn on_score(&mut self, rules: &ZombieRules, total_score: u32) {
        if total_score as f32 > self.score_to_drop {
            self.increment *= rules.drop_increment_factor;
            self.score_to_drop = total_score as f32 + self.increment;
            self.armed = true;
        }
    }

    /// The score that arms the next drop.
    pub fn score_to_drop(&self) -> f32 {
        self.score_to_drop
    }

    /// `powerup_drop` for a dead zombie. `roll100` is `RandomInt(100)`,
    /// `rand_index(n)` a uniform index below `n` (for the shuffle).
    pub fn on_death(&mut self, rules: &ZombieRules, roll100: u32, in_playable_area: bool, rand_index: &mut dyn FnMut(usize) -> usize) -> Option<Powerup> {
        if self.this_round >= rules.drop_max_per_round {
            return None;
        }
        if roll100 > rules.drop_random_max && !self.armed {
            return None;
        }
        if !in_playable_area || self.bag.is_empty() {
            return None;
        }
        if self.next >= self.bag.len() {
            // array_randomize: Fisher-Yates.
            for i in (1..self.bag.len()).rev() {
                let j = rand_index(i + 1);
                self.bag.swap(i, j);
            }
            self.next = 0;
        }
        let p = self.bag[self.next];
        self.next += 1;
        self.this_round += 1;
        self.armed = false;
        Some(p)
    }
}

// ---------------------------------------------------------------------------
// The round state machine.

/// What a newly spawned zombie gets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnSpec {
    pub health: f32,
    pub gait: Gait,
    pub speed: f32,
}

/// Simple round state machine, independent of the engine.
#[derive(Debug, Clone)]
pub struct RoundState {
    pub rules: ZombieRules,
    pub players: u32,
    pub round: u32,
    pub to_spawn: u32,
    pub alive: u32,
    pub spawn_timer: f32,
    pub intermission: f32,
    /// `level.zombie_health` this round.
    pub health: i32,
    /// `zombie_spawn_delay` this round.
    pub delay: f32,
    pub drops: PowerupDrops,
    /// Repair points earned this round (towards the cap).
    pub repair_reward: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundEvent {
    None,
    /// A zombie should be spawned now.
    Spawn,
    /// The round just started.
    RoundStarted(u32),
    /// The last zombie of the round died.
    RoundEnded(u32),
}

impl Default for RoundState {
    fn default() -> Self {
        Self::new()
    }
}

impl RoundState {
    pub fn new() -> Self {
        Self::with_rules(ZombieRules::nacht())
    }

    pub fn with_rules(rules: ZombieRules) -> Self {
        let drops = PowerupDrops::new(&rules, 1);
        Self {
            players: 1,
            round: 0,
            to_spawn: 0,
            alive: 0,
            spawn_timer: 0.0,
            // Round 1 starts as soon as the player is in.
            intermission: 1e-3,
            health: rules.health_start,
            delay: rules.spawn_delay_start,
            drops,
            repair_reward: 0,
            rules,
        }
    }

    pub fn in_intermission(&self) -> bool {
        self.intermission > 0.0
    }

    fn start_next_round(&mut self) {
        self.round += 1;
        if self.round > 1 {
            // Between rounds: the spawn delay shrinks; then the new round's
            // health (`ai_calculate_health`).
            self.delay = self.rules.next_spawn_delay(self.delay);
        }
        self.health = self.rules.next_health(self.health, self.round);
        self.to_spawn = self.rules.zombies_in_round(self.round, self.players);
        self.spawn_timer = if self.round == 1 { self.rules.first_round_spawn_wait } else { self.rules.round_spawn_wait };
        self.drops.round_start();
        self.repair_reward = 0;
    }

    /// Advance by `dt`; returns at most one event per call.
    pub fn tick(&mut self, dt: f32) -> RoundEvent {
        if self.intermission > 0.0 {
            self.intermission -= dt;
            if self.intermission <= 0.0 {
                self.intermission = 0.0;
                self.start_next_round();
                return RoundEvent::RoundStarted(self.round);
            }
            return RoundEvent::None;
        }
        if self.to_spawn == 0 && self.alive == 0 {
            self.intermission = self.rules.between_round_time;
            return RoundEvent::RoundEnded(self.round);
        }
        if self.to_spawn > 0 {
            self.spawn_timer -= dt;
            if self.spawn_timer <= 0.0 {
                // The spawn loop waits the delay after every attempt; an
                // attempt fails while the AI limit is reached.
                self.spawn_timer = self.delay;
                if self.alive < self.rules.ai_limit {
                    self.to_spawn -= 1;
                    self.alive += 1;
                    return RoundEvent::Spawn;
                }
            }
        }
        RoundEvent::None
    }

    pub fn on_zombie_killed(&mut self) {
        self.alive = self.alive.saturating_sub(1);
    }

    /// Health, gait and speed for a zombie spawned now (`roll` uniform in
    /// [0,1) for the speed roll).
    pub fn spawn_spec(&self, roll: f32) -> SpawnSpec {
        let health = if self.health <= 0 { self.rules.health_start } else { self.health };
        let gait = self.rules.pick_gait(self.round.max(1), roll);
        SpawnSpec { health: health as f32, gait, speed: self.rules.gait_speed(gait) }
    }

    /// Points for repairing one board now (0 once the round's cap is hit).
    pub fn repair_points(&mut self, double_points: bool) -> u32 {
        let cost = if double_points { self.rules.repair_points * 2 } else { self.rules.repair_points };
        self.repair_reward += cost;
        if self.repair_reward < self.rules.repair_cap(self.round) {
            cost
        } else {
            0
        }
    }

    /// Whether a kill should drop a power-up, given a uniform roll (the
    /// score trigger is not tracked here; see [`PowerupDrops`]).
    pub fn should_drop(&mut self, roll: f32) -> bool {
        let roll100 = (roll.clamp(0.0, 0.999_999) * 100.0) as u32;
        self.drops.on_death(&self.rules, roll100, true, &mut |_| 0).is_some()
    }
}

// ---------------------------------------------------------------------------
// Reading `set_zombie_var` from the game's scripts.

/// One `set_zombie_var( "name", value [, div] )` call.
#[derive(Debug, Clone, PartialEq)]
pub struct ZombieVarCall {
    pub name: String,
    pub value: f32,
    pub div: Option<f32>,
}

/// Finds the `set_zombie_var` calls in a script (ignoring `//` comments and
/// `/* */` blocks). The first call for a name wins: later ones are cheats
/// or split-screen variants.
pub fn parse_zombie_vars(script: &str) -> Vec<ZombieVarCall> {
    let text = strip_comments(script);
    let mut out: Vec<ZombieVarCall> = Vec::new();
    let needle = "set_zombie_var";
    let lower = text.to_ascii_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find(needle) {
        let start = at + i + needle.len();
        at = start;
        let rest = &text[start..];
        let Some(open) = rest.find('(') else { break };
        if !rest[..open].trim().is_empty() {
            continue;
        }
        let Some(close) = rest[open..].find(')') else { break };
        let args: Vec<&str> = rest[open + 1..open + close].split(',').map(str::trim).collect();
        if args.len() < 2 {
            continue;
        }
        let name = args[0].trim_matches('"').to_string();
        if name.is_empty() || name.contains(' ') || out.iter().any(|c| c.name == name) {
            continue;
        }
        let num = |s: &str| -> Option<f32> {
            match s {
                "true" => Some(1.0),
                "false" => Some(0.0),
                _ => s.parse().ok(),
            }
        };
        let Some(value) = num(args[1]) else { continue };
        let div = args.get(2).and_then(|s| num(s));
        out.push(ZombieVarCall { name, value, div });
    }
    out
}

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// `set_zombie_var` semantics: a non-empty value in `mp/zombiemode.csv`
/// (column 0 = name, column 1 = value) replaces the script's, as an
/// integer; then the divisor applies.
pub fn resolve_zombie_vars(calls: &[ZombieVarCall], table: &dyn Fn(&str) -> Option<String>) -> HashMap<String, f32> {
    calls
        .iter()
        .map(|c| {
            let mut v = c.value;
            if let Some(t) = table(&c.name).filter(|t| !t.trim().is_empty()) {
                v = gsc_int(&t) as f32;
            }
            if let Some(d) = c.div.filter(|d| *d != 0.0) {
                v /= d;
            }
            (c.name.clone(), v)
        })
        .collect()
}

/// `int( string )`: leading integer, 0 if none.
fn gsc_int(s: &str) -> i64 {
    let s = s.trim();
    let end = s.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')))).map(|(i, _)| i).unwrap_or(s.len());
    s[..end].parse().unwrap_or(0)
}

/// The zombie mode script a level script runs (`maps\_zombiemode*::main`).
pub fn zombiemode_script_name(level_script: &str) -> Option<String> {
    let text = strip_comments(level_script);
    let lower = text.to_ascii_lowercase();
    let i = lower.find("maps\\_zombiemode")?;
    let rest = &text[i + 5..];
    let end = rest.find("::")?;
    let name = &rest[..end];
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_').then(|| format!("maps/{name}.gsc"))
}

/// The power-ups a level script includes (`include_powerup( "name" )`).
pub fn included_powerups(level_script: &str) -> Vec<Powerup> {
    let text = strip_comments(level_script);
    Powerup::ALL
        .into_iter()
        .filter(|p| {
            let quoted = format!("\"{}\"", p.script_name());
            text.match_indices("include_powerup").any(|(i, _)| text[i..].split(')').next().is_some_and(|call| call.contains(&quoted)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_matches_the_script() {
        let r = ZombieRules::nacht();
        let want = [(1, 150), (2, 250), (3, 350), (4, 450), (5, 550), (6, 650), (7, 750), (8, 850), (9, 950), (10, 1045)];
        for (round, hp) in want {
            assert_eq!(r.zombie_health(round), hp, "round {round}");
        }
        // Truncated each round, so a little under 950 * 1.1^n.
        assert_eq!(r.zombie_health(11), 1149);
        assert_eq!(r.zombie_health(15), 1679);
        assert_eq!(r.zombie_health(20), 2701);
        assert_eq!(r.zombie_health(30), 7000);
        assert_eq!(r.zombie_health(50), 47073);
        assert_eq!(r.zombie_health(100), 5_525_295);
        // The 32-bit wrap the wiki puts at round 163.
        assert!(r.zombie_health_raw(162) > 2_000_000_000);
        assert!(r.zombie_health_raw(163) < 0);
        assert_eq!(r.zombie_health(163), 150);
        assert_eq!(zombie_health(9), 950.0);
    }

    #[test]
    fn solo_counts() {
        let r = ZombieRules::nacht();
        let solo: Vec<u32> = (1..=12).map(|n| r.zombies_in_round(n, 1)).collect();
        assert_eq!(solo, vec![4, 9, 14, 19, 24, 24, 24, 24, 24, 24, 24, 24]);
        assert_eq!(r.zombies_in_round(50, 1), 24);
        // Co-op grows: 2 players, round 1 = int(30 * 0.2).
        assert_eq!(r.zombies_in_round(1, 2), 6);
        assert_eq!(r.zombies_in_round(10, 4), 24 + (18.0f32 * 2.0 * 1.5) as u32);
        // Der Riese counts half a player when alone (wiki: 33 at round 10).
        let riese = ZombieRules { solo_player_factor: 0.5, ..ZombieRules::nacht() };
        assert_eq!(riese.zombies_in_round(10, 1), 33);
        assert_eq!(riese.zombies_in_round(20, 1), 60);
        assert_eq!(riese.zombies_in_round(30, 1), 105);
    }

    #[test]
    fn spawn_delay_shrinks() {
        let r = ZombieRules::nacht();
        assert_eq!(r.spawn_delay(1), 3.0);
        assert!((r.spawn_delay(2) - 2.85).abs() < 1e-5);
        assert!((r.spawn_delay(10) - 3.0 * 0.95f32.powi(9)).abs() < 1e-4);
        // Clamped to 0.08 then shrunk: never below 0.076.
        assert!((r.spawn_delay(200) - 0.076).abs() < 1e-4);
    }

    #[test]
    fn gaits_follow_move_speed() {
        let r = ZombieRules::nacht();
        assert_eq!(r.move_speed(1), 1);
        assert_eq!(r.move_speed(2), 8);
        assert_eq!(r.move_speed(10), 72);
        assert_eq!(r.gait_odds(1), [1.0, 0.0, 0.0]);
        assert_eq!(r.gait_odds(2), [28.0 / 35.0, 7.0 / 35.0, 0.0]);
        assert_eq!(r.gait_odds(5), [4.0 / 35.0, 31.0 / 35.0, 0.0]);
        // First sprinters in round 6, none walk.
        assert_eq!(r.gait_odds(6), [0.0, 31.0 / 35.0, 4.0 / 35.0]);
        assert_eq!(r.gait_odds(9), [0.0, 7.0 / 35.0, 28.0 / 35.0]);
        assert_eq!(r.gait_odds(10), [0.0, 0.0, 1.0]);
        assert_eq!(pick_gait(1, 0.999), Gait::Walk);
        assert_eq!(pick_gait(2, 0.0), Gait::Walk);
        assert_eq!(pick_gait(2, 0.99), Gait::Run);
        assert_eq!(pick_gait(6, 0.99), Gait::Sprint);
        assert_eq!(pick_gait(20, 0.0), Gait::Sprint);
        assert!(Gait::Walk.speed() < Gait::Run.speed() && Gait::Run.speed() < Gait::Sprint.speed());
        assert!((Gait::Sprint.speed() - 3.57).abs() < 0.01);
    }

    #[test]
    fn points() {
        let r = ZombieRules::nacht();
        assert_eq!(r.hit_points(), 10);
        assert_eq!(r.kill_points(KillKind::Body), 50);
        assert_eq!(r.kill_points(KillKind::Torso), 60);
        assert_eq!(r.kill_points(KillKind::Neck), 70);
        assert_eq!(r.kill_points(KillKind::Head), 100);
        assert_eq!(r.kill_points(KillKind::Melee), 130);
        assert_eq!(r.kill_points(KillKind::Explosive), 50);
        assert_eq!(r.insta_kill_points(KillKind::Head, false), 60);
        assert_eq!(r.insta_kill_points(KillKind::Melee, true), 130);
        assert_eq!(round_up_to_ten(6), 10);
        assert_eq!(round_up_to_ten(50), 50);
        assert_eq!(body_location(1.0, 1.0), KillKind::Torso);
        assert_eq!(body_location(0.5, 1.0), KillKind::Body);
    }

    #[test]
    fn repair_cap() {
        let mut rs = RoundState::new();
        rs.tick(0.01);
        assert_eq!(rs.round, 1);
        // Cap 50 in round 1; the reward is given while the running total
        // stays under it, so 4 boards pay.
        let paid: Vec<u32> = (0..6).map(|_| rs.repair_points(false)).collect();
        assert_eq!(paid, vec![10, 10, 10, 10, 0, 0]);
        assert_eq!(ZombieRules::nacht().repair_cap(30), 500);
    }

    #[test]
    fn full_round_cycle() {
        let mut rs = RoundState::new();
        assert_eq!(rs.tick(0.1), RoundEvent::RoundStarted(1));
        let mut spawned = 0;
        let mut first_spawn = None;
        let mut t: f32 = 0.0;
        let mut ended = false;
        for _ in 0..100_000 {
            t += 0.01;
            match rs.tick(0.01) {
                RoundEvent::Spawn => {
                    spawned += 1;
                    first_spawn.get_or_insert(t);
                    rs.on_zombie_killed();
                }
                RoundEvent::RoundEnded(1) => {
                    ended = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(ended);
        assert_eq!(spawned, 4);
        let first = first_spawn.unwrap();
        assert!((first - 6.75).abs() < 0.02, "{first}");
        assert!(rs.in_intermission());
        // Next round after 10 s, first spawn 0.5 s later, faster spawns.
        let mut t: f32 = 0.0;
        loop {
            t += 0.01;
            if rs.tick(0.01) == RoundEvent::RoundStarted(2) {
                break;
            }
        }
        assert!((t - 10.0).abs() < 0.02);
        assert_eq!(rs.health, 250);
        assert!((rs.delay - 2.85).abs() < 1e-5);
        assert_eq!(rs.to_spawn, 9);
    }

    #[test]
    fn ai_limit_holds_spawns() {
        let rules = ZombieRules { ai_limit: 2, ..ZombieRules::nacht() };
        let mut rs = RoundState::with_rules(rules);
        rs.tick(0.01);
        let n = (0..2000).filter(|_| rs.tick(0.01) == RoundEvent::Spawn).count();
        assert_eq!(n, 2);
        assert_eq!(rs.alive, 2);
    }

    #[test]
    fn health_regen() {
        let r = ZombieRules::nacht();
        let mut h = PlayerHealth::new(100.0);
        assert!(!h.hit(&r, 60.0));
        assert_eq!(h.hp, 40.0);
        // Invulnerable for a moment after a big hit.
        assert!(!h.hit(&r, 60.0));
        h.tick(&r, 0.4);
        // A second hit before regen downs the player.
        let mut h2 = h.clone();
        assert!(h2.hit(&r, 60.0));
        // After 2.4 s without a hit health is full again at once.
        h.tick(&r, 2.1);
        assert_eq!(h.hp, 100.0);
        // Very hurt: waits 5 s, then 0.5 s to full.
        let mut v = PlayerHealth::new(100.0);
        v.hit(&r, 85.0);
        assert!(v.very_hurt);
        v.tick(&r, 4.9);
        assert_eq!(v.hp, 15.0);
        for _ in 0..12 {
            v.tick(&r, 0.05);
        }
        assert_eq!(v.hp, 100.0);
    }

    #[test]
    fn drops() {
        let r = ZombieRules::nacht();
        let mut d = PowerupDrops::new(&r, 1);
        let mut pick = |n: usize| n - 1;
        // Not armed: only the 3% random drop.
        assert_eq!(d.on_death(&r, 50, true, &mut pick), None);
        assert!(d.on_death(&r, 2, true, &mut pick).is_some());
        // Armed once the total passes 2500, then 2000 * 1.14 more.
        d.on_score(&r, 2500);
        assert!(!d.armed);
        d.on_score(&r, 2510);
        assert!(d.armed);
        assert!((d.score_to_drop() - (2510.0 + 2280.0)).abs() < 0.01);
        assert!(d.on_death(&r, 99, true, &mut pick).is_some());
        assert!(!d.armed);
        // Outside the playable area nothing drops (and the trigger stays).
        d.on_score(&r, 10_000);
        assert_eq!(d.on_death(&r, 99, false, &mut pick), None);
        assert!(d.armed);
        // A full cycle has every power-up once; max 4 per round.
        let mut d = PowerupDrops::new(&r, 1);
        let got: Vec<Powerup> = (0..6).filter_map(|_| d.on_death(&r, 0, true, &mut pick)).collect();
        assert_eq!(got.len(), 4);
        for p in &r.powerups {
            assert!(got.contains(p));
        }
        d.round_start();
        assert!(d.on_death(&r, 0, true, &mut pick).is_some());
    }

    #[test]
    fn powerup_blink() {
        let r = ZombieRules::nacht();
        assert_eq!(r.powerup_ttl(), 26.5);
        assert_eq!(r.powerup_visible(10.0), Some(true));
        assert_eq!(r.powerup_visible(15.2), Some(true));
        assert_eq!(r.powerup_visible(15.7), Some(false));
        assert_eq!(r.powerup_visible(26.45), Some(false));
        assert_eq!(r.powerup_visible(26.6), None);
        assert!((r.powerup_grab_flat() - 2496f32.sqrt() * INCH).abs() < 1e-4);
    }

    #[test]
    fn reads_script_vars() {
        let script = r#"
            set_zombie_var( "zombie_spawn_delay", 				3 );
            // set_zombie_var( "zombie_max_ai", 99 );
            set_zombie_var( "zombie_health_increase_percent", 	10, 	100 );
            set_zombie_var( "zombify_player", false );
            set_zombie_var( "zombie_score_start", 500 );
            if( cheat ) { set_zombie_var( "zombie_score_start", 100000 ); }
            set_zombie_var( "zombie_max_ai", 24 );
        "#;
        let calls = parse_zombie_vars(script);
        assert_eq!(calls.len(), 5);
        let table = |k: &str| (k == "zombie_max_ai").then(|| "30".to_string());
        let vars = resolve_zombie_vars(&calls, &table);
        assert_eq!(vars["zombie_spawn_delay"], 3.0);
        assert!((vars["zombie_health_increase_percent"] - 0.1).abs() < 1e-6);
        assert_eq!(vars["zombie_score_start"], 500.0);
        assert_eq!(vars["zombie_max_ai"], 30.0);
        let mut r = ZombieRules::nacht();
        r.apply_vars(&vars);
        assert_eq!(r.max_ai, 30);
        assert_eq!(
            zombiemode_script_name("main() {\n maps\\_zombiemode_prototype::main();\n}"),
            Some("maps/_zombiemode_prototype.gsc".into())
        );
        let lvl = "include_powerup( \"nuke\" );\n//include_powerup( \"carpenter\" );\ninclude_powerup( \"full_ammo\" );";
        assert_eq!(included_powerups(lvl), vec![Powerup::MaxAmmo, Powerup::Nuke]);
    }
}
