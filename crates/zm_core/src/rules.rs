//! Round, scoring and difficulty rules. All tuning here is this project's own.

pub const PLAYER_MAX_HEALTH: f32 = 100.0;
pub const ZOMBIE_HIT_DAMAGE: f32 = 40.0;
pub const REGEN_DELAY: f32 = 3.5;
pub const REGEN_PER_SEC: f32 = 60.0;
pub const MAX_ALIVE: usize = 24;
pub const BOARDS_PER_WINDOW: u8 = 6;
pub const INTERMISSION_SECS: f32 = 10.0;

pub const POINTS_START: u32 = 500;
pub const POINTS_HIT: u32 = 10;
pub const POINTS_KILL: u32 = 50;
pub const POINTS_HEADSHOT_KILL: u32 = 100;
pub const POINTS_MELEE_KILL: u32 = 130;
pub const POINTS_BOARD: u32 = 10;
pub const POINTS_NUKE: u32 = 400;
pub const POINTS_CARPENTER: u32 = 200;
pub const CRATE_COST: u32 = 950;
pub const POWERUP_DURATION: f32 = 30.0;
pub const POWERUP_TTL: f32 = 26.0;
pub const MAX_DROPS_PER_ROUND: u32 = 4;
pub const DROP_CHANCE: f32 = 0.025;

/// Zombie hit points for a given round (1-based).
/// Linear growth early on, then compounding 10% per round.
pub fn zombie_health(round: u32) -> f32 {
    let r = round.max(1);
    if r <= 9 {
        150.0 + 100.0 * (r - 1) as f32
    } else {
        950.0 * 1.1f32.powi((r - 9) as i32)
    }
}

/// How many zombies spawn in a round for a solo player.
pub fn zombies_in_round(round: u32) -> u32 {
    match round.max(1) {
        1 => 6,
        2 => 8,
        3 => 13,
        4 => 18,
        r => (24.0 * (r as f32 * 0.15).max(1.0)).floor() as u32,
    }
}

/// Seconds between spawns.
pub fn spawn_delay(round: u32) -> f32 {
    (2.0 * 0.95f32.powi(round.max(1) as i32 - 1)).max(0.1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gait {
    Walk,
    Run,
    Sprint,
}

impl Gait {
    pub fn speed(self) -> f32 {
        match self {
            Gait::Walk => 1.1,
            Gait::Run => 3.0,
            Gait::Sprint => 4.6,
        }
    }
}

/// Pick a gait from a uniform roll in [0,1). Faster gaits unlock as rounds rise.
pub fn pick_gait(round: u32, roll: f32) -> Gait {
    let r = round as f32;
    let run_chance = ((r - 2.0) * 0.12).clamp(0.0, 1.0);
    let sprint_chance = ((r - 5.0) * 0.08).clamp(0.0, 0.6);
    if roll < sprint_chance {
        Gait::Sprint
    } else if roll < sprint_chance + run_chance {
        Gait::Run
    } else {
        Gait::Walk
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
    pub const ALL: [Powerup; 5] = [
        Powerup::MaxAmmo,
        Powerup::InstaKill,
        Powerup::DoublePoints,
        Powerup::Nuke,
        Powerup::Carpenter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Powerup::MaxAmmo => "Max Ammo",
            Powerup::InstaKill => "Insta-Kill",
            Powerup::DoublePoints => "Double Points",
            Powerup::Nuke => "Nuke",
            Powerup::Carpenter => "Carpenter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillKind {
    Body,
    Head,
    Melee,
    Explosive,
}

/// Points for killing a zombie (excluding the per-hit points).
pub fn kill_points(kind: KillKind) -> u32 {
    match kind {
        KillKind::Body | KillKind::Explosive => POINTS_KILL,
        KillKind::Head => POINTS_HEADSHOT_KILL,
        KillKind::Melee => POINTS_MELEE_KILL,
    }
}

/// Apply the double-points multiplier.
pub fn scaled(points: u32, double: bool) -> u32 {
    if double {
        points * 2
    } else {
        points
    }
}

/// Simple round state machine, independent of the engine.
#[derive(Debug, Clone)]
pub struct RoundState {
    pub round: u32,
    pub to_spawn: u32,
    pub alive: u32,
    pub spawn_timer: f32,
    pub intermission: f32,
    pub drops_this_round: u32,
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
        Self {
            round: 0,
            to_spawn: 0,
            alive: 0,
            spawn_timer: 0.0,
            intermission: 4.0,
            drops_this_round: 0,
        }
    }

    pub fn in_intermission(&self) -> bool {
        self.intermission > 0.0
    }

    /// Advance by `dt`; returns at most one event per call.
    pub fn tick(&mut self, dt: f32) -> RoundEvent {
        if self.intermission > 0.0 {
            self.intermission -= dt;
            if self.intermission <= 0.0 {
                self.intermission = 0.0;
                self.round += 1;
                self.to_spawn = zombies_in_round(self.round);
                self.spawn_timer = 1.0;
                self.drops_this_round = 0;
                return RoundEvent::RoundStarted(self.round);
            }
            return RoundEvent::None;
        }
        if self.to_spawn == 0 && self.alive == 0 {
            self.intermission = INTERMISSION_SECS;
            return RoundEvent::RoundEnded(self.round);
        }
        if self.to_spawn > 0 && (self.alive as usize) < MAX_ALIVE {
            self.spawn_timer -= dt;
            if self.spawn_timer <= 0.0 {
                self.spawn_timer = spawn_delay(self.round);
                self.to_spawn -= 1;
                self.alive += 1;
                return RoundEvent::Spawn;
            }
        }
        RoundEvent::None
    }

    pub fn on_zombie_killed(&mut self) {
        self.alive = self.alive.saturating_sub(1);
    }

    /// Whether a kill should drop a power-up, given a uniform roll.
    pub fn should_drop(&mut self, roll: f32) -> bool {
        if self.drops_this_round < MAX_DROPS_PER_ROUND && roll < DROP_CHANCE {
            self.drops_this_round += 1;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_curve_is_monotonic() {
        let mut last = 0.0;
        for r in 1..60 {
            let h = zombie_health(r);
            assert!(h > last, "round {r}");
            last = h;
        }
        assert_eq!(zombie_health(1), 150.0);
        assert_eq!(zombie_health(9), 950.0);
    }

    #[test]
    fn counts_grow() {
        assert_eq!(zombies_in_round(1), 6);
        assert!(zombies_in_round(10) > zombies_in_round(5));
    }

    #[test]
    fn gaits_progress() {
        assert_eq!(pick_gait(1, 0.0), Gait::Walk);
        assert_eq!(pick_gait(20, 0.0), Gait::Sprint);
        assert_eq!(pick_gait(20, 0.99), Gait::Run);
    }

    #[test]
    fn full_round_cycle() {
        let mut rs = RoundState::new();
        let mut started = None;
        for _ in 0..1000 {
            if let RoundEvent::RoundStarted(r) = rs.tick(0.1) {
                started = Some(r);
                break;
            }
        }
        assert_eq!(started, Some(1));
        let mut spawned = 0;
        let mut ended = false;
        for _ in 0..10_000 {
            match rs.tick(0.1) {
                RoundEvent::Spawn => {
                    spawned += 1;
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
        assert_eq!(spawned, zombies_in_round(1));
        assert!(rs.in_intermission());
    }

    #[test]
    fn drops_capped() {
        let mut rs = RoundState::new();
        let n = (0..100).filter(|_| rs.should_drop(0.0)).count();
        assert_eq!(n as u32, MAX_DROPS_PER_ROUND);
    }
}
