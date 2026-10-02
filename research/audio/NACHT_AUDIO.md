# Nacht der Untoten (nazi_zombie_prototype): audio reference

This is a clean-room research note for the Undead Rounds Bevy remake. Every alias name below was checked against the
user's install: the Nacht zone (`research/t4/sounds.json`, i.e. `zone/english/nazi_zombie_prototype.ff`), `common.ff` and the IWDs in
`main/`. **VERIFIED** means checked against data. **ASSUMED** means engine behaviour inferred from the data or from
general IW-engine knowledge, without a disassembly to confirm it.

Storage legend: **inline** = LoadedSound RIFF inside the zone (WAVE MS-ADPCM/PCM or XWMA). **stream** = streamed file
`sound/<dir>/<name>` in an IWD. **xref** = the Nacht alias exists, but its LoadedSound is a `,name` cross-zone reference with no data;
the bytes live in `common.ff` (see §7.3).

---------------------------------------------------------------------------------------------------------------------

## 0. TL;DR

* **Weapon foley comes from viewmodel xanim notetracks.** A viewmodel notetrack name is a **bare alias name**
  (`gr_kar98_clip_in_plr`, no prefix), and the engine resolves it through `WeaponDef.notetrackSoundMapKeys/Values[20]`
  (§2.1). Every map in Nacht is an identity map (key == value), so in practice "notetrack name == alias name, if the
  weapon lists it". AI/zombie anims use a different convention: `sndnt#<alias>`.
* **Notetrack time** is normalized (0..1) over the anim. The engine scales the anim to the WeaponDef state time
  (`iReloadTime`, `iReloadEmptyTime`, `iRechamberTime`, …), so the real time is **t × stateTime** (ASSUMED, §2.3).
* **Third-person fields** (`reloadSound` = `gr_*_3p`, `rechamberSound`, …) are for other players. The local player hears the
  `*SoundPlayer` fields plus the notetracks.
* **Nacht music**: music state `SPLASH_SCREEN` (one-shot `mx_splash_screen`, 11.1 s), then `WAVE_1` (looping
  `mx_zombie_wave_1`, 44.1 s, volume 0.06). The loop is never restarted or changed per round. There is **no round-start
  music**. Round start plays the SFX `chalk`, and round end plays the SFX `round_over`. Game over plays `mx_game_over` (25.8 s), then the heartbeat starts.
* **No announcer in Nacht.** Power-ups play `full_ammo`, `insta_kill_loop`→`insta_kill`, `double_point_loop`→`points_loop_off`,
  and `nuke_flash` + `nuked`. Announcer VO (`*_vox`) only arrives with Verrückt. Nacht has no carpenter, no teddy bear, and no box "ready" sound.
* **Zombie vocals are all notetrack-driven**: `amb_vocals` (walk/idle cycle start), `sprint_vocals`, `crawl_vocals(_slow)`,
  `attack_vocals` + `attack_whoosh` (attack), and `remove_boards` (tearing). Zombie deaths are **silent** (no `generic_death_*` alias
  exists, and `death_gurgle` is `sfx/null.wav`), except for gibs and the head pop.
* The runtime scripts for Nacht are the **patch.ff** versions (`maps/nazi_zombie_prototype.gsc`, `_zombiemode_prototype.gsc`,
  `_zombiemode_spawner_prototype.gsc`, `_zombiemode_blockers.gsc`). They override the copies inside the map fastfile (ASSUMED
  precedence, but the patched level script calls `_zombiemode_prototype::main` and listens for the patched blockers'
  `"junk purchased"` notify, so they are clearly the intended set).

---------------------------------------------------------------------------------------------------------------------

## 1. Sources and helper scripts (this folder)

| file | what |
|---|---|
| `dump_weapon_sounds.py` | Walks a zone with `research/t4` and dumps, for every WeaponDef: all 59 `SndAliasCustom` sound fields, `notetrackSoundMapKeys/Values` (script strings), state times (`iReloadTime`…) and every anim slot's notetracks (name, normalized time, frame, seconds). `--anims-from common.ff` resolves anims that live in common (colt45, knife). → `weapon_sounds_nacht.json/.txt` |
| `alias_index.py` | Alias lookup over Nacht + common.ff (`sounds_common.json` is generated on first run) + IWD check. `py alias_index.py name…` / `--grep substr` |
| `analyze_notetracks.py` | Cross-checks notetrack vs notetrackSoundMap vs alias existence (evidence for §2.1) |
| `check_weapon_aliases.py` → `weapon_alias_check.txt` | Existence/storage of every alias referenced by any Nacht weapon |
| `gen_weapon_tables.py` → `weapon_tables.md` | The per-weapon tables in §2.5 |
| `extract_rawfiles.py` | Dumps rawfiles (GSC/CSC) from a fastfile. It was used for `common.ff` and `patch.ff` (output in a temp dir, not kept; rerun it to get file:line context) |
| `dump_emitters.py` → `nacht_emitters.json` | Ambient emitters from the map entities (§5) |

Script locations below are written `zone:path:line`. **P** = `patch.ff` (runtime Nacht scripts), **N** = `nazi_zombie_prototype.ff`
(= `D:\Decompile Test\extracted\fastfiles\nazi_zombie_prototype\raw\…`), **C** = `common.ff`.

How scripts play sounds (N `maps/_zombiemode_utility.gsc:595-681`):
* `add_sound(ref, alias)` fills `level.zombie_sounds[ref]`.
* `play_sound_at_pos(ref, pos)` → `PlaySoundAtPosition(alias, pos)` (one-shot at a world point).
* `ent play_sound_on_ent(ref)` → `ent PlaySound(alias)` (follows the entity). **An entity's own `script_soundalias`, or its
  `script_sound` key, overrides `ref`** (this matters for one window, see §4.3).
* `play_loopsound_on_ent` is buggy: it calls `PlaySound`, not `PlayLoopSound` (line 680), so it plays once.

The ref → alias table (P `_zombiemode_prototype.gsc:204-240`, plus P `nazi_zombie_prototype.gsc:241`):

| ref | alias | storage | file |
|---|---|---|---|
| end_of_round | `round_over` | inline XWMA 2.8 s | sfx/levels/zombie/chalk/**chalk_v2**.wav |
| end_of_game | `mx_game_over` | stream 25.8 s | stream/music/mission/zombie/mx_game_over.wav |
| chalk_one_up | `chalk` | inline XWMA 3.9 s | sfx/levels/zombie/chalk/**round_over**.wav (names are swapped vs the alias!) |
| purchase | `cha_ching` | inline 0.4 s | sfx/levels/zombie/buy_generic/buy_generic.wav |
| no_purchase | `no_cha_ching` | inline 0.1 s | …/buy_generic/no_cha_ching.wav |
| playerzombie_* (3) | `attack_vocals` | inline ×23 | zombified-player mode only |
| zombie_head_gib | `zombie_head_gib` | inline ×3 | sfx/levels/zombie/zombie_head/zombie_head_0[0-2].wav |
| rebuild_barrier_piece | `repair_boards` | inline | wood_repair_00.wav |
| rebuild_barrier_hover | `boards_float` | inline | boards_float.wav |
| debris_hover_loop | `couch_loop` | inline (looping flag) | couch_loop/whisper_00.wav |
| break_barrier_piece | `break_boards` | inline ×6 | wood_snap_0[0-5].wav |
| blocker_end_move, barrier_rebuild_slam | `board_slam` | inline ×6 | board_slam_0[0-5].wav |
| door_slide_open, door_rotate_open | `door_slide_open` | inline | doors/door_slide_open.wav (unused in Nacht: no `zombie_door` entities) |
| debris_move, weapon_show | `weap_wall` | inline | weap_wall/weap_wall.wav |
| open_chest / music_chest / close_chest | `lid_open` / `music_box` / `lid_close` | inline | music_box/*.wav |
| break_stone | `break_stone` | inline ×5 | stone/stone_break/stone_break_0[0-4].wav |

---------------------------------------------------------------------------------------------------------------------

## 2. Weapon sounds

### 2.1 Notetrack → alias rule

**Rule:** while a weapon's **viewmodel** anim plays (any `szXAnims` slot), each notetrack is crossed at
`time × duration`. Its name (a script string) is compared with `WeaponDef.notetrackSoundMapKeys[i]` for
i = 0..19 (stop at the first 0 key). On a match, `notetrackSoundMapValues[i]` is played as a local (2D, player) sound
alias. Notetracks that are not in the map play nothing. `end` is the synthetic end marker.

Evidence (VERIFIED data, `analyze_notetracks.py`, `weapon_sounds_nacht.json`):
1. The WeaponDef layout has `0xF0 notetrackSoundMapKeys u16[20]` and `0x118 notetrackSoundMapValues u16[20]` (`T4_LAYOUTS.txt`),
   and all 44 Nacht weapons fill them. **Every pair is identity** (key == value). For example kar98k:
   `gr_kar98_bolt_up_plr, _bolt_back_plr, _clip_in_plr, _bolt_front_plr, _clip_eject_plr, _clip_land_plr, _bolt_down_plr` +
   `knife_stab_plr, knife_pull_plr, knife_slash_plr` (the 3 knife pairs are appended to every weapon except the flamethrower).
2. Viewmodel notetracks carry **bare alias names**. AI anims use `sndnt#<alias>` instead, which is a different (engine-level, ASSUMED
   generic) path, see §6.
3. Notetrack occurrences over all viewmodel anims of all Nacht weapons: **297 are in the map and have an alias; 0 are in the map without an alias;
   6 have an alias but are not in the map; 4 have neither**. The 10 exceptions are listed below. Under a "direct alias" rule the 6 would play; under the
   map rule they are silent. The map rule is the only reason for the tables to exist, which is why the map rule is adopted (the exact engine code is ASSUMED, not disassembled).

| weapon | anim (slot) | notetrack | in map | alias exists | effect under the map rule |
|---|---|---|---|---|---|
| m1garand, m1garand_gl | `viewmodel_m1garand_partial_reload` (reload) f25 | `gr_m1gar_futz_plr` | no | **no** | silent either way |
| m1garand, m1garand_gl | `viewmodel_m1garand_reload` (reloadEmpty) f17 | `gr_m1garand_futz_plr` | no | **no** | silent either way |
| springfield | `viewmodel_springfield_reload` f85 | `gr_springfield_clip_land_plr` | no | yes | silent |
| doublebarrel_sawed_grip | `viewmodel_trenchgun_rechamber` (rechamber) | `gr_shotgun_pull_plr`, `gr_shotgun_push_plr` | no | yes | silent (its map only has dbshot keys) |
| m2_flamethrower_zombie | `viewmodel_knife_stick` (meleeCharge) | `knife_stab_plr`, `knife_pull_plr` | no (map empty) | yes | silent |

4. `knife_slash_plr` is in every map but **does not exist as an alias**, and no anim uses it (`viewmodel_knife_slash` has no sound notetracks).

Implementation: `HashMap<notetrack, alias>` per weapon, built from `notetrackSoundMapKeys/Values` (the Rust `WeaponInfo` does not read
these yet: offsets 0xF0/0x118, u16 script-string indices). As a fallback, treating `notetrack == alias` gives the same result except for the 6 rows above.

All notetrack aliases (`gr_*_plr`, `ray_reload_*`, `gr_357_*`, `gr_mg_*`, `gr_m1gren_*`, `knife_*_plr`) are **inline in the Nacht zone**.
The colt45 anims live in common.ff, but their aliases (`gr_1911_*`) are in Nacht.

### 2.2 WeaponDef sound fields (VERIFIED values, ASSUMED trigger semantics)

Each field exists as a world/3rd-person version (`xxxSound`) and a local-player version (`xxxSoundPlayer`). For a first-person
remake, use the **Player** field and fall back to the plain one.

| field | when (ASSUMED engine) | typical Nacht values |
|---|---|---|
| fireSoundPlayer / fireLastSoundPlayer | every shot / the last round in the clip | `weap_<gun>_fire_plr`; M1 Garand last shot = `weap_m1garand_lastshot_plr` (the "ping" alias, but its file is the normal fire wav) |
| emptyFireSoundPlayer | trigger pulled with an empty clip | `dryfire_pistol_plr` / `dryfire_rifle_plr` / `dryfire_smg_plr` (inline) |
| reloadSoundPlayer / reloadEmptySoundPlayer | at reload start (in addition to the notetracks) | `gear_player` (×5 gear rustle) on colt, 357, garand, BAR, ray gun; empty for most others |
| raiseSoundPlayer / putawaySoundPlayer | weapon switch in / out | `weap_raise_plr` / `weap_putaway_plr` (both gear_player wavs) |
| firstRaiseSoundPlayer | first raise after GiveWeapon (wall-buy/box) | `weap_raise_plr`; `stg44_first_raise`, `carbine_first_raise`, `gewehr_first_raise`, `p38_first_raise`, `weap_flamethrower_foley_F` |
| meleeSwipeSoundPlayer | knife swing | `melee_swing_plr` (rifles/SMGs) / `melee_swing_small_plr` (pistols, ray gun) |
| meleeHitSound | knife connects | `melee_hit` (×9) |
| pickupSoundPlayer / ammoPickupSoundPlayer | picking up a weapon/ammo from the ground (unused in zombies) | `weap_pickup_plr`, `ammo_pickup_plr` |
| pullbackSoundPlayer | grenade cook/pin | `grenade_pull_pin` (stielhandgranate); `weap_molotov_light` **missing** |
| overheatSoundPlayer | MG overheat | `hmg_overheat_plr` (stream, iw_15.iwd) |
| projectileSound | rocket/raygun bolt loop | `weap_pnzr_fire_rocket`, `weap_rgun_loop` |
| rechamberSound, reloadSound, reloadStartSound, reloadEndSound (no Player variant set) | **3rd person only** | `gr_rifle_rechamber_3p`, `gr_smg_reload_*_3p`, `gr_pistol_reload_*_3p`, `gr_shotgun_shell`… |

Weapon aliases that are referenced but **missing** from the Nacht zone and common.ff: `player_out_of_ammo` (zombie_melee/grenades
emptyFireSound), `weap_ammo_pickup`, `weap_fraggrenade_fire/_pin/_reload` (fraggrenade is not used in Nacht), `weap_molotov_light/_throw/_impact`.
Only `hmg_overheat(_plr)` is streamed. Every other weapon alias is inline.

`m2_flamethrower_zombie` has **no fire sound field at all**. Its sounds come from the flame table (`flameIgniteSound`, `flameOnLoopSound`,
`flameOffLoopSound`, `flameCooldownSound` in `T4_LAYOUTS.txt` around line 469), which was **not** dumped here (open item). `weapon_fire_sounds` in
`nacht/build.rs` therefore finds nothing for it.

### 2.3 Timing rule

Notify `time` is normalized over `numframes` (VERIFIED in `T4_XANIM_NOTES.md`). The WeaponDef state times differ from the
anim lengths. For example, m1garand reload anim 4.38 s vs `iReloadTime` 3400 ms; thompson 1.93 s vs 2100 ms. The engine plays the viewmodel anim at
`rate = animLength / stateTime` (ASSUMED, consistent with how Sleight of Hand speeds up reload anims), so a notetrack fires at
**`t × stateTime`**. Slot → time field: reload→`iReloadTime`, reloadEmpty→`iReloadEmptyTime`, rechamber/adsRechamber→`iRechamberTime`,
raise→`iRaiseTime`, firstRaise→`iFirstRaiseTime`, drop→`iDropTime`, reloadStart→`iReloadStartTime`, reloadEnd→`iReloadEndTime`,
deploy→`deployTime`, breakdown→`breakdownTime`, altRaise/altDrop→`iAltRaiseTime/iAltDropTime`, meleeCharge→`meleeChargeTime`.
Ammo is added to the clip at `iReloadAddTime` (e.g. kar98k 1650 ms, thompson 1600 ms). Shotgun/scoped-kar98 reload one round per loop
(`reloadStart` → n × `reload` loop → `reloadEnd`).

### 2.4 Knife

* Normal knife (`melee` slot = `viewmodel_knife_slash`, common.ff, 19 f): **no sound notetracks**. Play the current weapon's
  `meleeSwipeSoundPlayer` at swing start, and `meleeHitSound` = `melee_hit` on impact (ASSUMED engine).
* Lunge (`meleeCharge` = `viewmodel_knife_stick`, 29 f @30, `meleeChargeTime` 1000 ms): `knife_stab_plr` at t 0.069 (≈0.07 s),
  `knife_pull_plr` at t 0.552 (≈0.55 s). These are identical for all weapons except the flamethrower (silent, empty map).

### 2.5 Per-weapon notetrack tables (generated by `gen_weapon_tables.py`, VERIFIED data)

Columns: frame, normalized t, **t × WeaponDef state time** (the time to fire the sound), alias. ADS up/down, knife slash and the
shared knife lunge rows are omitted. Wall-buys in Nacht: kar98k, m1carbine, thompson, doublebarrel, doublebarrel_sawed_grip, shotgun,
kar98k_scoped_zombie, bar, stielhandgranate. The starting pistol is zombie_colt. Everything else comes from the box.

#### `zombie_colt`

Player sound fields: firePlr=`weap_colt_fire_plr`, fireLastPlr=`weap_colt_fire_plr`, emptyFirePlr=`dryfire_pistol_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_small_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_colt45_reload_notempty` | 62 f @ 24 fps = 2.58s | 1900 ms | f9 / t0.145 / **0.28s** `gr_1911_mag_out_plr`<br>f39 / t0.629 / **1.20s** `gr_1911_futz_plr`<br>f43 / t0.694 / **1.32s** `gr_1911_mag_in_plr` |
| reloadEmpty | `viewmodel_colt45_reload` | 70 f @ 24 fps = 2.92s | 2650 ms | f9 / t0.129 / **0.34s** `gr_1911_mag_out_plr`<br>f35 / t0.500 / **1.32s** `gr_1911_futz_plr`<br>f42 / t0.600 / **1.59s** `gr_1911_mag_in_plr`<br>f56 / t0.800 / **2.12s** `gr_1911_slide_forward_plr` |

#### `sw_357`

Player sound fields: firePlr=`weap_357_fire_plr`, fireLastPlr=`weap_357_fire_plr`, emptyFirePlr=`dryfire_pistol_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_small_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_sw357_reload` | 89 f @ 30 fps = 2.97s | 3000 ms | f17 / t0.191 / **0.57s** `gr_357_open`<br>f40 / t0.449 / **1.35s** `gr_357_empty`<br>f62 / t0.697 / **2.09s** `gr_357_load`<br>f79 / t0.888 / **2.66s** `gr_357_close` |
| reloadEmpty | `viewmodel_sw357_reload` | 89 f @ 30 fps = 2.97s | 3000 ms | f17 / t0.191 / **0.57s** `gr_357_open`<br>f40 / t0.449 / **1.35s** `gr_357_empty`<br>f62 / t0.697 / **2.09s** `gr_357_load`<br>f79 / t0.888 / **2.66s** `gr_357_close` |

#### `kar98k`

Player sound fields: firePlr=`weap_kar98k_fire_plr`, fireLastPlr=`weap_kar98k_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_kar98_rechamber` | 31 f @ 30 fps = 1.03s | 1000 ms | f7 / t0.226 / **0.23s** `gr_kar98_bolt_up_plr`<br>f11 / t0.355 / **0.35s** `gr_kar98_bolt_back_plr`<br>f19 / t0.613 / **0.61s** `gr_kar98_bolt_front_plr` |
| reload | `viewmodel_kar98_reload` | 79 f @ 30 fps = 2.63s | 2500 ms | f5 / t0.063 / **0.16s** `gr_kar98_bolt_up_plr`<br>f9 / t0.114 / **0.28s** `gr_kar98_bolt_back_plr`<br>f44 / t0.557 / **1.39s** `gr_kar98_clip_in_plr`<br>f64 / t0.810 / **2.03s** `gr_kar98_bolt_front_plr`<br>f65 / t0.823 / **2.06s** `gr_kar98_clip_eject_plr`<br>f71 / t0.899 / **2.25s** `gr_kar98_bolt_down_plr` |
| reloadEmpty | `viewmodel_kar98_reload` | 79 f @ 30 fps = 2.63s | 2500 ms | f5 / t0.063 / **0.16s** `gr_kar98_bolt_up_plr`<br>f9 / t0.114 / **0.28s** `gr_kar98_bolt_back_plr`<br>f44 / t0.557 / **1.39s** `gr_kar98_clip_in_plr`<br>f64 / t0.810 / **2.03s** `gr_kar98_bolt_front_plr`<br>f65 / t0.823 / **2.06s** `gr_kar98_clip_eject_plr`<br>f71 / t0.899 / **2.25s** `gr_kar98_bolt_down_plr` |
| adsRechamber | `viewmodel_kar98_rechamber` | 31 f @ 30 fps = 1.03s | 1000 ms | f7 / t0.226 / **0.23s** `gr_kar98_bolt_up_plr`<br>f11 / t0.355 / **0.35s** `gr_kar98_bolt_back_plr`<br>f19 / t0.613 / **0.61s** `gr_kar98_bolt_front_plr` |

#### `springfield`

Player sound fields: firePlr=`weap_springfield_fire_plr`, fireLastPlr=`weap_springfield_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_springfield_rechamber` | 36 f @ 30 fps = 1.20s | 1000 ms | f5 / t0.139 / **0.14s** `gr_springfield_bolt_up_plr`<br>f11 / t0.306 / **0.31s** `gr_springfield_bolt_back_plr`<br>f19 / t0.528 / **0.53s** `gr_springfield_bolt_front_plr`<br>f23 / t0.639 / **0.64s** `gr_springfield_bolt_down_plr` |
| reload | `viewmodel_springfield_reload` | 92 f @ 30 fps = 3.07s | 3200 ms | f12 / t0.130 / **0.42s** `gr_springfield_bolt_up_plr`<br>f18 / t0.196 / **0.63s** `gr_springfield_bolt_back_plr`<br>f55 / t0.598 / **1.91s** `gr_springfield_clip_in_plr`<br>f78 / t0.848 / **2.71s** `gr_springfield_clip_eject_plr`<br>f78 / t0.848 / **2.71s** `gr_springfield_bolt_front_plr`<br>f83 / t0.902 / **2.89s** `gr_springfield_bolt_down_plr`<br>f85 / t0.924 / **2.96s** `gr_springfield_clip_land_plr` **(not in map)** |
| reloadEmpty | `viewmodel_springfield_reload` | 92 f @ 30 fps = 3.07s | 3200 ms | f12 / t0.130 / **0.42s** `gr_springfield_bolt_up_plr`<br>f18 / t0.196 / **0.63s** `gr_springfield_bolt_back_plr`<br>f55 / t0.598 / **1.91s** `gr_springfield_clip_in_plr`<br>f78 / t0.848 / **2.71s** `gr_springfield_clip_eject_plr`<br>f78 / t0.848 / **2.71s** `gr_springfield_bolt_front_plr`<br>f83 / t0.902 / **2.89s** `gr_springfield_bolt_down_plr`<br>f85 / t0.924 / **2.96s** `gr_springfield_clip_land_plr` **(not in map)** |
| adsRechamber | `viewmodel_springfield_rechamber` | 36 f @ 30 fps = 1.20s | 1000 ms | f5 / t0.139 / **0.14s** `gr_springfield_bolt_up_plr`<br>f11 / t0.306 / **0.31s** `gr_springfield_bolt_back_plr`<br>f19 / t0.528 / **0.53s** `gr_springfield_bolt_front_plr`<br>f23 / t0.639 / **0.64s** `gr_springfield_bolt_down_plr` |

#### `m1carbine`

Player sound fields: firePlr=`weap_carbine_fire_plr`, fireLastPlr=`weap_carbine_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`carbine_first_raise`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`carbine_first_raise`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_carbine_reload` | 95 f @ 30 fps = 3.17s | 2900 ms | f23 / t0.242 / **0.70s** `gr_m1carbine_mag_out_plr`<br>f50 / t0.526 / **1.53s** `gr_m1carbine_futz_plr`<br>f55 / t0.579 / **1.68s** `gr_m1carbine_mag_in_plr`<br>f71 / t0.747 / **2.17s** `gr_m1carbine_tap_plr` |
| reloadEmpty | `viewmodel_carbine_reload_empty` | 111 f @ 30 fps = 3.70s | 3700 ms | f20 / t0.180 / **0.67s** `gr_m1carbine_mag_out_plr`<br>f50 / t0.451 / **1.67s** `gr_m1carbine_futz_plr`<br>f54 / t0.486 / **1.80s** `gr_m1carbine_mag_in_plr`<br>f71 / t0.640 / **2.37s** `gr_m1carbine_tap_plr`<br>f94 / t0.847 / **3.13s** `gr_m1carbine_charge_plr` |

#### `gewehr43`

Player sound fields: firePlr=`weap_g43_fire_plr`, fireLastPlr=`weap_g43_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`gewehr_first_raise`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`gewehr_first_raise`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_g43_noscope_partial_reload` | 99 f @ 30 fps = 3.30s | 3000 ms | f18 / t0.182 / **0.55s** `gr_g43_mag_out_plr`<br>f51 / t0.515 / **1.55s** `gr_g43_mag_in_plr`<br>f66 / t0.667 / **2.00s** `gr_g43_tap_plr` |
| reloadEmpty | `viewmodel_g43_noscope_reload` | 142 f @ 30 fps = 4.73s | 4000 ms | f26 / t0.183 / **0.73s** `gr_g43_tap_plr`<br>f33 / t0.232 / **0.93s** `gr_g43_mag_out_plr`<br>f63 / t0.444 / **1.77s** `gr_g43_mag_in_plr`<br>f82 / t0.578 / **2.31s** `gr_g43_tap_plr`<br>f118 / t0.831 / **3.32s** `gr_kar98_bolt_front_plr` |

#### `m1garand`

Player sound fields: firePlr=`weap_m1garand_fire_plr`, fireLastPlr=`weap_m1garand_lastshot_plr`, emptyFirePlr=`dryfire_rifle_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_m1garand_partial_reload` | 105 f @ 24 fps = 4.38s | 3400 ms | f12 / t0.114 / **0.39s** `gr_m1gar_pull_slide_plr`<br>f22 / t0.209 / **0.71s** `gr_m1gar_clip_out_plr`<br>f25 / t0.238 / **0.81s** `gr_m1gar_futz_plr` **(no alias)**<br>f29 / t0.276 / **0.94s** `gr_m1gar_breech_close_plr`<br>f65 / t0.619 / **2.10s** `gr_m1gar_pull_slide_plr`<br>f75 / t0.714 / **2.43s** `gr_m1gar_clip_in_plr`<br>f82 / t0.781 / **2.66s** `gr_m1gar_breech_close_plr` |
| reloadEmpty | `viewmodel_m1garand_reload` | 46 f @ 24 fps = 1.92s | 1600 ms | f17 / t0.370 / **0.59s** `gr_m1garand_futz_plr` **(no alias)**<br>f24 / t0.522 / **0.83s** `gr_m1gar_clip_in_plr`<br>f33 / t0.717 / **1.15s** `gr_m1gar_breech_close_plr` |
| firstRaise | `viewmodel_m1garand_first_pullout` | 64 f @ 24 fps = 2.67s | 1000 ms | f16 / t0.250 / **0.25s** `gr_m1gar_pull_slide_plr`<br>f32 / t0.500 / **0.50s** `gr_m1gar_breech_close_plr` |

#### `m1garand_gl`

Player sound fields: firePlr=`weap_m1garand_fire_plr`, fireLastPlr=`weap_m1garand_lastshot_plr`, emptyFirePlr=`dryfire_rifle_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_m1garand_partial_reload` | 105 f @ 24 fps = 4.38s | 3400 ms | f12 / t0.114 / **0.39s** `gr_m1gar_pull_slide_plr`<br>f22 / t0.209 / **0.71s** `gr_m1gar_clip_out_plr`<br>f25 / t0.238 / **0.81s** `gr_m1gar_futz_plr` **(no alias)**<br>f29 / t0.276 / **0.94s** `gr_m1gar_breech_close_plr`<br>f65 / t0.619 / **2.10s** `gr_m1gar_pull_slide_plr`<br>f75 / t0.714 / **2.43s** `gr_m1gar_clip_in_plr`<br>f82 / t0.781 / **2.66s** `gr_m1gar_breech_close_plr` |
| reloadEmpty | `viewmodel_m1garand_reload` | 46 f @ 24 fps = 1.92s | 1600 ms | f17 / t0.370 / **0.59s** `gr_m1garand_futz_plr` **(no alias)**<br>f24 / t0.522 / **0.83s** `gr_m1gar_clip_in_plr`<br>f33 / t0.717 / **1.15s** `gr_m1gar_breech_close_plr` |
| firstRaise | `viewmodel_m1garand_first_pullout` | 64 f @ 24 fps = 2.67s | 1000 ms | f16 / t0.250 / **0.25s** `gr_m1gar_pull_slide_plr`<br>f32 / t0.500 / **0.50s** `gr_m1gar_breech_close_plr` |

#### `thompson`

Player sound fields: firePlr=`weap_thompson_fire_plr`, emptyFirePlr=`dryfire_smg_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_thompson_reload` | 58 f @ 30 fps = 1.93s | 2100 ms | f13 / t0.224 / **0.47s** `gr_thompson_mag_out_plr`<br>f41 / t0.707 / **1.48s** `gr_thompson_mag_in_plr` |
| reloadEmpty | `viewmodel_thompson_reload_empty` | 77 f @ 30 fps = 2.57s | 2450 ms | f13 / t0.169 / **0.41s** `gr_thompson_mag_out_plr`<br>f30 / t0.390 / **0.95s** `gr_thompson_mag_in_plr`<br>f52 / t0.675 / **1.65s** `gr_thompson_charge_plr` |

#### `mp40`

Player sound fields: firePlr=`weap_mp40_fire_plr`, fireLastPlr=`weap_mp40_fire_plr`, emptyFirePlr=`dryfire_smg_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_mp40_reload` | 78 f @ 30 fps = 2.60s | 2300 ms | f18 / t0.231 / **0.53s** `gr_mp40_mag_out_plr`<br>f49 / t0.628 / **1.44s** `gr_mp40_futz_plr`<br>f61 / t0.782 / **1.80s** `gr_mp40_mag_in_plr` |
| reloadEmpty | `viewmodel_mp40_reload_empty` | 100 f @ 30 fps = 3.33s | 2900 ms | f18 / t0.180 / **0.52s** `gr_mp40_mag_out_plr`<br>f51 / t0.510 / **1.48s** `gr_mp40_futz_plr`<br>f61 / t0.610 / **1.77s** `gr_mp40_mag_in_plr`<br>f81 / t0.810 / **2.35s** `gr_mp40_charge_plr` |
| firstRaise | `viewmodel_mp40_first_raise` | 50 f @ 30 fps = 1.67s | 1330 ms | f19 / t0.380 / **0.51s** `gr_mp40_charge_plr` |

#### `stg44`

Player sound fields: firePlr=`weap_MP44_fire_plr`, fireLastPlr=`weap_MP44_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`stg44_first_raise`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`stg44_first_raise`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_mp44_reload` | 60 f @ 30 fps = 2.00s | 2150 ms | f11 / t0.183 / **0.39s** `gr_mp44_mag_out_plr`<br>f34 / t0.567 / **1.22s** `gr_mp44_futz_plr`<br>f41 / t0.683 / **1.47s** `gr_mp44_mag_in_plr` |
| reloadEmpty | `viewmodel_mp44_reload_empty` | 85 f @ 30 fps = 2.83s | 2800 ms | f11 / t0.129 / **0.36s** `gr_mp44_mag_out_plr`<br>f34 / t0.400 / **1.12s** `gr_mp44_futz_plr`<br>f37 / t0.435 / **1.22s** `gr_mp44_mag_in_plr`<br>f63 / t0.741 / **2.08s** `gr_mp44_charge_plr` |

#### `bar`

Player sound fields: firePlr=`weap_bar_fire_plr`, fireLastPlr=`weap_bar_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_bar_reload` | 106 f @ 30 fps = 3.53s | 2750 ms | f27 / t0.255 / **0.70s** `gr_bar_mag_out_plr`<br>f59 / t0.557 / **1.53s** `gr_bar_mag_in_plr`<br>f73 / t0.689 / **1.89s** `gr_bar_tap_plr` |
| reloadEmpty | `viewmodel_bar_reload_empty` | 137 f @ 30 fps = 4.57s | 3250 ms | f27 / t0.197 / **0.64s** `gr_bar_mag_out_plr`<br>f58 / t0.423 / **1.38s** `gr_bar_mag_in_plr`<br>f73 / t0.533 / **1.73s** `gr_bar_tap_plr`<br>f102 / t0.745 / **2.42s** `gr_bar_charge_plr` |

#### `fg42_bipod`

Player sound fields: firePlr=`weap_fg42_fire_plr`, fireLastPlr=`weap_fg42_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_fg42_partial_reload` | 89 f @ 30 fps = 2.97s | 2400 ms | f15 / t0.169 / **0.40s** `gr_fg42_mag_out_plr`<br>f61 / t0.685 / **1.64s** `gr_fg42_futz_plr`<br>f67 / t0.753 / **1.81s** `gr_fg42_mag_in_plr` |
| reloadEmpty | `viewmodel_fg42_reload` | 148 f @ 30 fps = 4.93s | 3500 ms | f15 / t0.101 / **0.35s** `gr_fg42_mag_out_plr`<br>f61 / t0.412 / **1.44s** `gr_fg42_futz_plr`<br>f67 / t0.453 / **1.58s** `gr_fg42_mag_in_plr`<br>f110 / t0.743 / **2.60s** `gr_fg42_pull_plr`<br>f116 / t0.784 / **2.74s** `gr_fg42_release_plr` |
| deploy | `viewmodel_fg42_deploy` | 40 f @ 30 fps = 1.33s | 1400 ms | f11 / t0.275 / **0.39s** `gr_mg_deploy_start`<br>f17 / t0.425 / **0.59s** `gr_mg_deploy_end` |
| breakdown | `viewmodel_fg42_breakdown` | 40 f @ 30 fps = 1.33s | 1300 ms | f10 / t0.250 / **0.33s** `gr_mg_break_down` |

#### `mg42_bipod`

Player sound fields: firePlr=`weap_mg42_fire_plr`, fireLastPlr=`weap_mg42_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_mg42_reload` | 150 f @ 30 fps = 5.00s | 4500 ms | f18 / t0.120 / **0.54s** `gr_mg42_jimmy_plr`<br>f25 / t0.167 / **0.75s** `gr_mg42_mag_out_plr`<br>f55 / t0.367 / **1.65s** `gr_mg42_futz_plr`<br>f64 / t0.427 / **1.92s** `gr_mg42_mag_in_plr`<br>f101 / t0.673 / **3.03s** `gr_mg42_pull_plr`<br>f111 / t0.740 / **3.33s** `gr_mg42_charge_plr` |
| reloadEmpty | `viewmodel_mg42_reload_empty` | 150 f @ 30 fps = 5.00s | 4500 ms | f18 / t0.120 / **0.54s** `gr_mg42_jimmy_plr`<br>f25 / t0.167 / **0.75s** `gr_mg42_mag_out_plr`<br>f55 / t0.367 / **1.65s** `gr_mg42_futz_plr`<br>f64 / t0.427 / **1.92s** `gr_mg42_mag_in_plr`<br>f101 / t0.673 / **3.03s** `gr_mg42_pull_plr`<br>f111 / t0.740 / **3.33s** `gr_mg42_charge_plr` |
| deploy | `viewmodel_mg42_deploy` | 52 f @ 30 fps = 1.73s | 1600 ms | f23 / t0.442 / **0.71s** `gr_mg_deploy_start`<br>f32 / t0.615 / **0.98s** `gr_mg_deploy_end` |
| breakdown | `viewmodel_mg42_breakdown` | 48 f @ 30 fps = 1.60s | 1200 ms | f19 / t0.396 / **0.47s** `gr_mg_break_down` |

#### `30cal_bipod`

Player sound fields: firePlr=`weap_30cal_fire_plr`, fireLastPlr=`weap_30cal_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_30cal_partial_reload` | 267 f @ 30 fps = 8.90s | 7000 ms | f13 / t0.049 / **0.34s** `gr_30cal_start_plr`<br>f15 / t0.056 / **0.39s** `gr_30cal_charge_plr`<br>f61 / t0.229 / **1.60s** `gr_30cal_open_plr`<br>f80 / t0.300 / **2.10s** `gr_30cal_grab_belt_plr`<br>f87 / t0.326 / **2.28s** `gr_30cal_belt_remove_plr`<br>f117 / t0.438 / **3.07s** `gr_30cal_belt_raise_plr`<br>f126 / t0.472 / **3.30s** `gr_30cal_belt_contact_plr`<br>f136 / t0.509 / **3.57s** `gr_30cal_belt_press_plr`<br>f152 / t0.569 / **3.99s** `gr_30cal_lid_bonk_plr`<br>f156 / t0.584 / **4.09s** `gr_30cal_close_plr`<br>f178 / t0.667 / **4.67s** `gr_30cal_tap_plr`<br>f196 / t0.734 / **5.14s** `gr_30cal_ammo_toss_plr`<br>f236 / t0.884 / **6.19s** `gr_30cal_charge_release_plr` |
| reloadEmpty | `viewmodel_30cal_reload` | 262 f @ 30 fps = 8.73s | 6000 ms | f24 / t0.092 / **0.55s** `gr_30cal_start_plr`<br>f27 / t0.103 / **0.62s** `gr_30cal_charge_plr`<br>f85 / t0.324 / **1.95s** `gr_30cal_open_plr`<br>f116 / t0.443 / **2.66s** `gr_30cal_belt_raise_plr`<br>f125 / t0.477 / **2.86s** `gr_30cal_belt_contact_plr`<br>f135 / t0.515 / **3.09s** `gr_30cal_belt_press_plr`<br>f152 / t0.580 / **3.48s** `gr_30cal_lid_bonk_plr`<br>f154 / t0.588 / **3.53s** `gr_30cal_close_plr`<br>f173 / t0.660 / **3.96s** `gr_30cal_tap_plr`<br>f193 / t0.737 / **4.42s** `gr_30cal_ammo_toss_plr`<br>f226 / t0.863 / **5.18s** `gr_30cal_charge_release_plr` |
| deploy | `viewmodel_30cal_deploy` | 48 f @ 30 fps = 1.60s | 1400 ms | f0 / t0.000 / **0.00s** `gr_30cal_start_plr`<br>f17 / t0.354 / **0.50s** `gr_mg_deploy_start`<br>f24 / t0.500 / **0.70s** `gr_mg_deploy_end` |
| breakdown | `viewmodel_30cal_breakdown` | 71 f @ 30 fps = 2.37s | 2300 ms | f0 / t0.000 / **0.00s** `gr_30cal_start_plr`<br>f18 / t0.254 / **0.58s** `gr_mg_break_down` |

#### `doublebarrel`

Player sound fields: firePlr=`weap_dbshot_fire_plr`, fireLastPlr=`weap_dbshot_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_double_barrel_shotgun_partial_reload` | 94 f @ 30 fps = 3.13s | 3000 ms | f8 / t0.085 / **0.26s** `gr_dbshot_click_plr`<br>f18 / t0.192 / **0.57s** `gr_dbshot_break_plr`<br>f40 / t0.425 / **1.28s** `gr_dbshot_shake_1brl_plr`<br>f60 / t0.638 / **1.91s** `gr_dbshot_shell_in_1_plr`<br>f80 / t0.851 / **2.55s** `gr_dbshot_close_plr` |
| reloadEmpty | `viewmodel_double_barrel_shotgun_reload` | 110 f @ 30 fps = 3.67s | 4000 ms | f5 / t0.045 / **0.18s** `gr_dbshot_click_plr`<br>f19 / t0.173 / **0.69s** `gr_dbshot_break_plr`<br>f31 / t0.282 / **1.13s** `gr_dbshot_shake_plr`<br>f66 / t0.600 / **2.40s** `gr_dbshot_shell_in_1_plr`<br>f75 / t0.682 / **2.73s** `gr_dbshot_shell_in_2_plr`<br>f95 / t0.864 / **3.45s** `gr_dbshot_close_plr` |

#### `doublebarrel_sawed_grip`

Player sound fields: firePlr=`weap_dbsawshot_fire_plr`, fireLastPlr=`weap_dbsawshot_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_trenchgun_rechamber` | 27 f @ 30 fps = 0.90s | 800 ms | f7 / t0.259 / **0.21s** `gr_shotgun_pull_plr` **(not in map)**<br>f14 / t0.518 / **0.41s** `gr_shotgun_push_plr` **(not in map)** |
| reload | `viewmodel_double_barrel_shotgun_partial_reload` | 94 f @ 30 fps = 3.13s | 3000 ms | f8 / t0.085 / **0.26s** `gr_dbshot_click_plr`<br>f18 / t0.192 / **0.57s** `gr_dbshot_break_plr`<br>f40 / t0.425 / **1.28s** `gr_dbshot_shake_1brl_plr`<br>f60 / t0.638 / **1.91s** `gr_dbshot_shell_in_1_plr`<br>f80 / t0.851 / **2.55s** `gr_dbshot_close_plr` |
| reloadEmpty | `viewmodel_double_barrel_shotgun_reload` | 110 f @ 30 fps = 3.67s | 4000 ms | f5 / t0.045 / **0.18s** `gr_dbshot_click_plr`<br>f19 / t0.173 / **0.69s** `gr_dbshot_break_plr`<br>f31 / t0.282 / **1.13s** `gr_dbshot_shake_plr`<br>f66 / t0.600 / **2.40s** `gr_dbshot_shell_in_1_plr`<br>f75 / t0.682 / **2.73s** `gr_dbshot_shell_in_2_plr`<br>f95 / t0.864 / **3.45s** `gr_dbshot_close_plr` |

#### `shotgun`

Player sound fields: firePlr=`weap_shotgun_fire_pump_plr`, fireLastPlr=`weap_shotgun_fire_pump_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_trenchgun_rechamber` | 27 f @ 30 fps = 0.90s | 800 ms | f7 / t0.259 / **0.21s** `gr_shotgun_pull_plr`<br>f14 / t0.518 / **0.41s** `gr_shotgun_push_plr` |
| reload | `viewmodel_trenchgun_reload_loop` | 39 f @ 30 fps = 1.30s | 600 ms | f17 / t0.436 / **0.26s** `gr_shotgun_shell_plr` |
| reloadEmpty | `viewmodel_trenchgun_reload_loop` | 39 f @ 30 fps = 1.30s | 600 ms | f17 / t0.436 / **0.26s** `gr_shotgun_shell_plr` |
| reloadStart | `viewmodel_trenchgun_reload_start` | 55 f @ 30 fps = 1.83s | 900 ms | f34 / t0.618 / **0.56s** `gr_shotgun_shell_plr` |
| reloadEnd | `viewmodel_trenchgun_reload_end` | 35 f @ 30 fps = 1.17s | 950 ms | f12 / t0.343 / **0.33s** `gr_shotgun_pull_plr`<br>f15 / t0.429 / **0.41s** `gr_shotgun_push_plr` |
| adsRechamber | `viewmodel_trenchgun_rechamber` | 27 f @ 30 fps = 0.90s | 800 ms | f7 / t0.259 / **0.21s** `gr_shotgun_pull_plr`<br>f14 / t0.518 / **0.41s** `gr_shotgun_push_plr` |

#### `kar98k_scoped_zombie`

Player sound fields: firePlr=`weap_kar98k_fire_plr_snp`, fireLastPlr=`weap_kar98k_fire_plr_snp`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_kar98scoped_rechamber` | 31 f @ 30 fps = 1.03s | 1200 ms | f7 / t0.226 / **0.27s** `gr_kar98_bolt_up_plr`<br>f11 / t0.355 / **0.43s** `gr_kar98_bolt_back_plr`<br>f19 / t0.613 / **0.74s** `gr_kar98_bolt_front_plr` |
| reload | `viewmodel_kar98scoped_reload_loop` | 20 f @ 30 fps = 0.67s | 600 ms | f6 / t0.300 / **0.18s** `gr_kar98_shell_in_plr` |
| reloadEmpty | `viewmodel_kar98scoped_reload_loop` | 20 f @ 30 fps = 0.67s | 600 ms | f6 / t0.300 / **0.18s** `gr_kar98_shell_in_plr` |
| reloadStart | `viewmodel_kar98scoped_reload_start` | 55 f @ 30 fps = 1.83s | 1800 ms | f4 / t0.073 / **0.13s** `gr_kar98_bolt_up_plr`<br>f7 / t0.127 / **0.23s** `gr_kar98_bolt_back_plr`<br>f41 / t0.746 / **1.34s** `gr_kar98_shell_in_plr` |
| reloadEnd | `viewmodel_kar98scoped_reload_end` | 28 f @ 30 fps = 0.93s | 770 ms | f10 / t0.357 / **0.27s** `gr_kar98_bolt_front_plr`<br>f15 / t0.536 / **0.41s** `gr_kar98_bolt_down_plr` |
| adsRechamber | `viewmodel_kar98scoped_rechamber` | 31 f @ 30 fps = 1.03s | 1200 ms | f7 / t0.226 / **0.27s** `gr_kar98_bolt_up_plr`<br>f11 / t0.355 / **0.43s** `gr_kar98_bolt_back_plr`<br>f19 / t0.613 / **0.74s** `gr_kar98_bolt_front_plr` |

#### `ptrs41_zombie`

Player sound fields: firePlr=`weap_ptrs_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_ptrs_reload` | 160 f @ 30 fps = 5.33s | 4000 ms | f45 / t0.281 / **1.12s** `gr_ptrs_open_plr`<br>f92 / t0.575 / **2.30s** `gr_ptrs_futz_plr`<br>f96 / t0.600 / **2.40s** `gr_ptrs_clip_in_plr`<br>f121 / t0.756 / **3.03s** `gr_ptrs_close_plr` |
| reloadEmpty | `viewmodel_ptrs_reload_empty` | 190 f @ 30 fps = 6.33s | 5300 ms | f37 / t0.195 / **1.03s** `gr_ptrs_open_plr`<br>f76 / t0.400 / **2.12s** `gr_ptrs_futz_plr`<br>f80 / t0.421 / **2.23s** `gr_ptrs_clip_in_plr`<br>f101 / t0.532 / **2.82s** `gr_ptrs_close_plr`<br>f147 / t0.774 / **4.10s** `gr_ptrs_pull_plr`<br>f155 / t0.816 / **4.32s** `gr_ptrs_release_plr` |

#### `panzerschrek`

Player sound fields: firePlr=`weap_pnzr_fire_plr_f`, fireLastPlr=`weap_pnzr_fire_plr_f`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`, projectile=`weap_pnzr_fire_rocket`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_panzerschreck_reload` | 88 f @ 25 fps = 3.52s | 2850 ms | f2 / t0.023 / **0.06s** `gr_panzerschreck_down_plr`<br>f23 / t0.261 / **0.74s** `gr_panzerschreck_ground_plr`<br>f33 / t0.375 / **1.07s** `gr_panzerschreck_start_plr`<br>f44 / t0.500 / **1.43s** `gr_panzerschreck_tap_plr`<br>f60 / t0.682 / **1.94s** `gr_panzerschreck_up_plr` |

#### `m2_flamethrower_zombie`

Player sound fields: raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_flamethrower_foley_F`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, pickupPlr=`weap_flamethrower_foley_F`

_No sound notetracks._

#### `ray_gun`

Player sound fields: firePlr=`weap_rgun_fire_plr`, fireLastPlr=`weap_rgun_fire_plr`, emptyFirePlr=`dryfire_pistol_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_small_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`, projectile=`weap_rgun_loop`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_ray_gun_reload` | 119 f @ 30 fps = 3.97s | 3000 ms | f12 / t0.101 / **0.30s** `ray_reload_open`<br>f39 / t0.328 / **0.98s** `ray_reload_battery_out`<br>f71 / t0.597 / **1.79s** `ray_reload_battery_in`<br>f95 / t0.798 / **2.39s** `ray_reload_close` |

#### `m7_launcher`

Player sound fields: firePlr=`weap_m1gren_fire_plr`, fireLastPlr=`weap_m1gren_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_m1garand_grenade_reload` | 64 f @ 24 fps = 2.67s | 1500 ms | f17 / t0.266 / **0.40s** `gr_m1gren_futz`<br>f20 / t0.312 / **0.47s** `gr_m1gren_load` |
| firstRaise | `viewmodel_m1garand_grenade_first_pullout` | 70 f @ 24 fps = 2.92s | 2200 ms | f24 / t0.343 / **0.75s** `gr_m1gren_futz`<br>f33 / t0.471 / **1.04s** `gr_m1gren_load` |
| altRaise | `viewmodel_m1garand_grenade_alt_pullout` | 80 f @ 24 fps = 3.33s | 2200 ms | f32 / t0.400 / **0.88s** `gr_m1gren_load`<br>f38 / t0.475 / **1.04s** `gr_m1gren_futz` |
| altDrop | `viewmodel_m1garand_grenade_alt_putaway` | 89 f @ 24 fps = 3.71s | 2000 ms | f35 / t0.393 / **0.79s** `gr_m1gren_remove` |

#### `stielhandgranate`

Player sound fields: firePlr=`foley_throw`, raisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`, pullbackPlr=`grenade_pull_pin`

_No sound notetracks._

#### `molotov`

Player sound fields: firePlr=`weap_molotov_throw`, raisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`, pullbackPlr=`weap_molotov_light`

_No sound notetracks._

---------------------------------------------------------------------------------------------------------------------

## 3. Game / round flow and music

Music system: server `setmusicstate(state)` (C `maps/_music.gsc:12`) → client `clientscripts/_music.csc` state machine. The Nacht states
are declared in N `clientscripts/nazi_zombie_prototype_amb.csc:37-43`:

| state | content | semantics (C `_music.csc`) |
|---|---|---|
| `SPLASH_SCREEN` | `musicAlias("mx_splash_screen", 12)`, `musicWaitTillDone()` | one-shot. When leaving the state, it waits until the one-shot finishes (`transitionOut`, lines 179-187) |
| `WAVE_1` | `musicAliasLoop("mx_zombie_wave_1", 0, 4)` | looping alias, fade-in 0, fade-out 4 s |

| moment | what plays | alias (storage) | source | kind |
|---|---|---|---|---|
| level load | ambient package `zombies`: `amb_spooky_2d` every 5–8 s (dist 300–2000), room reverb `stoneroom` | `amb_spooky_2d` ×5 inline | N `nazi_zombie_prototype_amb.csc:22-33` | ambience |
| t = 0 (`all_players_connected`) | `round_think` → `setmusicstate("WAVE_1")` | `mx_zombie_wave_1` | P `_zombiemode_prototype.gsc:1174` | music |
| t ≈ 1.0 s | intro: `setmusicstate("SPLASH_SCREEN")`; 0.2 s later `setmusicstate("WAVE_1")` | `mx_splash_screen` (stream, 11.1 s, vol 1.0) | P `_zombiemode_prototype.gsc:391-402` | music |
| after the splash ends (≈12 s) | `WAVE_1` loop starts (it fades out the early WAVE_1 over 4 s when the splash starts; ASSUMED net effect) | `mx_zombie_wave_1` (stream `mx_wave_1.wav`, 44.1 s, **looping, vol 0.06**) | `_music.csc:209-265` | music, plays for the rest of the game |
| round 1 start | "Round" text fades in (1 s), wait 1 + 0.5 s → `chalk` | `chalk` (= round_over.wav, 3.9 s) | P `_zombiemode_prototype.gsc:1052` (`chalk_one_up`, `PlaySoundAtPosition(…,(0,0,0))`) | SFX |
| round N>1 start | chalk HUD fades 0.5 s → `chalk` | `chalk` | same | SFX |
| round end (last zombie dead) | `chalk_round_hint`: wait `between_round_time × 0.25` = **2.5 s** → `round_over`; next round starts 10 s after the round end | `round_over` (= chalk_v2.wav, 2.8 s) | P `_zombiemode_prototype.gsc:1122`, `:1209` | SFX |
| game over (all players down) | "GAME OVER" fades in, wait 1 s → `mx_game_over`; wait 2 s → intermission | `mx_game_over` (stream 25.8 s, **not** looping) | P `_zombiemode_prototype.gsc:1735` | music played as SFX |
| intermission | `lsm`=1 → client heartbeat loop: `heart_beat` every 0.5→2.0 s, volume 0.5→1.0 (×1.05 per beat), bus `zombie_death` | `heart_beat` (**xref** → common) | P `:2156`, C `clientscripts/_load.csc:33-88` | SFX |
| player downed (laststand, coop) | same heartbeat loop; on revive `revive_gasp` | `heart_beat`, `revive_gasp` (inline ×4) | C `maps/_laststand.gsc:347`, C `_load.csc:30` | SFX |

Nacht-specific vs later maps (VERIFIED in the asylum/factory scripts): Verrückt and later add the music states `round_begin`
(`musicAlias("chalk")`), `round_end`, `mx_dog_round`, `end_of_game`, `dog_round_start`, and powerup VO (`nuke_vox`, `insta_vox`, `dp_vox`). None of these exist in
Nacht, whose WAVE_1 loop runs unchanged from the splash to the end. `mx_game_over2.wav` and `ANN_*.wav` are later-map assets.

---------------------------------------------------------------------------------------------------------------------

## 4. Gameplay sound events

### 4.1 Purchases, wall-buys, box, debris

| event | alias(es), order | where it plays | script |
|---|---|---|---|
| wall-buy, new weapon (enough points) | `cha_ching` on the **player** (`weapon_give`) | player | N `_zombiemode_weapons.gsc:891` |
| wall-buy, first purchase of that wall weapon (anyone, weapon or ammo) | + `weap_wall` at the chalk model when it slides out (`weapon_show`) | model | N `_zombiemode_weapons.gsc:849` |
| wall-buy ammo (already owned, ammo not full) | `cha_ching` on the player (`ammo_give`); ammo already full → nothing, no charge | player | N `_zombiemode_weapons.gsc:945` |
| wall-buy, not enough points | `no_cha_ching` on the trigger | trigger | N `_zombiemode_weapons.gsc:784`, `:819` |
| box: buy (950) | **no cha_ching at purchase**; `lid_open` + `music_box` together at the lid | lid | N `_zombiemode_weapons.gsc:400-401` |
| box: cycling (≈4 s, 40 model swaps 0.05→0.3 s) | nothing else (the music_box one-shot is the jingle) | – | N `:472-494` |
| box: weapon taken | `cha_ching` on the player, then `lid_close` | player / lid | N `:564`, `:410` |
| box: 12 s timeout | weapon sinks back, `lid_close` | lid | N `:389-391`, `:410` |
| box: not enough points | nothing (the trigger just keeps waiting) | – | N `:266-281` |
| box teddy bear / moving box | **does not exist in Nacht** | – | – |
| debris (upstairs, cost 1000), bought | `cha_ching` at the trigger | trigger | P `_zombiemode_blockers.gsc:361` |
| debris, not enough points | `no_cha_ching` at the trigger | trigger | P `:425` |
| debris pieces move (2 pieces, "jiggle" then fly to a struct) | per piece: `weap_wall` (on piece) + `lightning_l` (**missing alias**) → jiggle 3–4× 0.1–0.4 s → move 0.5 s → `couch_slam` (= board_slam wavs) → `zombie_spawn` (**missing alias**) + `large_ceiling_dust` fx → delete | piece | P `:437-478` |
| (unpatched N version) | `weap_wall` → … → `couch_slam` + one-shot `couch_loop` (whisper) | piece | N `_zombiemode_blockers.gsc:339`, `:373-374` |
| zombie_door open | `cha_ching` + `door_slide_open` — **no zombie_door entities in Nacht** | – | P `:160-206` |

### 4.2 Power-ups (N `maps/_zombiemode_powerups.gsc`)

Nacht includes only nuke, insta_kill, double_points and full_ammo (P `nazi_zombie_prototype.gsc` `include_powerups`). **There is no announcer.**

| event | alias(es) | where | line |
|---|---|---|---|
| drop spawns | `spawn_powerup` at the drop, then `PlayLoopSound("spawn_powerup_loop")` on the model (loop, vol 0.25) | world | 214, 224 |
| grabbed (within 64 u) | effect function runs; 0.1 s later `powerup_grabbed` at the drop, loop stopped | world | 269-272 |
| timeout (15 s + 40 blinks) | loop ends with the model delete (no sound) | – | 310-344 |
| full_ammo | `full_ammo` on every player (vol 0.7) | player | 623 |
| insta_kill | `insta_kill_loop` on a script_origin at (0,0,0) for 30 s (vol 0.15); **at the end** `insta_kill` on every player, loop fade-out 2 s | (0,0,0) / player | 496, 519, 522 |
| double_points | `double_point_loop` at (0,0,0) for 30 s; at the end `points_loop_off` on every player, loop fade-out 2 s | (0,0,0) / player | 568, 584, 586 |
| nuke | `nuke_flash` at (0,0,0) (white flash); then for each zombie (closest first, 0.1–0.7 s apart) head gib (`zombie_head_gib`) + `nuked` at the zombie | world | 379, 370-373 |

### 4.3 Barriers (windows), P `_zombiemode_blockers.gsc` + P `_zombiemode_spawner_prototype.gsc`

| event | alias(es) | trigger | line |
|---|---|---|---|
| zombie starts pulling a board | `remove_boards` (zombie anim notetrack `sndnt#remove_boards`, see §6) | tear anim | anim data |
| board comes off | `break_boards` on the chunk (`break_barrier_piece`), fx `wood_chunk_destory`, small earthquake | tear-anim notetrack `board`/`board_one…six` → `remove_chunk` | 755 (spawner :539-549) |
| player repairs a board (hold use, every ~1 s) | `repair_boards` on the chunk → `boards_float` at the chunk (hover) → move 0.1–0.3 s + rotate +0.25 s, wait 0.2 s, snap 0.15 s → `board_slam` at the chunk | `blocker_think` / `replace_chunk` | 683, 882, 914 |
| **stone window** (exterior_goal at (450,1062,18), chunks `auto1`) | its 8 chunks have `script_sound "break_stone"`, which overrides `play_sound_on_ent`: **both break and repair play `break_stone`**. Hover/slam are unchanged | map entity key | utility :641-645 |
| points for repair | no extra sound | – | – |

### 4.4 Zombies and combat

| event | alias(es) | trigger | location |
|---|---|---|---|
| zombie ambient/walk | `amb_vocals` (×21, vol 0.4–0.5, dist 50–2500) at frame 0 of `ai_zombie_walk_v1..v4`, `ai_zombie_idle_v1_delta` → once per anim cycle | notetrack | anim data |
| sprinting | `sprint_vocals` (×15): sprint_v1 f9, sprint_v2 f38; footsteps `step_zombie` (×4) at every foot plant of sprint/walk_fast | notetrack | anim data |
| crawler | `crawl_vocals` (crawl_sprint f9), `crawl_vocals_slow` (crawl_v1 f35), `crawl_hands` at each hand plant | notetrack | anim data |
| walk foot plants | `footstep_*` notetracks → C `clientscripts/_footsteps.csc:1-15`: `step_run_<surface>` (dirt/wood/concrete…, inline ×6) + `gear_rattle_run` (xref → null.wav, silent); walk_v1/v4 also `step_sweetner` at f0 | notetrack | C `animscripts/shared.gsc:520-525`, `:1103-1140` |
| attack (window or player) | `attack_vocals` at f0, `attack_whoosh` at each swing, damage on `fire` (e.g. attack_v1: whoosh f20/f34, fire f25/f39) | notetrack | anim data |
| crawler traverse landing | `bodyfall large` → `bodyfall_<ground>_large` (dirt: inline ×5) | notetrack | C `animscripts/shared.gsc:802-813` |
| head pop (health < 10 %, head/helmet/neck hit by a rifle/pistol bullet (not a pistol-class weapon) or a shotgun, a grenade airburst within 55 u, or a projectile within 10 u; also insta-kill/nuke) | `zombie_head_gib` on the zombie (vol 1, dist 2000–4000) | `zombie_head_gib()` | P `_zombiemode_spawner_prototype.gsc:607`, conditions 648-724 |
| limb gib | `death_gibs` (×4) at the zombie | `animscripts\death::do_gib` | C `animscripts/death.gsc:2082` |
| zombie death | **silent**. Zombies get a `deathanim`, so `animscripts/death.gsc` returns before `PlayDeathSound` (which would need `generic_death_<voice>`, not in the zone) | – | C `animscripts/death.gsc:84-118`, `:513-520` |
| headshot impact | `bullet_impact_headshot` / `_helmet` only on the non-deathanim death path, so normally not heard. The **headshot sound in Nacht is effectively `zombie_head_gib`** | – | C `death.gsc:144-156` |
| hit marker | **none**: `_damagefeedback` is disabled (`scr_damagefeedback` 0) and `SP_hit_alert` is not in the zone | – | C `maps/_damagefeedback.gsc:7-11` |
| player knife | §2.4 | | |
| zombie spawns | **no sound** (no script plays one; the alias `zombie_spawn` does not even exist) | – | – |

### 4.5 Player

| event | alias(es) | source |
|---|---|---|
| hurt (low health) | `breathing_hurt` looped (0.3–0.7 s gaps) while health/maxHealth ≤ `healthOverlayCutoff`; `breathing_better` when it recovers | C `maps/_gameskill.gsc:1590-1628`, `:2015` → C `clientscripts/_load.csc:155-156`; both **xref** |
| hit while ADS | `player_hit_while_ads` (only if dvar `stuntime` ≠ 0) | C `maps/_load.gsc:3556`, **xref** |
| per-hit pain grunt | no script plays one. `player_pain_small` exists (×8, **xref**), so ASSUMED engine-driven | – |
| downed / game over | heartbeat (§3); `mx_death`/`mx_death_rear` (C `_callbackglobal.gsc:589`) are **not** reached because zombiemode fakes the death (`player_fake_death`) | P `:1652-1688` |
| out of ammo click | weapon `emptyFireSoundPlayer` (`dryfire_*_plr`) | WeaponDef |
| "low ammo" warning | none in Nacht | – |
| weapon pickup | not used (purchases use GiveWeapon); first raise uses `firstRaiseSoundPlayer` + firstRaise notetracks (mp40 `gr_mp40_charge_plr`, m1garand slide/breech) | WeaponDef |

---------------------------------------------------------------------------------------------------------------------

## 5. Ambience (client-side, C `clientscripts/_audio.csc`; data in `nacht_emitters.json`)

| emitter | count | alias (storage) | behaviour |
|---|---|---|---|
| ambient package `zombies` | 1 | `amb_spooky_2d` (inline ×5, vol 0.5–0.75) | random 2D one-shot every 5–8 s (dist 300–2000) |
| `script_label random` | 43 | `amb_spooky` (inline ×15, vol 0.5–0.85, dist 150–1500) | `wait RandomFloatRange(4, 14)` then a one-shot at the struct (`_audio.csc:6-36`) |
| `script_label looper` | 8 / 5 | `light` (light-bulb hum, loop) / `fire_med` (fire loop) | `playloopat` forever at the struct (`:40-53`) |
| `script_label line_emitter` + `script_looping` | 2 | `amb_zombies_left`, `amb_zombies_right` (**stream**, iw_15.iwd, 115 s, loop, dist 200–2500) | looping sound whose source slides along the segment start→`target` struct, kept at the point on the line closest to the listener (`:56-243`) |
| radio `kzmb` | 0 | (12 songs declared) | no `kzmb` entity in Nacht → inert |

---------------------------------------------------------------------------------------------------------------------

## 6. AI notetrack convention (zombie anims, VERIFIED data)

`sndnt#<alias>` = play alias `<alias>` on the AI (engine behaviour, ASSUMED). Other notetracks are animscript events (`fire`, `board*`,
`footstep_*`, `bodyfall large`, `start_ragdoll`, `gravity on`, `blend`). Full list with frames (30 fps):

| notetrack | anims @ frame |
|---|---|
| `sndnt#amb_vocals` | walk_v1..v4 @0, idle_v1_delta @0 |
| `sndnt#step_sweetner` | walk_v1 @0, walk_v4 @0 |
| `sndnt#step_zombie` | sprint_v1 @6,15,26,37,47,58; sprint_v2 @7,17,26,37,46,57; walk_fast_v1 @8,22,34,47,61,75; walk_fast_v2 @8,21,32,44,57,68; walk_fast_v3 @7,20,31,43,52,63 |
| `sndnt#sprint_vocals` | sprint_v1 @9, sprint_v2 @38 |
| `sndnt#attack_vocals` | attack_v1 @0, attack_v2 @0,96, attack_forward_v1/v2 @0, door_tear_v1 @0, door_tear_right @58 |
| `sndnt#attack_whoosh` | attack_v1 @20,34; attack_v2 @14,63,109,130,154; attack_forward_v1 @25,36; attack_forward_v2 @17,38,56 |
| `sndnt#remove_boards` | door_tear_v1 @8,41,75,109,167,193,227; door_tear_high @27, left @31, right @28 |
| `sndnt#crawl_hands` | crawl_v1 @27,56,87,121; crawl_sprint @10,20,32,42,57,69 |
| `sndnt#crawl_vocals` / `_slow` | crawl_sprint @9 / crawl_v1 @35 |
| `board` (→ break_boards) | door_tear_high @35, left @38, right @38, low @33; attack_crawl @30; attack_crawl_lunge @16 |

---------------------------------------------------------------------------------------------------------------------

## 7. Comparison with `sounds.cfg` and the current code

### 7.1 `sounds.cfg` mismatches

| key | current | Nacht reality | fix |
|---|---|---|---|
| `game_start` | `mx_wave_1.wav` (= the WAVE_1 loop file) | `mx_splash_screen` one-shot (11.1 s), then the `mx_zombie_wave_1` **loop** for the whole game (vol 0.06) | `alias:mx_splash_screen`, and add a separate music loop `alias:mx_zombie_wave_1` started after the splash |
| `round_start` | `mx_wave_1.wav` | SFX `chalk` (file round_over.wav) | `alias:chalk` |
| `round_end` | `alias:round_over \| …chalk_v2.wav` | `round_over` correct (both entries are the same file). Timing: 2.5 s after the last kill | ok; delay 2.5 s |
| `wall_buy` | `alias:cha_ching \| …chalk_v2.wav` | `cha_ching` (+ `weap_wall` on the first buy of that wall weapon). chalk_v2.wav is the round-over sound | drop the chalk_v2 fallback; add `weap_wall` |
| `purchase` | `alias:cha_ching` | correct (door/debris/ammo/box grab) | ok |
| `deny` | `alias:no_cha_ching \| …asylum/door/deny/deny_00.wav` | `no_cha_ching` only; deny_00 is Verrückt | drop the fallback. Box with too few points: no sound |
| `game_over` | mx_game_over.wav \| mx_game_over2.wav | `mx_game_over` only (one-shot, 1 s after the GAME OVER text), then the heartbeat | `alias:mx_game_over`; drop game_over2 |
| `door_open` | `alias:door_slide_open` | Nacht's only "door" is debris: `weap_wall` → (jiggle/move) → `couch_slam` | use `weap_wall`, then `couch_slam` when the pieces land |
| `crate_open` | (not configured → synth) | `lid_open` + `music_box` | add |
| `crate_ready` | lottery_laugh.wav (Shi No Numa+) | no "ready" sound in Nacht | remove |
| (box close) | – | `lid_close` on grab/timeout | add |
| `zombie_groan` | `alias:attack_vocals` | ambient = `amb_vocals`; sprinters `sprint_vocals`; crawlers `crawl_vocals(_slow)`; all once per anim cycle | `alias:amb_vocals` (better: drive from notetracks, §6) |
| `zombie_attack` | `alias:attack_whoosh` | `attack_vocals` at attack start + `attack_whoosh` per swing | add `attack_vocals` at swing start |
| `zombie_death` | `death_gurgle \| death_gibs` | `death_gurgle` = **sfx/null.wav (silent)**. Deaths are silent; `death_gibs` only on a limb gib | remove gurgle; play `death_gibs` only on gib |
| `zombie_spawn` | `alias:mortar_dirt_debris` | no spawn sound in Nacht | remove |
| `board_tear` | `break_boards \| remove_boards` (random) | **sequence**: `remove_boards` (zombie, ~0.2 s before) then `break_boards` (chunk); stone window = `break_stone` | split into two events |
| `board_repair` | `repair_boards \| board_slam` (random) | **sequence**: `repair_boards` + `boards_float` (together) → `board_slam` ~0.7–0.9 s later; stone window `break_stone` instead of repair_boards | split into three |
| `headshot` | `zombie_head_gib` | correct, but only on the head pop (≤10 % health…), not on every head hit | gate it |
| `knife` | `knife_stab_plr \| melee_swing_gear_plr` | swing = weapon `meleeSwipeSoundPlayer` (`melee_swing_plr` / `melee_swing_small_plr`), hit = `melee_hit`; `knife_stab_plr`/`knife_pull_plr` only on the lunge | per-weapon fields |
| `player_hurt` | `alias:player_pain_small` | engine-driven (ASSUMED); alias is **xref** (data in common.ff); plus the `breathing_hurt` loop at low health | needs the xref fix (§7.3) |
| `powerup_spawn` | `spawn_powerup` | correct, **plus** the looping `spawn_powerup_loop` on the drop | add loop |
| `powerup_grab` | `powerup_grabbed` | correct (0.1 s after the grab) | ok |
| `max_ammo` | ANN_MaxAmmo.wav | `full_ammo` | `alias:full_ammo` |
| `insta_kill` | ANN_InstaKill.wav | `insta_kill_loop` for 30 s, then `insta_kill` at the **end** | loop + end sound |
| `double_points` | ANN_DoublePoints.wav | `double_point_loop` 30 s, `points_loop_off` at the end | loop + end sound |
| `nuke` | ANN_Nuke.wav | `nuke_flash` + per zombie `nuked` + `zombie_head_gib` | as described |
| `carpenter` | ANN_carpenter.wav | Nacht has no carpenter | disable the power-up in Nacht |

### 7.2 Code (`crates/zm_game/src`)

* `weapons.rs:186/195`: `Sfx::Reload` at reload start and end (synth). It should instead play the per-weapon `reloadSoundPlayer` at start (if set)
  plus the viewmodel **notetrack** events at `t × iReload(Empty)Time` (§2.5). The same applies to rechamber (bolt/pump), first raise, raise/putaway,
  deploy/breakdown and shotgun start/loop/end. `XAnimInfo.notify` already carries the data (`nacht/build.rs:107`) but nothing reads it for sound.
* `weapons.rs:257`: `Sfx::DryFire` (synth) → weapon `emptyFireSoundPlayer`.
* `weapons.rs:353`: `Sfx::Hit` hitmarker sound does not exist in Nacht. `Sfx::Headshot` should be tied to the head-pop condition.
* Fire: `weapon_fire_sounds` uses `fireSoundPlayer`, which is right. It does not use `fireLastSoundPlayer` (only differs for m1garand).
* `zombies.rs:508`: groan on a random 3–8 s timer → ideally `amb_vocals` on each walk-cycle start (or keep the timer but use `amb_vocals`).
* `round.rs:49`: random `ZombieSpawn` sound → not in Nacht. `round.rs:54`: GameStart/RoundStart → `chalk` (+ music start at game start).
* `powerups.rs:45`: `ZombieDeath` sound on **every** kill → Nacht is silent here.
* No music loop, no ambient emitters (§5), no box lid close, no power-up loops, no breathing/heartbeat.
* `interact.rs:197-198`: door = DoorOpen + Purchase → debris sequence (§4.1).

### 7.3 Asset-loading pitfalls

* **Cross-zone LoadedSounds**: `heart_beat`, `breathing_hurt`, `breathing_better`, `player_pain_small`, `player_hit_while_ads` and `gear_rattle_run`
  are present in Nacht as alias lists, but their LoadedSound is a `,name` reference with no data (`LoadedSoundInfo.len == 0`).
  `collect_sounds` (`nacht/build.rs:624-648`) stops at the first zone that has the alias (Nacht) and then gets no PCM. Fix: for
  `Loaded(i)` with `len == 0`, look the LoadedSound up **by name** (minus the leading `,`) in common.ff.
* Streamed music exists in several IWDs (`iw_20/22/23/25/27`). The highest-numbered IWD wins; the files are identical anyway.
* Alias variant choice: many aliases have several entries (`attack_vocals` ×23). Pick one at random and apply `volMin..volMax` / `pitchMin..pitchMax`
  (the current code ignores vol/pitch, e.g. `mx_zombie_wave_1` is authored at volume **0.06**).

---------------------------------------------------------------------------------------------------------------------

### 7.4 xWMA sounds (found while implementing; VERIFIED)

* 336 of Nacht's inline LoadedSounds are **xWMA** (RIFF `XWMA`, WMA v2 tag 0x161, chunks `fmt `, `PRIV` (2042 B, Treyarch), `dpds`, `data`),
  among them `chalk`, `round_over`, `cha_ching`, `weap_wall`, `music_box`, `repair_boards`, `boards_float`, `nuke_flash`, `points_loop_off`,
  `amb_spooky_2d` and `breathing_better`. Only 2 of them also exist as plain files in the IWDs.
* Packets are `nBlockAlign` bytes (2230 mono, 4459 stereo: standard xWMA sizes). The fmt has no codec bytes (cbSize 0). For WMA v2 the decoder needs
  `samplesPerBlock` (2048 at 44.1 kHz), `encodeOptions` **0x1F** and `superBlockAlign`.
* **The header bitrate is wrong for mono files.** They declare `nAvgBytesPerSec` 12000 but are encoded at **6000**. WMA derives its bitstream layout
  from the bitrate, so decoding with 12000 gives silence. The real rate follows from the `dpds` total:
  `data_len × (2 × channels × rate) / dpds_last` = 6402 for `cha_ching` → snap to the allowed xWMA rates → 6000.
* With those values the Windows Media Foundation WMA decoder (`CWMADecMediaObject`) decodes everything; `chalk` matches the IWD's ADPCM
  `round_over.wav` with envelope correlation 0.99998. The remake does this in `crates/zm_game/src/xwma.rs`.
* Durations in §3/§8 that were estimated as `data_bytes / nAvgBytesPerSec` are too short. Real lengths (decoded): `chalk` 14.0 s, `cha_ching` 2.8 s,
  `music_box` 7.8 s, `weap_wall` 3.0 s, `lid_close` 1.8 s, `full_ammo` 2.7 s, `insta_kill` 2.2 s.

## 8. Alias properties (VERIFIED from the zone; the "2D" column is ASSUMED from flag bit 0x40 = 3D/spatialized)

`flags` bit 0 = looping (VERIFIED: set exactly on the loop aliases). Bit 0x40 is set on world sounds and absent on `_plr`, `_2d`, music and UI sounds,
so it is read as "3D" (ASSUMED). Durations are approximate for XWMA.

| alias | vol | dist min–max | loop | 3D | length |
|---|---|---|---|---|---|
| mx_splash_screen | 1.0 | – | no | no (2D) | 11.1 s |
| mx_zombie_wave_1 | **0.06** | – | **yes** | no | 44.1 s |
| mx_game_over | 1.0 | – | no | no | 25.8 s |
| chalk / round_over | 0.7 | 250–2500 | no | no | 3.9 / 2.8 s |
| cha_ching / no_cha_ching | 1.0 | 250–2500 | no | yes | 0.4 / 0.1 s |
| lid_open / music_box / lid_close | 1.0 | 250(500)–2500 | no | yes | 1.0 / ~1.2 / 0.9 s |
| weap_wall | 1.0 | 250–2500 | no | yes | 0.5 s |
| spawn_powerup / spawn_powerup_loop | 0.8 / 0.25 | 500–4000 / 50–800 | no / yes | yes | 0.5 / 1.5 s |
| powerup_grabbed, nuke_flash, insta_kill, full_ammo, points_loop_off | 0.6–1.0 | – | no | no | 1.3 / 0.4 / 2.3 / 2.8 / 1.8 s |
| insta_kill_loop / double_point_loop | 0.15 | – | yes | no | 1.7 / 2.5 s |
| nuked | 1.0 | 500–4000 | no | yes | 0.9 s |
| attack_vocals / amb_vocals | 1.0 / 0.4–0.5 | 500–2500 / 50–2500 | no | yes | – |
| zombie_head_gib | 1.0 (pitch 0.9–1.08) | 2000–4000 | no | yes | 0.5 s |
| break_boards / board_slam / repair_boards | 1.0 | 1000–5000 / 250–2500 / 250–2500 | no | yes | 0.3 / 1.0 / 0.6 s |
| gr_kar98_bolt_up_plr (typical notetrack) | 0.75–0.8 | – | no | no | – |
| weap_kar98k_fire_plr / weap_kar98k_fire | 0.87–0.94 / 0.8–0.9 | – / 250–2000 | no | no / yes | – |
| amb_spooky / amb_spooky_2d | 0.5–0.85 / 0.5–0.75 | 150–1500 / – | no | yes / no | – |
| light / fire_med / amb_zombies_left/right | 0.65–0.85 / 1.0 / 0.8 | 100–800 / 50–500 / 200–2500 | yes | yes | – / – / 115 s |

## 9. Open items / ASSUMED summary

* Engine code paths were not disassembled (clean room): the notetrackSoundMap lookup, the anim-rate scaling to the state time, which WeaponDef sound plays at
  which moment, `sndnt#` handling, the player pain grunt and the meaning of the flag bits are inferred.
* patch.ff vs map-ff precedence for identically named rawfiles is inferred from intent. The sound differences between the two blocker versions are
  only in the debris sequence (§4.1).
* Music state ordering at t≈0–1 s (WAVE_1 set by `round_think` before the intro's SPLASH_SCREEN) depends on the client-sys sync. The audible
  result is the splash followed by the quiet loop either way.
