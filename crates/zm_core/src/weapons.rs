//! Weapon table. Values are this project's own tuning; when the player's
//! extracted install contains a matching weapon file, its numbers override
//! ours (see [`WeaponDef::apply_weapon_file`]).

use crate::weaponfile::WeaponFile;

/// Engine units in weapon files are inches; the game world is in metres.
pub const UNITS_TO_M: f32 = 0.0254;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireMode {
    Semi,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pistol,
    Rifle,
    Smg,
    Shotgun,
    Lmg,
    /// Hitscan with an explosion at the impact point.
    Wonder,
}

#[derive(Debug, Clone)]
pub struct WeaponDef {
    pub id: &'static str,
    pub name: String,
    pub kind: Kind,
    pub damage: f32,
    /// Damage at max range (linear falloff between near and far).
    pub min_damage: f32,
    pub near: f32,
    pub far: f32,
    pub head_mult: f32,
    pub fire_interval: f32,
    pub reload_time: f32,
    pub clip: u32,
    pub max_ammo: u32,
    pub mode: FireMode,
    pub pellets: u32,
    /// Spread cone half-angle in degrees.
    pub spread: f32,
    pub splash_radius: f32,
    pub splash_damage: f32,
    /// Purely cosmetic: viewmodel length and recoil kick.
    pub kick: f32,
    /// Bolt-action or pump: the rechamber animation plays after each shot.
    pub rechamber: bool,
    /// File name to look for in `<assets>/iwd/weapons/sp/` (optional).
    pub weapon_file: Option<&'static str>,
}

impl WeaponDef {
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: &'static str,
        name: &str,
        kind: Kind,
        damage: f32,
        fire_interval: f32,
        reload_time: f32,
        clip: u32,
        max_ammo: u32,
        mode: FireMode,
    ) -> Self {
        Self {
            id,
            name: name.to_string(),
            kind,
            damage,
            min_damage: damage * 0.6,
            near: 12.0,
            far: 35.0,
            head_mult: 2.5,
            fire_interval,
            reload_time,
            clip,
            max_ammo,
            mode,
            pellets: 1,
            spread: 0.6,
            splash_radius: 0.0,
            splash_damage: 0.0,
            kick: 1.0,
            rechamber: matches!(id, "kar98k" | "kar98k_scoped_zombie" | "springfield" | "trenchgun"),
            weapon_file: None,
        }
    }

    /// Damage at `dist` metres before head/body multipliers.
    pub fn damage_at(&self, dist: f32) -> f32 {
        if dist <= self.near {
            self.damage
        } else if dist >= self.far {
            self.min_damage
        } else {
            let t = (dist - self.near) / (self.far - self.near);
            self.damage + (self.min_damage - self.damage) * t
        }
    }

    /// Override numeric stats from a weapon file in the user's install.
    /// Returns how many fields were applied.
    pub fn apply_weapon_file(&mut self, wf: &WeaponFile) -> usize {
        let mut n = 0;
        fn set(n: &mut usize, dst: &mut f32, v: Option<f32>) {
            if let Some(v) = v.filter(|v| v.is_finite() && *v > 0.0) {
                *dst = v;
                *n += 1;
            }
        }
        set(&mut n, &mut self.damage, wf.f32("damage"));
        set(&mut n, &mut self.min_damage, wf.f32("minDamage"));
        set(&mut n, &mut self.fire_interval, wf.f32("fireTime"));
        set(&mut n, &mut self.reload_time, wf.f32("reloadTime"));
        set(&mut n, &mut self.near, wf.f32("maxDamageRange").map(|u| u * UNITS_TO_M));
        set(&mut n, &mut self.far, wf.f32("minDamageRange").map(|u| u * UNITS_TO_M));
        if let Some(r) = wf.f32("explosionRadius").filter(|r| *r > 0.0) {
            self.splash_radius = (r * UNITS_TO_M).max(1.0);
            n += 1;
        }
        set(&mut n, &mut self.splash_damage, wf.f32("explosionOuterDamage"));
        if let Some(c) = wf.u32("clipSize").filter(|c| *c > 0) {
            self.clip = c;
            n += 1;
        }
        if let Some(m) = wf.u32("maxAmmo").filter(|m| *m > 0) {
            self.max_ammo = m;
            n += 1;
        }
        if let Some(p) = wf.u32("shotCount").filter(|p| *p > 0) {
            self.pellets = p;
            n += 1;
        }
        if let Some(b) = wf.get("boltAction") {
            self.rechamber = b.trim() == "1";
            n += 1;
        }
        if let Some(ft) = wf.get("fireType") {
            self.mode = if ft.eq_ignore_ascii_case("full auto") { FireMode::Auto } else { FireMode::Semi };
            n += 1;
        }
        if self.min_damage > self.damage {
            self.min_damage = self.damage;
        }
        if self.far <= self.near {
            self.far = self.near + 1.0;
        }
        n
    }
}

/// Index of the starting pistol in [`default_weapons`].
pub const START_PISTOL: usize = 0;

pub fn default_weapons() -> Vec<WeaponDef> {
    use FireMode::*;
    use Kind::*;
    let mut v = Vec::new();

    let mut pistol = WeaponDef::new("m1911", "M1911", Pistol, 30.0, 0.12, 1.6, 8, 80, Semi);
    pistol.weapon_file = Some("zombie_colt");
    pistol.kick = 0.7;
    v.push(pistol);

    let mut kar = WeaponDef::new("kar98k", "Kar98k", Rifle, 100.0, 1.1, 2.6, 5, 50, Semi);
    kar.head_mult = 4.0;
    kar.near = 40.0;
    kar.far = 80.0;
    kar.spread = 0.1;
    kar.kick = 2.0;
    v.push(kar);

    let mut carbine = WeaponDef::new("m1carbine", "M1A1 Carbine", Rifle, 50.0, 0.14, 2.2, 15, 150, Semi);
    carbine.spread = 0.3;
    v.push(carbine);

    let mut garand = WeaponDef::new("m1garand", "M1 Garand", Rifle, 90.0, 0.16, 2.4, 8, 96, Semi);
    garand.head_mult = 3.0;
    garand.spread = 0.2;
    garand.kick = 1.6;
    v.push(garand);

    let mut thompson = WeaponDef::new("thompson", "Thompson", Smg, 40.0, 0.085, 2.3, 20, 200, Auto);
    thompson.spread = 1.6;
    thompson.kick = 0.8;
    v.push(thompson);

    let mut mp40 = WeaponDef::new("mp40", "MP40", Smg, 35.0, 0.1, 2.4, 32, 192, Auto);
    mp40.spread = 1.4;
    mp40.kick = 0.7;
    v.push(mp40);

    let mut ppsh = WeaponDef::new("ppsh", "PPSh-41", Smg, 30.0, 0.055, 3.0, 71, 284, Auto);
    ppsh.spread = 2.2;
    ppsh.kick = 0.6;
    v.push(ppsh);

    let mut stg = WeaponDef::new("stg44", "STG-44", Rifle, 55.0, 0.1, 2.5, 30, 180, Auto);
    stg.spread = 1.2;
    v.push(stg);

    let mut bar = WeaponDef::new("bar", "BAR", Lmg, 70.0, 0.13, 2.8, 20, 160, Auto);
    bar.spread = 1.3;
    bar.kick = 1.4;
    v.push(bar);

    let mut trench = WeaponDef::new("trenchgun", "Trench Gun", Shotgun, 45.0, 0.75, 3.2, 6, 60, Semi);
    trench.pellets = 8;
    trench.spread = 5.0;
    trench.near = 4.0;
    trench.far = 14.0;
    trench.min_damage = 10.0;
    trench.head_mult = 1.5;
    trench.kick = 2.2;
    v.push(trench);

    let mut dbl = WeaponDef::new("doublebarrel", "Double-Barrel", Shotgun, 50.0, 0.3, 2.6, 2, 60, Semi);
    dbl.pellets = 10;
    dbl.spread = 6.5;
    dbl.near = 3.5;
    dbl.far = 12.0;
    dbl.min_damage = 10.0;
    dbl.head_mult = 1.5;
    dbl.kick = 2.5;
    v.push(dbl);

    // Variants found on Nacht der Untoten's walls.
    let mut sawed = WeaponDef::new("doublebarrel_sawed_grip", "Sawed-Off Double-Barrel", Shotgun, 55.0, 0.3, 2.6, 2, 60, Semi);
    sawed.pellets = 10;
    sawed.spread = 8.5;
    sawed.near = 3.0;
    sawed.far = 9.0;
    sawed.min_damage = 10.0;
    sawed.head_mult = 1.5;
    sawed.kick = 2.7;
    v.push(sawed);

    let mut scoped = WeaponDef::new("kar98k_scoped_zombie", "Scoped Kar98k", Rifle, 150.0, 1.2, 2.8, 5, 60, Semi);
    scoped.weapon_file = Some("kar98k_scoped_zombie");
    scoped.head_mult = 4.0;
    scoped.near = 60.0;
    scoped.far = 120.0;
    scoped.spread = 0.05;
    scoped.kick = 2.2;
    v.push(scoped);

    let mut wonder = WeaponDef::new("raypistol", "Ray Pistol", Wonder, 1000.0, 0.33, 3.0, 20, 160, Semi);
    wonder.weapon_file = Some("ray_gun");
    wonder.min_damage = 1000.0;
    wonder.near = 100.0;
    wonder.far = 200.0;
    wonder.spread = 0.0;
    wonder.splash_radius = 2.0;
    wonder.splash_damage = 300.0;
    wonder.head_mult = 1.0;
    wonder.kick = 1.2;
    v.push(wonder);

    v
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

    #[test]
    fn map_aliases() {
        let defs = default_weapons();
        assert_eq!(defs[find(&defs, "shotgun").unwrap()].id, "trenchgun");
        // Only bolt-action and pump guns cycle after a shot.
        let rechambers = |id: &str| defs[find(&defs, id).unwrap()].rechamber;
        assert!(rechambers("kar98k") && rechambers("trenchgun"));
        assert!(!rechambers("doublebarrel_sawed_grip") && !rechambers("thompson"));
        for id in ["kar98k", "thompson", "bar", "m1carbine", "doublebarrel", "kar98k_scoped_zombie", "doublebarrel_sawed_grip"] {
            assert!(find(&defs, id).is_some(), "{id}");
        }
        assert!(find(&defs, "stielhandgranate").is_none());
    }

    #[test]
    fn falloff() {
        let d = &default_weapons()[START_PISTOL];
        assert_eq!(d.damage_at(0.0), d.damage);
        assert_eq!(d.damage_at(1000.0), d.min_damage);
        let mid = d.damage_at((d.near + d.far) / 2.0);
        assert!(mid <= d.damage && mid >= d.min_damage);
    }

    #[test]
    fn weapon_file_override() {
        let mut d = default_weapons().remove(START_PISTOL);
        let wf = WeaponFile::parse(
            "WEAPONFILE\\damage\\20\\minDamage\\20\\fireTime\\0.075\\clipSize\\8\\maxAmmo\\80\\fireType\\Single Shot\\maxDamageRange\\425\\minDamageRange\\1000",
        )
        .unwrap();
        assert!(d.apply_weapon_file(&wf) >= 7);
        assert_eq!(d.damage, 20.0);
        assert_eq!(d.mode, FireMode::Semi);
        assert!((d.near - 10.795).abs() < 0.01);
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
        for id in ["raypistol", "trenchgun", "mp40", "thompson", "doublebarrel_sawed_grip", "kar98k_scoped_zombie"] {
            assert!(ids.contains(&id), "{id}");
        }
        assert!(pool.iter().all(|(_, w)| *w == 1));
    }
}
