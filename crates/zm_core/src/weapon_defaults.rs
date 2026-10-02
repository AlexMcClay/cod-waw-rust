//! Placeholder weapon numbers, used only when no World at War install is
//! found. They are round, class-typical values picked by hand (not the
//! game's): with an install every weapon reads its real numbers from the
//! game's own weapon files and the map's zone.
//!
//! Each weapon is a class template plus what makes it work the way it does
//! (fire mode, bolt action, pellets, shell-by-shell reloads, its real-world
//! magazine size), as `key\\value` pairs in weapon-file units (inches,
//! seconds), so they go through the same parser as the real files.

/// A weapon class: the template its placeholder numbers start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Pistol,
    Magnum,
    SemiRifle,
    BoltRifle,
    Sniper,
    AntiTank,
    Smg,
    AssaultRifle,
    Lmg,
    Shotgun,
    Wonder,
}

const COMMON: &str = "\
    weaponType\\bullet\\fireType\\Single Shot\\boltAction\\0\\penetrateType\\medium\\\
    meleeDamage\\125\\meleeTime\\0.5\\meleeDelay\\0.1\\\
    locNone\\1\\locHelmet\\3\\locHead\\3\\locNeck\\2\\locTorsoUpper\\1.2\\locTorsoLower\\1\\\
    locRightArmUpper\\0.8\\locLeftArmUpper\\0.8\\locRightArmLower\\0.7\\locLeftArmLower\\0.7\\locRightHand\\0.5\\locLeftHand\\0.5\\\
    locRightLegUpper\\0.8\\locLeftLegUpper\\0.8\\locRightLegLower\\0.7\\locLeftLegLower\\0.7\\locRightFoot\\0.5\\locLeftFoot\\0.5\\locGun\\0\\\
    rechamberTime\\0\\shotCount\\1\\\
    reloadEmptyAddTime\\0\\reloadStartTime\\0\\reloadStartAddTime\\0\\reloadEndTime\\0\\segmentedReload\\0\\noPartialReload\\0\\reloadAmmoAdd\\0\\reloadStartAdd\\0\\\
    hipSpreadStandMin\\2\\hipSpreadMax\\6\\hipSpreadDuckedMin\\1.5\\hipSpreadDuckedMax\\5\\hipSpreadProneMin\\1\\hipSpreadProneMax\\4\\adsSpread\\0.5\\\
    hipSpreadFireAdd\\1\\hipSpreadMoveAdd\\4\\hipSpreadTurnAdd\\0\\hipSpreadDecayRate\\5\\hipSpreadDuckedDecay\\1\\hipSpreadProneDecay\\1\\\
    adsTransInTime\\0.25\\adsTransOutTime\\0.25\\adsZoomFov\\50\\moveSpeedScale\\1\\adsMoveSpeedScale\\1\\sprintDurationScale\\1\\\
    raiseTime\\0.5\\dropTime\\0.4\\firstRaiseTime\\0.8\\hipViewKickPitchMin\\30\\hipViewKickPitchMax\\40";

fn template(class: Class) -> &'static str {
    match class {
        Class::Pistol => "\
            weaponClass\\pistol\\penetrateType\\small\\damage\\30\\minDamage\\15\\maxDamageRange\\400\\minDamageRange\\1200\\\
            fireTime\\0.12\\reloadTime\\2\\reloadEmptyTime\\2.5\\reloadAddTime\\1.5\\adsZoomFov\\55\\adsMoveSpeedScale\\1.2",
        Class::Magnum => "\
            weaponClass\\pistol\\penetrateType\\large\\damage\\300\\minDamage\\150\\maxDamageRange\\500\\minDamageRange\\1500\\\
            fireTime\\0.35\\reloadTime\\3\\reloadEmptyTime\\3\\reloadAddTime\\2.5\\adsZoomFov\\55",
        Class::SemiRifle => "\
            weaponClass\\rifle\\damage\\80\\minDamage\\60\\maxDamageRange\\1000\\minDamageRange\\2500\\\
            fireTime\\0.15\\reloadTime\\2.8\\reloadEmptyTime\\2.8\\reloadAddTime\\2\\adsZoomFov\\45\\moveSpeedScale\\0.95",
        Class::BoltRifle => "\
            weaponClass\\rifle\\boltAction\\1\\damage\\120\\minDamage\\100\\maxDamageRange\\1500\\minDamageRange\\3000\\\
            fireTime\\0.3\\rechamberTime\\1\\reloadTime\\2.5\\reloadEmptyTime\\2.5\\reloadAddTime\\1.8\\adsZoomFov\\45\\moveSpeedScale\\0.95",
        Class::Sniper => "\
            weaponClass\\rifle\\boltAction\\1\\penetrateType\\large\\damage\\400\\minDamage\\300\\maxDamageRange\\2000\\minDamageRange\\4000\\\
            fireTime\\0.3\\rechamberTime\\1.1\\reloadTime\\3\\reloadEmptyTime\\3\\reloadAddTime\\2.2\\adsZoomFov\\15\\adsTransInTime\\0.35\\moveSpeedScale\\0.9",
        Class::AntiTank => "\
            weaponClass\\rifle\\penetrateType\\large\\damage\\1000\\minDamage\\800\\maxDamageRange\\2000\\minDamageRange\\4000\\\
            fireTime\\0.5\\reloadTime\\3.5\\reloadEmptyTime\\3.5\\reloadAddTime\\2.5\\adsZoomFov\\15\\adsTransInTime\\0.4\\moveSpeedScale\\0.8",
        Class::Smg => "\
            weaponClass\\smg\\fireType\\Full Auto\\penetrateType\\small\\damage\\100\\minDamage\\70\\maxDamageRange\\300\\minDamageRange\\1500\\\
            fireTime\\0.09\\reloadTime\\2.4\\reloadEmptyTime\\2.9\\reloadAddTime\\1.7\\adsZoomFov\\55",
        Class::AssaultRifle => "\
            weaponClass\\mg\\fireType\\Full Auto\\damage\\110\\minDamage\\80\\maxDamageRange\\600\\minDamageRange\\2000\\\
            fireTime\\0.1\\reloadTime\\2.6\\reloadEmptyTime\\3.1\\reloadAddTime\\1.9\\adsZoomFov\\50\\moveSpeedScale\\0.95",
        Class::Lmg => "\
            weaponClass\\mg\\fireType\\Full Auto\\penetrateType\\large\\damage\\120\\minDamage\\90\\maxDamageRange\\800\\minDamageRange\\2500\\\
            fireTime\\0.1\\reloadTime\\4\\reloadEmptyTime\\4\\reloadAddTime\\3\\adsZoomFov\\50\\adsTransInTime\\0.4\\moveSpeedScale\\0.8",
        Class::Shotgun => "\
            weaponClass\\spread\\penetrateType\\small\\shotCount\\8\\damage\\100\\minDamage\\10\\maxDamageRange\\200\\minDamageRange\\700\\\
            fireTime\\0.25\\reloadTime\\2.5\\reloadEmptyTime\\2.5\\reloadAddTime\\2\\adsZoomFov\\55\\\
            hipSpreadStandMin\\5\\hipSpreadMax\\8\\adsSpread\\4",
        Class::Wonder => "\
            weaponClass\\pistol\\weaponType\\projectile\\fireType\\Full Auto\\damage\\900\\explosionRadius\\80\\explosionInnerDamage\\1200\\explosionOuterDamage\\200\\\
            projectileSpeed\\3000\\fireTime\\0.3\\reloadTime\\3\\reloadEmptyTime\\3\\reloadAddTime\\2\\adsZoomFov\\55",
    }
}

/// The placeholder weapon file for a class, with `extra` (`key\\value`
/// pairs) on top: later keys win.
pub fn placeholder(class: Class, extra: &str) -> String {
    let mut s = format!("{COMMON}\\{}", template(class));
    if !extra.is_empty() {
        s.push('\\');
        s.push_str(extra);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_key_value_pairs() {
        for c in [
            Class::Pistol,
            Class::Magnum,
            Class::SemiRifle,
            Class::BoltRifle,
            Class::Sniper,
            Class::AntiTank,
            Class::Smg,
            Class::AssaultRifle,
            Class::Lmg,
            Class::Shotgun,
            Class::Wonder,
        ] {
            let s = placeholder(c, "clipSize\\5");
            assert_eq!(s.split('\\').count() % 2, 0, "{c:?}");
        }
    }
}
