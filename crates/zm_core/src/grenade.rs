//! Hand grenades: stats (from the game's weapon file when available), the
//! zombie mode's grenade allowance, radius damage and bouncing.
//!
//! Distances here are game units (inches) unless noted.

use crate::geom::V3;
use crate::weaponfile::WeaponFile;

/// Stats of the thrown grenade.
#[derive(Debug, Clone, PartialEq)]
pub struct GrenadeDef {
    /// Seconds from pulling the pin to the explosion (`fuseTime`).
    pub fuse: f32,
    /// The fuse runs while the grenade is held (`cookOffHold`).
    pub cook: bool,
    /// `explosionRadius`, units.
    pub radius: f32,
    /// Damage at the centre and at the edge (`explosionInnerDamage` / `Outer`).
    pub inner_damage: f32,
    pub outer_damage: f32,
    /// Throw speed along the view and the extra up along the view's up
    /// (`projectileSpeed`, `projectileSpeedUp`), units per second.
    pub speed: f32,
    pub speed_up: f32,
    /// Most grenades carried (`maxAmmo`).
    pub max_ammo: u32,
    /// Pin-pull time before a throw can happen (`holdFireTime`), and the
    /// throw itself (`fireTime`), seconds.
    pub hold_time: f32,
    pub fire_time: f32,
    /// Fractions of the velocity kept along / against a surface on a bounce
    /// (`parallelDefaultBounce`, `perpendicularDefaultBounce`).
    pub parallel_bounce: f32,
    pub perpendicular_bounce: f32,
    pub projectile_model: Option<String>,
    pub world_model: Option<String>,
    /// Alias prefix of the bounce sounds (`bounceSound`, + `_<surface>`).
    pub bounce_sound: Option<String>,
    pub hud_icon: Option<String>,
}

impl Default for GrenadeDef {
    /// Stand-in stats when the game's weapon file is not available.
    fn default() -> Self {
        GrenadeDef {
            fuse: 3.5,
            cook: true,
            radius: 250.0,
            inner_damage: 200.0,
            outer_damage: 50.0,
            speed: 900.0,
            speed_up: 120.0,
            max_ammo: 4,
            hold_time: 0.4,
            fire_time: 0.4,
            parallel_bounce: 0.5,
            perpendicular_bounce: 0.25,
            projectile_model: None,
            world_model: None,
            bounce_sound: None,
            hud_icon: None,
        }
    }
}

impl GrenadeDef {
    /// Takes every stat the weapon file has; returns how many were applied.
    pub fn apply_weapon_file(&mut self, wf: &WeaponFile) -> usize {
        let mut n = 0;
        let mut f = |dst: &mut f32, key: &str| {
            if let Some(v) = wf.f32(key) {
                *dst = v;
                n += 1;
            }
        };
        f(&mut self.fuse, "fuseTime");
        f(&mut self.radius, "explosionRadius");
        f(&mut self.inner_damage, "explosionInnerDamage");
        f(&mut self.outer_damage, "explosionOuterDamage");
        f(&mut self.speed, "projectileSpeed");
        f(&mut self.speed_up, "projectileSpeedUp");
        f(&mut self.hold_time, "holdFireTime");
        f(&mut self.fire_time, "fireTime");
        f(&mut self.parallel_bounce, "parallelDefaultBounce");
        f(&mut self.perpendicular_bounce, "perpendicularDefaultBounce");
        if let Some(v) = wf.u32("maxAmmo") {
            self.max_ammo = v;
            n += 1;
        }
        if let Some(v) = wf.get("cookOffHold") {
            self.cook = v.trim() != "0";
            n += 1;
        }
        let mut s = |dst: &mut Option<String>, key: &str| {
            if let Some(v) = wf.get(key) {
                *dst = Some(v.trim().to_string());
                n += 1;
            }
        };
        s(&mut self.projectile_model, "projectileModel");
        s(&mut self.world_model, "worldModel");
        s(&mut self.bounce_sound, "bounceSound");
        s(&mut self.hud_icon, "hudIcon");
        n
    }

    /// Radius damage at `dist` units from the explosion: linear from the
    /// inner damage at the centre to the outer damage at the edge, none
    /// beyond it.
    pub fn damage_at(&self, dist: f32) -> Option<f32> {
        if dist >= self.radius || self.radius <= 0.0 {
            return None;
        }
        let k = (dist / self.radius).clamp(0.0, 1.0);
        Some(self.inner_damage + (self.outer_damage - self.inner_damage) * k)
    }

    /// The bounce sound alias for a surface (`grenade_bounce_default`...).
    pub fn bounce_alias(&self, surface: &str) -> Option<String> {
        self.bounce_sound.as_ref().map(|b| format!("{b}_{surface}"))
    }
}

/// Grenades a survivor holds after a round starts (`award_grenades_for_
/// survivors`): by the fraction of the maximum held, set to 2 (under a
/// quarter), 3 (under half) or 4 — i.e. +2, capped at four.
pub fn award_for_survivor(held: u32, max_ammo: u32) -> u32 {
    let frac = if max_ammo == 0 { 1.0 } else { held as f32 / max_ammo as f32 };
    let set = if frac < 0.25 {
        2
    } else if frac < 0.5 {
        3
    } else {
        4
    };
    // SetWeaponAmmoClip is capped by the clip size.
    set.min(max_ammo.max(held))
}

/// Velocity after hitting a surface with unit `normal`: the part along the
/// surface keeps `parallel`, the part into it reflects and keeps `perp`.
pub fn bounce(vel: V3, normal: V3, parallel: f32, perp: f32) -> V3 {
    let into = vel.dot(normal);
    let n_part = normal.scale(into);
    let t_part = vel.sub(n_part);
    t_part.scale(parallel).sub(n_part.scale(perp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn award_matches_the_script() {
        // maxAmmo 4: none -> 2, one -> 3, two or more -> 4.
        assert_eq!(award_for_survivor(0, 4), 2);
        assert_eq!(award_for_survivor(1, 4), 3);
        assert_eq!(award_for_survivor(2, 4), 4);
        assert_eq!(award_for_survivor(3, 4), 4);
        assert_eq!(award_for_survivor(4, 4), 4);
    }

    #[test]
    fn radius_damage_falls_off() {
        let g = GrenadeDef { radius: 256.0, inner_damage: 200.0, outer_damage: 50.0, ..Default::default() };
        assert_eq!(g.damage_at(0.0), Some(200.0));
        assert_eq!(g.damage_at(128.0), Some(125.0));
        assert_eq!(g.damage_at(256.0), None);
        assert_eq!(g.damage_at(300.0), None);
    }

    #[test]
    fn weapon_file_stats() {
        let wf = WeaponFile::parse(
            "WEAPONFILE\\fuseTime\\3.5\\explosionRadius\\256\\explosionInnerDamage\\200\\explosionOuterDamage\\50\\projectileSpeed\\940\\projectileSpeedUp\\120\\maxAmmo\\4\\cookOffHold\\1\\projectileModel\\some_model\\bounceSound\\some_bounce",
        )
        .unwrap();
        let mut g = GrenadeDef { cook: false, ..Default::default() };
        assert_eq!(g.apply_weapon_file(&wf), 10);
        assert_eq!(g.speed, 940.0);
        assert!(g.cook);
        assert_eq!(g.projectile_model.as_deref(), Some("some_model"));
        assert_eq!(g.bounce_alias("default").as_deref(), Some("some_bounce_default"));
    }

    #[test]
    fn bounce_keeps_fractions() {
        // Falling at 45 degrees onto a floor.
        let v = bounce(V3::new(1.0, -1.0, 0.0), V3::new(0.0, 1.0, 0.0), 0.5, 0.25);
        assert!((v.x - 0.5).abs() < 1e-6 && (v.y - 0.25).abs() < 1e-6 && v.z.abs() < 1e-6);
    }
}
