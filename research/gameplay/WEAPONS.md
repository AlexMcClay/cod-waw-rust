# World at War Zombies weapons (Nacht der Untoten)

Every gameplay number the game uses for a weapon, where it comes from, and
how *Undead Rounds* applies it. The tables are generated from the install by
`gen_weapon_defaults.py` (it also writes the no-install fallback table
`crates/zm_core/src/weapon_defaults.rs`).

## Sources, in order of authority

1. **Zone `WeaponDef`** (`zone/english/nazi_zombie_prototype.ff`, compiled
   weapons; struct layout in `research/t4/T4_LAYOUTS.txt`, 2476 bytes). This is
   what the shipping game runs on. `waw_assets::t4::weapondef::stats` reads
   ~80 gameplay fields plus the 19 `locationDamageMultipliers` and returns
   them under the weapon-file key names and units (ms -> s, enums -> words).
   `cargo run -p waw_assets --example weaponstats` prints them and diffs them
   against the IWD files.
2. **IWD weapon files** `weapons/sp/<name>` (all in `main/iw_14.iwd`; no later
   archive overrides any of them). Plain `WEAPONFILE\key\value...` text.
3. **Zombie scripts** (`nazi_zombie_prototype.gsc`, `_zombiemode_*.gsc`) for
   which weapons exist and what the scripts add on top of engine damage.
4. **Wiki** (callofduty / nazizombies fandom) only as a cross-check; the
   pages themselves block automated fetches (HTTP 402/403), so only search
   snippets were available.

**Zone vs IWD: identical.** For all 32 weapons in Nacht's zone, every field
that the IWD file sets has exactly the same value in the zone WeaponDef
(checked field by field with the `weaponstats` example; zero differences).
Fields a file omits are 0 in the zone (e.g. the Ray Gun has no `fireType`,
so it is `Full Auto`; no hit-location multipliers, so they are all 0, and no
damage ranges, so `999999`).

The game applies them in that order too: the weapon file at startup (so the
bunker prototype map has them), then, when Nacht's zones are read, the zone
WeaponDef over it (`audio::apply_zone_weapon_stats`, which logs any weapon
whose numbers change; none do). Without an install the built-in table (the
same numbers) is used.

## Nacht's weapons

`include_weapons()` in `nazi_zombie_prototype.gsc` (the box picks uniformly):
sw_357, m1carbine, m1garand, gewehr43, stg44, thompson, mp40, kar98k,
springfield, ptrs41_zombie, kar98k_scoped_zombie, molotov, stielhandgranate,
m1garand_gl, m7_launcher (included, never `add_zombie_weapon`ed),
m2_flamethrower_zombie, doublebarrel, doublebarrel_sawed_grip, shotgun,
fg42_bipod, mg42_bipod, 30cal_bipod, bar, panzerschrek, ray_gun. The player
starts with `zombie_colt`.

Implemented (hitscan): all of the above except the molotov and
stielhandgranate (grenades, handled by `grenades.rs`), the panzerschrek
(rocket projectile), the M2 flamethrower (gas weapon) and the m7_launcher.
Added in this pass: **sw_357, gewehr43, springfield, ptrs41_zombie,
m1garand_gl** (the Garand half; its rifle grenade alt-weapon is not
modelled), **fg42_bipod, mg42_bipod, 30cal_bipod** (fired from the hip and
sights; the bipod deploy is not modelled). Their zone names are their ids, so
they join the box automatically. The PPSh stays in the table (later maps)
but is not in Nacht's box.

## Values (from the weapon files = zone)

Units: weapon files use inches (1 in = 0.0254 m) and seconds. RPM is
`60 / (fireTime + rechamberTime)` for bolt-action/pump guns, `60 / fireTime`
otherwise. `*` = not in the file; the engine default (zone value) applies.

### Damage and fire

| Weapon (file) | Class | Fire | Damage | Min dmg | Full dmg to (in / m) | Min dmg from (in / m) | Pellets | fireTime (s) | Rechamber (s) | RPM | Penetrate |
|---|---|---|---|---|---|---|---|---|---|---|---|
| m1911 (`zombie_colt`) | pistol | Single Shot | 20 | 20 | 425 / 10.8 | 1000 / 25.4 | 1 | 0.075 | - | 800 | small |
| sw_357 (`sw_357`) | pistol | Single Shot | 1000 | 300 | 384 / 9.8 | 1500 / 38.1 | 1 | 0.32 | - | 188 | large |
| m1carbine (`m1carbine`) | rifle | Single Shot | 68 | 50 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.135 | - | 444 | medium |
| m1garand (`m1garand`) | rifle | Single Shot | 68 | 50 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.135 | - | 444 | medium |
| m1garand_gl (`m1garand_gl`) | rifle | Single Shot | 68 | 50 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.135 | - | 444 | medium |
| gewehr43 (`gewehr43`) | rifle | Single Shot | 90 | 50 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.125 | - | 480 | medium |
| kar98k (`kar98k`) | rifle | Single Shot | 100 | 80 | 1200 / 30.5 | 2200 / 55.9 | 1 | 0.33 | 1.0 | 45 | medium |
| springfield (`springfield`) | rifle | Single Shot | 100 | 80 | 1200 / 30.5 | 2200 / 55.9 | 1 | 0.33 | 1.0 | 45 | medium |
| kar98k_scoped_zombie (`kar98k_scoped_zombie`) | rifle | Single Shot | 300 | 300 | 4000 / 101.6 | 5000 / 127.0 | 1 | 0.33 | 1.2 | 39 | medium |
| ptrs41_zombie (`ptrs41_zombie`) | rifle | Single Shot | 1000 | 1000 | 4000 / 101.6 | 5000 / 127.0 | 1 | 0.8 | - | 75 | large |
| thompson (`thompson`) | smg | Full Auto | 120 | 80 | 800 / 20.3 | 1800 / 45.7 | 1 | 0.08 | - | 750 | medium |
| mp40 (`mp40`) | smg | Full Auto | 100 | 60 | 800 / 20.3 | 1800 / 45.7 | 1 | 0.112 | - | 536 | small |
| stg44 (`stg44`) | smg | Full Auto | 100 | 70 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.112 | - | 536 | medium |
| ppsh (`ppsh`) | smg | Full Auto | 100 | 60 | 800 / 20.3 | 1500 / 38.1 | 1 | 0.048 | - | 1250 | small |
| bar (`bar`) | mg | Full Auto | 100 | 85 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.16 | - | 375 | medium |
| fg42_bipod (`fg42_bipod`) | mg | Full Auto | 100 | 85 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.064 | - | 938 | small |
| mg42_bipod (`mg42_bipod`) | mg | Full Auto | 130 | 90 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.064 | - | 938 | large |
| 30cal_bipod (`30cal_bipod`) | mg | Full Auto | 130 | 90 | 1024 / 26.0 | 2400 / 61.0 | 1 | 0.096 | - | 625 | large |
| trenchgun (`shotgun`) | spread | Single Shot | 160 | 15 | 256 / 6.5 | 800 / 20.3 | 8 | 0.283 | 0.8 | 55 | medium |
| doublebarrel (`doublebarrel`) | spread | Single Shot | 200 | 15 | 256 / 6.5 | 800 / 20.3 | 8 | 0.283 | - | 212 | medium |
| doublebarrel_sawed_grip (`doublebarrel_sawed_grip`) | spread | Single Shot | 200 | 15 | 256 / 6.5 | 800 / 20.3 | 8 | 0.283 | - | 212 | medium |
| raypistol (`ray_gun`) | pistol (projectile) | Full Auto* | 1000 | - | - | - | 1 | 0.33 | - | 182 | - |

Ray Gun projectile: `projectileSpeed` 3200 in/s (81 m/s), explodes on impact
(`projImpactExplode`), `explosionRadius` 64 in (1.63 m), `explosionInnerDamage`
1500 at the centre to `explosionOuterDamage` 300 at the edge. All weapons:
`meleeDamage` 150, `meleeTime` 0.5 s, `meleeDelay` 0.05 s.

### Hit-location multipliers (left = right for every weapon)

| Weapon | Helmet | Head | Neck | Torso up | Torso low | Arm up | Arm low | Hand | Leg up | Leg low | Foot |
|---|---|---|---|---|---|---|---|---|---|---|---|
| m1911 | 3.5 | 3.5 | 3.5 | 1.25 | 1.1 | 0.85 | 0.6 | 0.35 | 0.85 | 0.6 | 0.35 |
| sw_357 | 1.5 | 1.5 | 1.5 | 1.1 | 0.9 | 0.8 | 0.8 | 0.8 | 0.5 | 0.4 | 0.1 |
| m1carbine | 2.7 | 2.7 | 2.7 | 1.25 | 1.25 | 0.8 | 0.7 | 0.6 | 0.8 | 0.7 | 0.6 |
| m1garand | 2.7 | 2.7 | 2.7 | 1.25 | 1.25 | 0.8 | 0.7 | 0.6 | 0.8 | 0.7 | 0.6 |
| m1garand_gl | 2.7 | 2.7 | 2.7 | 1.25 | 1.25 | 0.8 | 0.7 | 0.6 | 0.8 | 0.7 | 0.6 |
| gewehr43 | 2.7 | 2.7 | 2.7 | 1.25 | 1.25 | 0.8 | 0.7 | 0.6 | 0.8 | 0.7 | 0.6 |
| kar98k | 1 | 3.5 | 3.5 | 1.8 | 1.7 | 1 | 1 | 0.7 | 1 | 1 | 0.7 |
| springfield | 0.5 | 2 | 2 | 1.2 | 0.9 | 0.8 | 0.6 | 0.1 | 0.5 | 0.4 | 0.1 |
| kar98k_scoped_zombie | 3 | 10 | 5 | 2.25 | 2.25 | 0.8 | 0.8 | 0.8 | 0.8 | 0.8 | 0.8 |
| ptrs41_zombie | 3 | 10 | 5 | 2.25 | 2.25 | 2 | 1 | 1 | 1 | 1 | 1 |
| thompson | 4 | 4 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| mp40 | 4 | 4 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| stg44 | 2.7 | 2.7 | 2.7 | 1 | 0.9 | 0.6 | 0.5 | 0.3 | 0.6 | 0.5 | 0.3 |
| ppsh | 4 | 4 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| bar | 2.7 | 2.7 | 2.7 | 2 | 2 | 1.7 | 1.5 | 1.4 | 1.7 | 1.5 | 1.4 |
| fg42_bipod | 2.7 | 2.7 | 2.7 | 2 | 2 | 1.7 | 1.5 | 1.4 | 1.7 | 1.5 | 1.4 |
| mg42_bipod | 3 | 3 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| 30cal_bipod | 3 | 3 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| trenchgun | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| doublebarrel | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| doublebarrel_sawed_grip | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| raypistol | 0* | 0* | 0* | 0* | 0* | 0* | 0* | 0* | 0* | 0* | 0* |

`locNone` is 1 and `locGun` 0 for every bullet weapon. The Ray Gun's are all
0 in the zone; its impact damage is applied unscaled (see below).

### Ammo and reload (seconds)

`maxAmmo` is the reserve ("stock") on top of the clip. Normal reloads put the
magazine in at "Add at"; empty reloads at "Empty add at" (see the note on
`reloadStartAddTime`). An add time of 0 or past the end means at the end.

| Weapon | Clip | Max (stock) | Start | Reload | Add at | Empty reload | Empty add at (`reloadStartAddTime`) | Segmented: start / start add / end | Rounds per loop / in start |
|---|---|---|---|---|---|---|---|---|---|
| m1911 | 8 | 80 | 40 | 1.9 | 1.4 | 2.65 | 1.65 | - | - |
| sw_357 | 6 | 80 | 80 | 3 | 3.5 | 3 | 3.5 | - | - |
| m1carbine | 15 | 120 | 120 | 2.9 | 2.2 | 3.7 | 2.3 | - | - |
| m1garand | 8 | 128 | 96 | 3.4 | 2.5 | 1.6 | 0 | - | - |
| m1garand_gl | 8 | 128 | 96 | 3.4 | 2.5 | 1.6 | 0 | - | - |
| gewehr43 | 10 | 120 | 96 | 3 | 2.05 | 4 | 2.35 | - | - |
| kar98k | 5 | 50 | 50 | 2.5 | 1.65 | 2.5 | 1.65 | - | - |
| springfield | 5 | 50 | 50 | 3.2 | 2 | 3.2 | 0 | - | - |
| kar98k_scoped_zombie | 5 | 50 | 50 | 0.6 per round | 0.2 | - | - | 1.8 / 1.4 / 0.77 | 1 / 1 |
| ptrs41_zombie | 5 | 60 | 30 | 4 | 2.5 | 5.3 | 0 | - | - |
| thompson | 20 | 200 | 200 | 2.1 | 1.6 | 2.45 | 1.2 | - | - |
| mp40 | 32 | 192 | 192 | 2.3 | 1.85 | 2.9 | 1.85 | - | - |
| stg44 | 30 | 180 | 180 | 2.15 | 1.4 | 2.8 | 1.4 | - | - |
| ppsh | 71 | 355 | 355 | 2.1 | 1.9 | 2.8 | 1.7 | - | - |
| bar | 20 | 140 | 140 | 2.75 | 1.95 | 3.25 | 1.75 | - | - |
| fg42_bipod | 32 | 192 | 192 | 2.4 | 1.85 | 3.5 | 1.75 | - | - |
| mg42_bipod | 125 | 500 | 500 | 4.5 | 2 | 4.5 | 2 | - | - |
| 30cal_bipod | 125 | 500 | 500 | 7 | 4.75 | 6 | 0 | - | - |
| trenchgun | 6 | 60 | 60 | 0.6 per shell | 0 (end) | - | - | 0.9 / 0.75 / 0.95 | 1 / 1 |
| doublebarrel | 2 | 60 | 60 | 3 | 2.65 | 4 | 0 | - | - |
| doublebarrel_sawed_grip | 2 | 60 | 60 | 3 | 2.65 | 4 | 0 | - | - |
| raypistol | 20 | 160 | 160 | 3 | 0 (end) | - | - | - | - |

### Spread (degrees), ADS and handling

| Weapon | Hip stand min-max | Ducked | Prone | ADS | Fire add | Move add | Decay /s | ADS in / out (s) | ADS FOV | Move speed | ADS move | Sprint dur. | Raise / drop / first raise (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| m1911 | 3-6 | 2.5-5 | 2-4 | 0.2 | 1 | 4.5 | 4 | 0.245 / 0.25 | 60 | 1 | 1.5 | 1.2 | 0.4 / 0.3 / 0.83 |
| sw_357 | 2-4 | 1.5-3 | 1-2 | 1.15 | 1 | 4.5 | 4 | 0.245 / 0.25 | 60 | 0.9 | 1.2 | 1.2 | 0.4 / 0.4 / 0.83 |
| m1carbine | 1-5 | 0.75-4 | 0.5-3 | 0 | 0.6 | 5 | 4 | 0.3 / 0.4 | 45 | 0.9 | 0.9 | 0.9 | 0.5 / 0.4 / 1.4 |
| m1garand | 1-5 | 0.75-4 | 0.5-3 | 0.2 | 0.6 | 5 | 4 | 0.25 / 0.25 | 40 | 0.9 | 0.9 | 0.9 | 0.8 / 0.25 / 1 |
| m1garand_gl | 1-5 | 0.75-4 | 0.5-3 | 0.2 | 0.6 | 5 | 4 | 0.25 / 0.25 | 40 | 0.9 | 0.9 | 0.9 | 0.8 / 0.25 / 1 |
| gewehr43 | 1-5 | 0.75-4 | 0.5-3 | 0 | 0.6 | 5 | 4 | 0.3 / 0.4 | 40 | 0.9 | 0.9 | 0.9 | 0.6 / 0.5 / 1 |
| kar98k | 8-10 | 7.5-9.5 | 7-9 | 0.05 | 1 | 5 | 5 | 0.2 / 0.4 | 40 | 0.9 | 0.9 | 0.9 | 0.5 / 0.4 / 0.9 |
| springfield | 8-10 | 7.5-9.5 | 7-9 | 0.05 | 1 | 5 | 5 | 0.2 / 0.4 | 40 | 0.9 | 0.9 | 0.9 | 0.5 / 0.4 / 0.75 |
| kar98k_scoped_zombie | 8-10 | 7.5-9.5 | 7-9 | 0 | 1 | 5 | 5 | 0.3 / 0.4 | 10 | 0.9 | 0.75 | 0.9 | 0.7 / 0.5 / 0.85 |
| ptrs41_zombie | 8-10 | 7.5-9.5 | 7-9 | 0 | 1 | 5 | 5 | 0.4 / 0.6 | 10 | 0.75 | 0.75 | 0.75 | 1.1 / 0.85 / 1.1 |
| thompson | 1.5-6 | 1.25-5 | 1-4 | 0 | 0.52 | 4 | 4 | 0.22 / 0.3 | 55 | 1 | 1.3 | 1 | 0.44 / 0.38 / 0.6 |
| mp40 | 1.5-6 | 1.25-5 | 1-4 | 0 | 0.52 | 4 | 4 | 0.25 / 0.3 | 55 | 1 | 1.3 | 1 | 0.35 / 0.3 / 1.33 |
| stg44 | 2-8 | 1.8-6.5 | 1.5-5 | 0 | 0.56 | 5 | 4 | 0.35 / 0.4 | 45 | 1 | 1.3 | 1 | 0.6 / 0.4 / 1 |
| ppsh | 2-5 | 1.75-4.5 | 1.5-4 | 0 | 0.52 | 4 | 4 | 0.2 / 0.28 | 55 | 1 | 1.3 | 1 | 0.44 / 0.38 / 0.65 |
| bar | 2-8 | 1.8-6.5 | 1.5-5 | 0.4 | 0.56 | 5 | 4 | 0.32 / 0.37 | 35 | 1 | 0.85 | 0.9 | 0.6 / 0.4 / 0.65 |
| fg42_bipod | 2-8 | 1.8-6.5 | 1.5-5 | 0.4 | 0.56 | 5 | 4 | 0.22 / 0.4 | 35 | 0.8 | 0.8 | 0.8 | 0.7 / 0.5 / 0.7 |
| mg42_bipod | 3.7-6 | 2.5-5 | 1-4 | 0 | 0.6 | 5 | 4 | 0.7 / 0.9 | 50 | 0.75 | 0.75 | 0.75 | 0.8 / 0.8 / 0.8 |
| 30cal_bipod | 4-10 | 3.5-8 | 3-6 | 0 | 0.6 | 5 | 4 | 0.22 / 0.4 | 50 | 0.75 | 0.75 | 0.75 | 0.9 / 0.83 / 1.5 |
| trenchgun | 4-4 | 4-4 | 4-4 | 5.5 | 0 | 0.1 | 5 | 0.24 / 0.29 | 55 | 1 | 1.2 | 1 | 0.6 / 0.38 / 0.6 |
| doublebarrel | 4-4 | 4-4 | 4-4 | 5.5 | 0 | 0.1 | 5 | 0.24 / 0.29 | 55 | 1 | 1.2 | 1 | 0.6 / 0.38 / 0.6 |
| doublebarrel_sawed_grip | 6-6 | 4-4 | 4-4 | 6.5 | 0 | 0.1 | 5 | 0.24 / 0.29 | 55 | 1 | 1.2 | 1 | 0.6 / 0.38 / 0.6 |
| raypistol | 1-2 | 1-2 | 1-2 | 0 | 1 | 0.5 | 3.25 | 0.245 / 0.25 | 60 | 1 | 1 | 1 | 0.8 / 0.8 / 0.8 |

### Bullet penetration (common.ff `info/bullet_penetration_sp`)

Flesh: small 32 in, medium 96 in, large 128 in (read from common.ff when
Nacht loads; the same numbers are the built-in fallback). Other surfaces
(bark, brick, wood, metal...) are in the same table but walls are not
penetrated here.

## Script-level rules (Nacht scripts)

* **No script damage scaling for bullets.** `_zombiemode_spawner.gsc`
  `zombie_damage()` only awards points for bullet hits; damage is the
  engine's: weapon damage at range x hit-location multiplier.
* **Grenades** (`MOD_GRENADE(_SPLASH)`): extra `DoDamage(round +
  RandomInt(100, 500))` after the engine damage. **Projectiles**
  (`MOD_PROJECTILE`, `MOD_PROJECTILE_SPLASH`, `MOD_EXPLOSIVE` - the Ray Gun's
  impact and splash, the Panzerschreck): extra `DoDamage(round *
  RandomInt(100, 500))`. GSC `RandomInt` takes one argument, so this is
  `RandomInt(100)` = 0..99 (as `grenades.rs` already assumes).
* **Insta-kill**: `check_for_instakill` gibs the head and does `health + 666`.
* **Head gib** (`head_should_gib`): zombie at <= 10 % health, rifle/pistol
  bullet (or a shotgun), hit location head/helmet/neck, and the weapon is
  not pistol class (so never the Colt, .357 or Ray Gun) and not a gas weapon.
  Grenades within 55 units of `j_head` and projectiles within 10 units also
  gib.
* **Points** (`_zombiemode_score.gsc`, for the rules owner): hit 5 (10 after
  `round_up_to_ten`), ADS hit `Int(5 * 1.25)` = 6 -> also 10; kill 50 plus
  bonus by hit location: head/helmet +50, **neck +20**, upper/lower torso
  +10, limbs +0; melee +80, burn +10.
* **Ammo**: `weapon_give` = `GiveWeapon` + `GiveMaxAmmo` (full clip + full
  `maxAmmo` reserve); wall ammo and the Max Ammo power-up = `GiveMaxAmmo`
  (reserve to `maxAmmo`, clip unchanged). The spawn Colt gets `startAmmo`
  40 = 8 + 32.

## How Undead Rounds applies them

* **Damage**: `damage` up to `maxDamageRange`, linear down to `minDamage` at
  `minDamageRange` (`WeaponDef::damage_at`), times the multiplier of the hit
  location (`WeaponDef::bullet_damage`). `zombies::hit_location` classifies a
  hit on the zombie's head sphere and body box into head / neck / upper and
  lower torso / upper and lower arm / hand / upper and lower leg / foot (left
  or right) from the hit's height and side in the zombie's own frame (an
  approximation of the engine's per-bone hitboxes; helmets are not
  distinguished from heads because Nacht's zombies are bare-headed).
* **Shotguns**: `shotCount` pellets, each a separate hit (damage and hit
  points per pellet), and pellets stop at `minDamageRange` (800 in = 20 m;
  IW3 traces pellets only that far).
* **Penetration**: a bullet goes through bodies until it has used up its
  flesh depth (torso 12 in, head 8, upper leg 7, other limbs 4 per body);
  damage scales with the depth left. Approximation of IW3's per-surface
  penetration (exact entry/exit depths are not traced).
* **Fire timing**: one shot per `fireTime`; bolt-action/pump guns
  (`boltAction 1`) add `rechamberTime` after a shot that leaves a round in the
  clip (cycle = fire + rechamber; the fire and rechamber animations play in
  sequence). `fireType` Single Shot = semi, Full Auto = auto, N-Round Burst
  supported.
* **Reloads**: normal reloads take `reloadTime` (`reloadEmptyTime` from an
  empty clip) and the rounds go in at the add time, so knifing/switching
  after it keeps them ("reload cancel"). Segmented reloads (trench gun,
  scoped Kar98k): start (`reloadStartTime`, loads `reloadStartAdd` rounds at
  `reloadStartAddTime`), then per round `reloadTime` (round in at
  `reloadAddTime`, end of the loop when 0), then `reloadEndTime`. Firing
  during the start/loop skips to the end phase (the pump), then fires.
* **Spread**: the engine's aim-spread scale: `hipSpreadFireAdd` per shot,
  `hipSpreadMoveAdd` per second while moving (scaled by speed / 190),
  `hipSpreadTurnAdd` while turning, otherwise decay at `hipSpreadDecayRate`
  per second (x `hipSpreadDuckedDecay` / `hipSpreadProneDecay`). Cone =
  stance min + (max - min) x scale, lerped to `adsSpread` by the aim amount.
  The direction is a random angle and a uniform radius (bunched to the
  centre, like IW3's `RandomBulletDir`). The crosshair follows the cone.
* **Switching**: `dropTime` of the old weapon + `raiseTime` of the new one; a
  newly bought weapon comes up in `firstRaiseTime`.
* **Melee**: the held weapon's `meleeDamage` (150) and `meleeTime` (0.5 s).
* **Ray Gun**: hitscan (not yet a 81 m/s projectile); the direct hit does
  1000 unscaled by location, then the explosion at the impact point does 1500
  -> 300 over 1.63 m to every zombie in view of it (the hit one included),
  each followed by the script's `round * RandomInt(100)`.
* **Handling** for the player controller: `weapons::HeldWeapon` (resource,
  updated every frame) exposes `moveSpeedScale`, `adsMoveSpeedScale`,
  `sprintDurationScale`, `adsTransInTime`/`OutTime` and `adsZoomFov` (with
  `ads_fov(base)` = base x zoom / 65, the game's `cg_fov`). `player.rs` uses
  them: aim in/out times, the aimed FOV, sprint length, and speed = hip
  `moveSpeedScale` lerped to 0.6 x `adsMoveSpeedScale` by the aim amount (the
  0.6 global aimed slow-down is ASSUMED; the pistol's 1.5 implies one exists).

## Discrepancies and uncertainties

* **Wiki vs data, trench gun**: the wiki says 8 pellets x 30 damage to 7.5 m,
  falling to 10 at its effective range. Those are the **multiplayer** file's
  numbers. Zombies (SP `weapons/sp/shotgun`) is 8 x **160** to 256 in (6.5 m)
  falling to **15** at 800 in (20.3 m), and multipliers are 1 everywhere
  (no headshot bonus for shotguns).
* **Wiki vs data, Ray Gun**: the wiki's "1000 impact, 1500-300 splash,
  automatic, 181 RPM, 20 / 160, 3 s reload" matches the data exactly
  (Full Auto because the file sets no `fireType`).
* **Empty-reload add time**: T4's WeaponDef has a `reloadEmptyAddTime`, but
  every weapon file sets it to 0 and instead gives `reloadStartAddTime` for
  non-segmented guns, with values that fit the empty reload (e.g. MP40 empty
  2.9 s, 1.85; Thompson empty 2.45 s, 1.2). We use `reloadEmptyAddTime` if
  set, else `reloadStartAddTime`, else `reloadAddTime` (if it is within the
  empty reload), else the end (the Garand: empty reload 1.6 s, add 2.5 ->
  1.6). Which one the T4 engine really reads for empty reloads is not
  confirmed.
* **Add time past the end**: the .357's `reloadAddTime` (3.5) is longer than
  its `reloadTime` (3.0); we load at the end.
* **Shotgun ADS spread**: `adsSpread` (5.5 / 6.5) is *wider* than the hip
  spread (4 / 6). We lerp to it like any weapon; whether IW3 uses the ADS
  value for pellets is not confirmed.
* **Spread-scale units**: the engine's aim-spread scale is 0..255 internally;
  the files' add/decay rates are treated as fractions of full per shot / per
  second, which matches how quickly the hip crosshair opens in the game but
  is not verified against the engine code.
* **Rechamber timing**: modelled as `fireTime` then `rechamberTime`
  (bolt-action RPM 45 for the Kar98k); `rechamberBoltTime` (when the brass
  ejects) is not used.
* **Zombie hitboxes**: height/side bands on a box, not the model's bone
  hitboxes, so limb hits are coarse (arms held out in front are counted as
  torso).

## Before -> after (key numbers)

Before this pass only the Colt, scoped Kar98k and Ray Gun read their weapon
files; the rest used made-up values, a single head multiplier and one reload
time. Old values are the effective ones in game (file-applied where a file
was read).

| Weapon | Damage (old -> new) | Head mult | Cycle s | Reload s (partial / empty) | Clip / reserve |
|---|---|---|---|---|---|
| M1911 | 20 -> 20 | 2.5 -> 3.5 | 0.075 | 1.9 -> 1.9 / 2.65 | 8/80 (start 8/32) |
| Kar98k | 100-60 -> 100-80 | 4 -> 3.5 | 1.1 -> 1.33 | 2.6 -> 2.5 | 5/45 -> 5/50 |
| M1A1 Carbine | 50 -> 68-50 | 2.5 -> 2.7 | 0.14 -> 0.135 | 2.2 -> 2.9 / 3.7 | 15/135 -> 15/120 |
| M1 Garand | 90 -> 68-50 | 3 -> 2.7 | 0.16 -> 0.135 | 2.4 -> 3.4 / 1.6 | 8/88 -> 8/128 |
| Thompson | 40 -> 120-80 | 2.5 -> 4 (neck 1) | 0.085 -> 0.08 | 2.3 -> 2.1 / 2.45 | 20/180 -> 20/200 |
| MP40 | 35 -> 100-60 | 2.5 -> 4 (neck 1) | 0.1 -> 0.112 | 2.4 -> 2.3 / 2.9 | 32/160 -> 32/192 |
| STG-44 | 55 -> 100-70 | 2.5 -> 2.7 | 0.1 -> 0.112 | 2.5 -> 2.15 / 2.8 | 30/150 -> 30/180 |
| BAR | 70 -> 100-85 | 2.5 -> 2.7 | 0.13 -> 0.16 | 2.8 -> 2.75 / 3.25 | 20/140 -> 20/140 |
| Trench Gun | 8x45 -> 8x160-15 | 1.5 -> 1 | 0.75 -> 1.083 | 3.2 -> 0.9 + 0.6/shell + 0.95 | 6/54 -> 6/60 |
| Double-Barrel | 10x50 -> 8x200-15 | 1.5 -> 1 | 0.3 -> 0.283 | 2.6 -> 3.0 / 4.0 | 2/58 -> 2/60 |
| Sawed-Off | 10x55 -> 8x200-15 | 1.5 -> 1 | 0.3 -> 0.283 | 2.6 -> 3.0 / 4.0 | 2/58 -> 2/60 |
| Scoped Kar98k | 300 -> 300 | 4 -> 10 (neck 5) | 0.33 -> 1.53 | 0.6 -> 1.8 + 0.6/round + 0.77 | 5/45 -> 5/50 |
| Ray Gun | 1000 + 300 splash (1 m) -> 1000 + 1500-300 (1.63 m) + script | - | 0.33 semi -> 0.33 auto | 3.0 | 20/140 -> 20/160 |
| PPSh (not on Nacht) | 30 -> 100-60 | 2.5 -> 4 | 0.055 -> 0.048 | 3.0 -> 2.1 / 2.8 | 71/213 -> 71/355 |
| New: .357, Gewehr 43, Springfield, PTRS-41, Garand w/ Launcher, FG42, MG42, M1919 | see tables | | | | |
