//! The gameplay numbers of a zone `WeaponDef` (2476 bytes, layout in
//! `research/t4/T4_LAYOUTS.txt`), returned under the key names and units of
//! the plain-text weapon files (`weapons/sp/<name>`): times in seconds (the
//! zone stores milliseconds), enums as the words the files use. That lets one
//! parser apply either source.

/// Hit locations in `hitLocation_t` order, as weapon-file keys.
pub const LOCATION_KEYS: [&str; 19] = [
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
];

#[derive(Clone, Copy)]
enum K {
    Int,
    /// Integer milliseconds, written as seconds.
    Ms,
    Float,
    Bool,
    Enum(&'static [&'static str]),
}

const WEAP_TYPE: &[&str] = &["bullet", "grenade", "projectile", "binoculars", "gas", "bomb", "mine"];
const WEAP_CLASS: &[&str] = &["rifle", "mg", "smg", "spread", "pistol", "grenade", "rocketlauncher", "turret", "non-player", "gas", "item"];
const PENETRATE: &[&str] = &["none", "small", "medium", "large"];
const FIRE_TYPE: &[&str] = &["Full Auto", "Single Shot", "2-Round Burst", "3-Round Burst", "4-Round Burst"];

const FIELDS: &[(&str, usize, K)] = &[
    ("weaponType", 0x144, K::Enum(WEAP_TYPE)),
    ("weaponClass", 0x148, K::Enum(WEAP_CLASS)),
    ("penetrateType", 0x14c, K::Enum(PENETRATE)),
    ("fireType", 0x158, K::Enum(FIRE_TYPE)),
    ("startAmmo", 0x3ec, K::Int),
    ("maxAmmo", 0x404, K::Int),
    ("clipSize", 0x408, K::Int),
    ("shotCount", 0x40c, K::Int),
    ("damage", 0x420, K::Int),
    ("playerDamage", 0x42c, K::Int),
    ("meleeDamage", 0x430, K::Int),
    ("fireDelay", 0x438, K::Ms),
    ("meleeDelay", 0x43c, K::Ms),
    ("fireTime", 0x448, K::Ms),
    ("rechamberTime", 0x44c, K::Ms),
    ("rechamberBoltTime", 0x450, K::Ms),
    ("holdFireTime", 0x454, K::Ms),
    ("meleeTime", 0x45c, K::Ms),
    ("reloadTime", 0x464, K::Ms),
    ("reloadEmptyTime", 0x46c, K::Ms),
    ("reloadAddTime", 0x470, K::Ms),
    ("reloadEmptyAddTime", 0x474, K::Ms),
    ("reloadStartTime", 0x478, K::Ms),
    ("reloadStartAddTime", 0x47c, K::Ms),
    ("reloadEndTime", 0x480, K::Ms),
    ("dropTime", 0x484, K::Ms),
    ("raiseTime", 0x488, K::Ms),
    ("firstRaiseTime", 0x49c, K::Ms),
    ("emptyRaiseTime", 0x4a0, K::Ms),
    ("emptyDropTime", 0x4a4, K::Ms),
    ("sprintInTime", 0x4a8, K::Ms),
    ("sprintLoopTime", 0x4ac, K::Ms),
    ("sprintOutTime", 0x4b0, K::Ms),
    ("moveSpeedScale", 0x504, K::Float),
    ("adsMoveSpeedScale", 0x508, K::Float),
    ("sprintDurationScale", 0x50c, K::Float),
    ("adsZoomFov", 0x510, K::Float),
    ("adsZoomInFrac", 0x514, K::Float),
    ("adsZoomOutFrac", 0x518, K::Float),
    ("hipSpreadStandMin", 0x53c, K::Float),
    ("hipSpreadDuckedMin", 0x540, K::Float),
    ("hipSpreadProneMin", 0x544, K::Float),
    ("hipSpreadMax", 0x548, K::Float),
    ("hipSpreadDuckedMax", 0x54c, K::Float),
    ("hipSpreadProneMax", 0x550, K::Float),
    ("hipSpreadDecayRate", 0x554, K::Float),
    ("hipSpreadFireAdd", 0x558, K::Float),
    ("hipSpreadTurnAdd", 0x55c, K::Float),
    ("hipSpreadMoveAdd", 0x560, K::Float),
    ("hipSpreadDuckedDecay", 0x564, K::Float),
    ("hipSpreadProneDecay", 0x568, K::Float),
    ("adsTransInTime", 0x570, K::Ms),
    ("adsTransOutTime", 0x574, K::Ms),
    ("rifleBullet", 0x5cc, K::Bool),
    ("armorPiercing", 0x5d0, K::Bool),
    ("boltAction", 0x5d4, K::Bool),
    ("aimDownSight", 0x5d8, K::Bool),
    ("rechamberWhileAds", 0x5dc, K::Bool),
    ("clipOnly", 0x5ec, K::Bool),
    ("noPartialReload", 0x624, K::Bool),
    ("segmentedReload", 0x628, K::Bool),
    ("reloadAmmoAdd", 0x630, K::Int),
    ("reloadStartAdd", 0x634, K::Int),
    ("explosionRadius", 0x650, K::Int),
    ("explosionRadiusMin", 0x654, K::Int),
    ("explosionInnerDamage", 0x658, K::Int),
    ("explosionOuterDamage", 0x65c, K::Int),
    ("projectileSpeed", 0x664, K::Int),
    ("projImpactExplode", 0x6a4, K::Bool),
    ("adsViewKickPitchMin", 0x814, K::Float),
    ("adsViewKickPitchMax", 0x818, K::Float),
    ("adsSpread", 0x830, K::Float),
    ("hipViewKickPitchMin", 0x85c, K::Float),
    ("hipViewKickPitchMax", 0x860, K::Float),
    ("minDamage", 0x914, K::Int),
    ("minPlayerDamage", 0x918, K::Int),
    ("maxDamageRange", 0x91c, K::Float),
    ("minDamageRange", 0x920, K::Float),
];

/// Offset of `locationDamageMultipliers[HITLOC_COUNT]`.
const LOCATION_MULTIPLIERS: usize = 0x930;

/// The numeric and enum fields of a `WeaponDef` header, as weapon-file
/// `(key, value)` pairs. `h` must be the whole 2476-byte struct.
pub fn stats(h: &[u8]) -> Vec<(&'static str, String)> {
    if h.len() < 0x9ac {
        return Vec::new();
    }
    let u = |o: usize| u32::from_le_bytes([h[o], h[o + 1], h[o + 2], h[o + 3]]);
    let mut out = Vec::with_capacity(FIELDS.len() + LOCATION_KEYS.len());
    for &(key, o, kind) in FIELDS {
        let v = match kind {
            K::Int => (u(o) as i32).to_string(),
            K::Ms => fmt_f32(u(o) as i32 as f32 / 1000.0),
            K::Float => fmt_f32(f32::from_bits(u(o))),
            K::Bool => (u(o) != 0).then_some("1").unwrap_or("0").to_string(),
            K::Enum(names) => match names.get(u(o) as usize) {
                Some(n) => n.to_string(),
                None => continue,
            },
        };
        out.push((key, v));
    }
    for (i, key) in LOCATION_KEYS.iter().enumerate() {
        out.push((key, fmt_f32(f32::from_bits(u(LOCATION_MULTIPLIERS + 4 * i)))));
    }
    out
}

/// Shortest text that reads back as the same `f32`.
fn fmt_f32(v: f32) -> String {
    format!("{v}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fields_in_file_units() {
        let mut h = vec![0u8; 2476];
        let put = |h: &mut Vec<u8>, o: usize, v: u32| h[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut h, 0x158, 1);
        put(&mut h, 0x420, 160);
        put(&mut h, 0x448, 283);
        put(&mut h, 0x91c, 256f32.to_bits());
        put(&mut h, 0x930 + 8, 3.5f32.to_bits());
        let s = stats(&h);
        let get = |k: &str| s.iter().find(|(f, _)| *f == k).map(|(_, v)| v.as_str());
        assert_eq!(get("fireType"), Some("Single Shot"));
        assert_eq!(get("damage"), Some("160"));
        assert_eq!(get("fireTime"), Some("0.283"));
        assert_eq!(get("maxDamageRange"), Some("256"));
        assert_eq!(get("locHead"), Some("3.5"));
        assert_eq!(get("boltAction"), Some("0"));
    }
}
