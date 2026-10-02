# Zombie, round and points mechanics (World at War, Nacht der Untoten)

Clean-room research note for Undead Rounds. Every value comes from the game's own scripts and data in the user's install,
read for behaviour only; the wiki was used as a cross-check. Where the two disagree, the scripts win and the difference
is noted. Implemented in `crates/zm_core/src/rules.rs` (`ZombieRules::nacht()`, `RoundState`, `PlayerHealth`,
`PowerupDrops`).

**VERIFIED** = read in a script or in the game data. **ASSUMED** = engine behaviour (code in `CoDWaW.exe`, not read) or an
interpretation, with the reason given. **WIKI** = from the CoD wiki only.

## 0. Sources and which scripts run

| tag | what | where |
|---|---|---|
| **P** | `patch.ff` rawfiles: `maps/nazi_zombie_prototype.gsc`, `maps/_zombiemode_prototype.gsc`, `maps/_zombiemode_spawner_prototype.gsc`, `maps/_zombiemode_blockers.gsc` | `zone/english/patch.ff`, dumped with `research/audio/extract_rawfiles.py` (line numbers below are of those dumps) |
| **N** | Nacht's own fastfile scripts: `_zombiemode_powerups.gsc`, `_zombiemode_score.gsc`, `_zombiemode_utility.gsc`, `_zombiemode_weapons.gsc` | `D:\Decompile Test\extracted\fastfiles\nazi_zombie_prototype\raw\maps\` |
| **C** | `common.ff`: `maps/_gameskill.gsc`, `maps/_laststand.gsc`, `animscripts/melee.gsc`, `animscripts/init.gsc`, string table `mp/zombiemode.csv` | `zone/english/common.ff` |
| **A** | Nacht zone data: weapon defs (`iMeleeDamage`), xanim notetracks and root motion | `nazi_zombie_prototype.ff`, `research/t4/xanim_list_nacht.txt`, `research/t4/T4_XANIM_NOTES.md` §8 |
| **UI** | `ui.ff` menus | how the mode is launched (difficulty) |

The patched level script calls `maps\_zombiemode_prototype::main()` (P `nazi_zombie_prototype.gsc:17`), so the P copies are
the ones that run (see also `research/audio/NACHT_AUDIO.md` §0). The map fastfile's `_zombiemode.gsc` /
`_zombiemode_spawner.gsc` are older versions and are not used. There is no `nazi_zombie_prototype_patch.ff`.

`set_zombie_var( name, value, div )` (N `_zombiemode_utility.gsc:687-704`) first looks the name up in `mp/zombiemode.csv`
(column 0 → column 1) and, if found, uses `int(table value)`; then divides by `div`. The table in this install (C) holds
the same numbers as the scripts, plus `zombie_powerup_*` (2000, 4, 30, 30) and `rebuild_barrier_cap_per_round` 500; its
`zombie_score_start` row is misspelt `mbie_score_start` and never matches. VERIFIED (table dumped from common.ff).

**Runtime reading:** the game build (`zm_game/src/nacht/build.rs::zombie_rules`) now reads the level script from patch.ff,
finds the zombie-mode script it runs, parses its `set_zombie_var` calls (plus `_zombiemode_powerups.gsc`), applies the
`mp/zombiemode.csv` overrides (string tables are now kept by `waw_assets::t4`), and the `include_powerup` list. The
formulas (round multipliers, 0.95 decay, ×8 speed step…) are code in `ZombieRules`, with Nacht's numbers.

GSC arithmetic notes used below: `int / int` gives a float when not exact (proof: `10 / 100` must be 0.1 or zombie health
would stop growing after round 9); `Int()` truncates; script ints are 32-bit, floats are f32. VERIFIED by the outcome,
ASSUMED for the VM internals.

## 1. Zombie health

| value | Nacht | source | status |
|---|---|---|---|
| start health | 150 | P `_zombiemode_prototype.gsc:272` (`zombie_health_start`), P `_zombiemode_spawner_prototype.gsc:9` | VERIFIED |
| rounds 2–9 | `+100` per round (`zombie_health_increase`) | P `:270`, `ai_calculate_health` P `:1261-1275` | VERIFIED |
| round ≥ 10 | `health += Int(health * 0.1)` (`zombie_health_increase_percent` 10/100) | P `:271`, `:1264-1267` | VERIFIED |
| each zombie | `maxhealth = health = level.zombie_health` at spawn | P spawner `:94-95` | VERIFIED |
| overflow | 32-bit wrap at round 163; zombies then have round-1 health | f32/i32 emulation hits it exactly at 163 (test) / WIKI for the after-effect | ASSUMED (after-effect) |

Health per round (the script truncates every round, so it is a little under the wiki's `950 × 1.1^n`):

| round | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 15 | 20 | 30 | 50 | 100 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| health | 150 | 250 | 350 | 450 | 550 | 650 | 750 | 850 | 950 | 1045 | 1149 | 1679 | 2701 | 7000 | 47073 | 5525295 |

Wiki says 1683 (r15), 7030 (r30), 47295 (r50): those are the untruncated formula. Scripts win.

Insta-kill: while on, any player damage the damage script sees (every non-lethal hit, any weapon, grenade splash too) is
followed by `zombie_head_gib` + `DoDamage(health + 666)` (N `_zombiemode_powerups.gsc:452-459`, called from P spawner
`:1194`, `:1216`). VERIFIED.

## 2. Rounds and spawning

| value | Nacht | source | status |
|---|---|---|---|
| zombies per round | `max = zombie_max_ai (24) + int((players - 1) × 6 × mult)`, `mult = max(round/5, 1)`, ×`round × 0.15` from round 10 | P `:826-840` | VERIFIED |
| early rounds | round 1 (`first_round`) ×0.2, round 2 ×0.4, round 3 ×0.6, round 4 ×0.8, `int()` each | P `:842-857` | VERIFIED |
| **solo** | 4, 9, 14, 19, then **24 every round** (the per-player term is 0 alone) | as above; WIKI agrees ("maps before Der Riese have a maximum of 24 zombies per round on solo") | VERIFIED |
| Der Riese difference | counts 0.5 player when alone (`0.5 × zombie_ai_per_player × mult`) → 33 at r10, 60 at r20 | `nazi_zombie_factory_patch` `_zombiemode.gsc:1487` | VERIFIED (kept as `solo_player_factor`) |
| max alive | `SetAILimit( 24 )`; a spawn attempt at the limit fails and is retried after the spawn delay | P `:25`, `:861-888` | VERIFIED (retry: spawn_zombie returns undefined, count not increased) |
| spawn delay | `zombie_spawn_delay` 3 s in round 1; after every round `d = max(d, 0.08) × 0.95` → 2.85, 2.71…; floor 0.076 s | P `:267`, `:1212-1220` | VERIFIED |
| first spawn, round 1 | 6.75 s after the round starts (intro chalk: 1 + 0.5 + 3 + 0.25 + 2 s) | `chalk_one_up` P `:987-1102` | VERIFIED |
| first spawn, later rounds | 0.5 s after the round starts (chalk fade) | P `:1041-1050` | VERIFIED |
| between rounds | 10 s (`zombie_between_round_time`) from the last kill; `round_over` sound at 2.5 s | P `:263`, `:1208-1210`, `:1121-1122` | VERIFIED |
| round start | round 1 starts as soon as all players are connected | P `:80-85`, `:940-964` | VERIFIED |
| failsafe | every 30 s, a zombie that moved < 24 units (and tore no board in the last 5 s) or fell below z −1000 is killed | P `:1279-1323` | VERIFIED (not implemented: movement code) |
| "last zombie" behaviour | none in Nacht (no crawler/sprinter special case) | whole P scripts | VERIFIED |
| later-round spawners | `later_round_spawners` with `script_start` join from that round; spawners behind a door/debris only once it opens | N utility `:445-475`, P spawner `:11-24` | VERIFIED (Nacht's map has none of the former) |

Spawn order inside `RoundState`: round start → health update → count → wait → spawn, wait delay, spawn…

## 3. Movement speed

| value | Nacht | source | status |
|---|---|---|---|
| `zombie_move_speed` | 1 in round 1; after round N ends: `N × 8` (so round r uses `(r-1) × 8`) | P spawner `:8`, P `:1223` | VERIFIED |
| per zombie | `rand = RandomIntRange(speed, speed + 35)` (35 values); ≤ 35 walk, ≤ 70 run, else sprint | P spawner `:192-209` | VERIFIED |
| anim per gait | walk: 4 walk cycles, run: `walk_fast_v1..3`, sprint: `sprint_v1/v2` | P spawner `:117-190`, P `:346-355` | VERIFIED |
| ground speed | walk 37–48 u/s (avg 40.5 = 1.03 m/s), run 65–81 (74.7 = 1.90 m/s), sprint 139–142 (140.5 = 3.57 m/s) | anim root motion, A (`T4_XANIM_NOTES.md` §8) | VERIFIED (AI move rate = anim root speed: ASSUMED engine default playback 1.0) |

Gait odds per round:

| round | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10+ |
|---|---|---|---|---|---|---|---|---|---|---|
| walk | 100% | 80% | 57% | 34% | 11% | 0 | 0 | 0 | 0 | 0 |
| run | 0 | 20% | 43% | 66% | 89% | 89% | 66% | 43% | 20% | 0 |
| sprint | 0 | 0 | 0 | 0 | 0 | 11% | 34% | 57% | 80% | 100% |

Crawlers: Nacht has them only through gibbing. A non-lethal hit of at least 10% of the zombie's health before the hit,
from a grenade/projectile (location derived from the nearest bone) or a bullet hit on a leg *that killed* (so never a
crawler), picks a gib; a leg gib on a living zombie makes it a crawler (`crawl1..3` anims, `allowedStances("crouch")`).
In practice: explosives that don't kill. Pistols and melee never gib. P spawner `:757-983`. VERIFIED. **Not implemented**
(needs crawler movement/anims in `zombies.rs`).

## 4. Zombie attacks and the player

| value | Nacht | source | status |
|---|---|---|---|
| player health | 100 (no perks in Nacht) | code default; Verrückt's Juggernog sets 160 (`nazi_zombie_asylum` `_zombiemode_perks.gsc:348`) | VERIFIED (by contrast) |
| damage per zombie hit | **60** = zombie AI weapon `kar98k` `iMeleeDamage` 150 × `player_meleeDamageMultiplier` 0.4 | aitype `axis_zombie_ger_ber_sshonor.gsc` (`self.weapon = "kar98k"`), A (`kar98k` iMeleeDamage 150), C `_gameskill.gsc:428` (`100 / 250`, dvar text "Melee damage to the player is scaled by this amount") | ASSUMED: that the engine's AI melee uses the AI weapon's `iMeleeDamage` (no other AI melee damage dvar exists in the exe's dvar list; result matches the well-known "two hits down") |
| down = game over (solo) | `player_damage_override`: if `damage >= health` and no other player alive → last stand + `end_game` | P `:1652-1688` | VERIFIED |
| hits per swing | each melee anim has 2–5 `fire` notetracks; each one calls `melee()` (a hit if still in reach) | C `melee.gsc:124-147`, A | VERIFIED |
| melee anims | random of `attack_forward_v1` (fire 0.97/1.40 s, 2.33 s long), `attack_forward_v2` (0.87/1.40, 3.33), `attack_v1` (0.83/1.30, 1.80), `attack_v2` (0.70/2.33/3.83/4.47/5.33, 7.17); the loop repeats while in reach | C `melee.gsc:627-667`, P `:362-366`, A | VERIFIED |
| reach | `meleeAttackDist` 64, `anim.meleeRange` 64 units (1.63 m) | P spawner `:1283-1284`, C `init.gsc:1562` | VERIFIED (the engine's `ai_meleeRange` default is ASSUMED to agree) |
| attack through windows | **no**: `ignoreall = true` from spawn until every board is down | P spawner `:57`, `:1278` (`zombie_setup_attack_properties` after `tear_into_building`) | VERIFIED; WIKI: Verrückt is the first map where zombies hit through windows |

Health regeneration (`playerHealthRegen`, C `_gameskill.gsc:1201-1430`), with **Regular** difficulty values. The zombies
menu entry runs `devmap nazi_zombie_prototype` without touching `g_gameskill` (UI), so the profile's campaign difficulty
applies; Regular is the default (ASSUMED). Regular uses the "normal" auto-difficulty fraction 0.75 (C `:408`):

| value | Regular | source | status |
|---|---|---|---|
| regen delay after a hit | 2.4 s | C `:284` | VERIFIED |
| then | straight back to full health (`newHealth = 1`) | C `:1341` | VERIFIED |
| "very hurt" | health ≤ 20% (`healthOverlayCutoff`) | C `:160` | VERIFIED |
| very hurt regen | waits `longRegenTime` 5 s after the hit, then +10% of max per 0.05 s (full in 0.5 s) | C `:154`, `:1243`, `:1335` | VERIFIED (single data point → 5 s for all fractions: ASSUMED from the blend code) |
| invulnerable after a hit (≥10% of max) | 0.35 s (not red), 0.5 s (turns red), 0.3 s (already red) | C `:259`, `:267`, `:275`, `:291` | VERIFIED |
| breathing | below 35% health | C `playerBreathingSound(maxHealth * 0.35)` | VERIFIED (already in the game) |

So one zombie double-swipe (60 + 60 within 0.47 s, after the 0.35 s invulnerability) ends a solo game; a single hit heals
completely 2.4 s later.

Hardened/Veteran (for reference): delay 1.2 s, cutoff 0.3/0.5, invulnerability 0.1/0.0 s (C `:139-287`).

## 5. Points

`player_add_points` (N `_zombiemode_score.gsc:9-83`): everything goes through `round_up_to_ten`, then × `zombie_point_scalar`.

| event | points | source | status |
|---|---|---|---|
| start | 500 | P `:277` | VERIFIED |
| non-lethal hit (hip) | `zombie_score_damage` 5 → 10 | P `:279`, N score `:30-32` | VERIFIED |
| non-lethal hit (ADS) | `int(5 × 1.25)` = 6 → 10 | N score `:34-36` | VERIFIED |
| lethal hit | **no hit points**, only the kill: the damage script (`level.global_damage_func`) runs only for hits that leave the zombie alive | C `_gameskill.gsc:2840-2900` | VERIFIED |
| kill, limbs / no location | 50 | P `:278` | VERIFIED |
| kill, torso (`torso_upper/lower`) | 50 + 10 = 60 | P `:283` | VERIFIED |
| kill, neck | 50 + 20 = 70 | P `:282` | VERIFIED |
| kill, head / helmet | 50 + 50 = 100 | P `:281` | VERIFIED |
| kill, melee | 50 + 80 = 130 | P `:280` | VERIFIED |
| kill, fire (`MOD_BURNED`) | 50 + 10 = 60; fire damage gives hit points at most every 0.5 s | P `:284`, `:290`; P spawner `:1219-1228` | VERIFIED |
| kill, explosive splash | 50 (location "none") | as above | VERIFIED |
| Insta-Kill kill | the triggering hit's 10 + Insta-Kill's own kill 50 = **60**, whatever the weapon/location; a hit that would have killed anyway scores normally | N powerups `:452-459` (`DoDamage` without a location); WIKI ("In Nacht der Untoten and Verrückt, the player is only given 60 points per kill, regardless if the knife was used") | VERIFIED |
| board repair | 10 (20 while Double Points runs: the *cost* is doubled, not the scalar, so ×4 never applies) | P blockers `:563-567`, `:612-617`, `:666-674` | VERIFIED |
| repair cap | per player per round: rewarded while the running total `< min(50 × round, 500)` → 40 points in round 1 (4 boards), 490 from round 10 | P `:1180-1183`, P blockers `:704-709`, `:748-751` | VERIFIED |
| repairs and the drop trigger | repairs use `add_to_player_score` (not `score_total`), so they never count towards power-up drops | N score `:128-139` | VERIFIED |
| nuke | **0 in Nacht** (later maps 400) | N powerups `:347-375` (no points call); WIKI agrees | VERIFIED |
| carpenter | not in Nacht (`include_powerups`: nuke, insta_kill, double_points, full_ammo) | P `nazi_zombie_prototype.gsc:306-312` | VERIFIED |
| penalties | downed −5%, died 0%, not reviving −10% (co-op only matters) | P `:286-288` | VERIFIED |

## 6. Power-ups

| value | Nacht | source | status |
|---|---|---|---|
| pool | Nuke, Insta-Kill, Double Points, Max Ammo | P level `:306-312`, N powerups `:30-44` | VERIFIED |
| order | a shuffled cycle: each power-up once per cycle, reshuffled when used up (the last of one cycle can repeat as the first of the next) | N powerups `:46-63` | VERIFIED |
| score trigger | `score_to_drop = players × 500 + 2000`; when the players' total *earned* score (`score_total`, starting 500 included) passes it: `increment ×= 1.14`, `score_to_drop = total + increment`, and the next kill may drop (flag). Polled every 0.5 s | N powerups `:65-90` | VERIFIED |
| per kill | `RandomInt(100) ≤ 2` (3%) drops even without the flag; otherwise only with the flag | N powerups `:143-166` | VERIFIED |
| cap | 4 per round (`zombie_powerup_drop_max_per_round`), reset at round start | N powerups `:18`, `:138-151` | VERIFIED |
| which deaths | every zombie death runs the drop check (nuked and failsafe-killed ones too) | P spawner `:1078-1104` | VERIFIED |
| where | only if the drop point is inside every `playable_area` trigger | N powerups `:168-180` | VERIFIED (not implemented: the game has no playable-area volumes yet; drops are allowed wherever the zombie dies) |
| height | 40 units above the zombie's origin | N powerups `:171` | VERIFIED |
| pick-up | any player within 64 units (3D, feet to model) → about 50 units horizontally on the same floor | N powerups `:238` | VERIFIED |
| on the ground | 15 s, then 40 hide/show steps: 15 × 0.5 s, 10 × 0.25 s, 15 × 0.1 s = 11.5 s → 26.5 s total | N powerups `:310-344` | VERIFIED |
| Insta-Kill | 30 s; another one restarts the 30 s | N powerups `:438-450` | VERIFIED |
| Double Points | 30 s; `zombie_point_scalar *= 2` — another one while it runs makes it **×4** (×8…) and restarts the 30 s, then back to ×1 | N powerups `:405-419`; WIKI agrees (stacks on Nacht, Verrückt, Shi No Numa) | VERIFIED |
| Max Ammo | `GiveMaxAmmo` for primary weapons only (no grenades) | N powerups `:421-436` | VERIFIED |
| Nuke | kills every zombie, closest to the power-up first, one every 0.1–0.7 s, head gib, no points | N powerups `:347-375` | VERIFIED |

## 7. Windows and boards

| value | Nacht | source | status |
|---|---|---|---|
| boards per window | the window's own chunks (`barrier_chunks`, map entities) | P blockers `:490-521` | VERIFIED (read from the map by `Level`) |
| zombies per window | 3 attack spots (centre, ±28 units) | P blockers `:523-544` | VERIFIED (not modelled) |
| tear | one board per tear anim, chosen by board height (> 70 units above the feet: `tear_high`, < 40: `tear_low`, else `left`/`right`); board off at its `board` notetrack | P spawner `:415-587`, A | VERIFIED |
| tear timing | high: board at 1.17 s of 2.60 s; left 1.27/2.40; right 1.27/3.03; low 1.10/2.27 → about one board per 2.3–3.0 s, regardless of gait | A | VERIFIED (height pick approximated by a random pick: ASSUMED) |
| after the last board | finishes the anim, then goes in (`tear_into_building` returns, `find_flesh`) | P spawner `:313-342` | VERIFIED |
| repair | hold use: first board 0.4 s after pressing, then one per second (`wait(1)` after each) | P blockers `:621-718` | VERIFIED |
| repair trigger | radius 96 units around the window's trigger struct | P blockers `:572-598` | VERIFIED (the game keeps its own 1.6 m test) |

## 8. Other balance mechanics

* **Explosive bonus damage** (P spawner `:1171-1192`): a non-lethal grenade hit is followed by
  `DoDamage(round + RandomInt(100, 500))`, a projectile/explosive one by `DoDamage(round × RandomInt(100, 500))`.
  `RandomInt` takes one argument; the grenade code (`grenades.rs`) reads it as `RandomInt(100)` (0–99). ASSUMED.
* **Head pop bleed-out** (P spawner `:589-724`): a head/neck hit with a rifle/SMG/MG bullet, shotgun, close grenade
  (≤ 55 units from `j_head`) or projectile (≤ 10) that leaves ≤ 10% health (and did ≥ 10% damage) pops the head, then
  deals 20% of the health at that moment every second until death (credited to the player). Pistols and the flamethrower
  never pop heads. VERIFIED; not implemented (weapons code).
* **Fire** (P spawner `:1114-1150`): burning zombies take a share of `level.zombie_health` every 2–5 s
  (20–30% before round 6, 10–20% to round 8, then a bugged `RandomFloatRange(0.8, 0.16)` to round 10, 6–14% after).
  VERIFIED; not implemented (flamethrower).
* **Grenades** each round start: refilled to 2/3/4 by the fraction held (P `:1231-1259`) — already in `grenade.rs`.
* **Knife**: the held weapon's `iMeleeDamage` (150 for every Nacht gun) — one-hit kills in round 1. VERIFIED (A).
* **Zombie pathing**: re-targets the closest player every 1–3 s (P spawner `:1292-1345`). Movement code's business.

## 9. Old → new (this change)

| mechanic | before | now |
|---|---|---|
| health r1/r9/r10/r20 | 150/950/1045/2710 (float ×1.1) | 150/950/1045/2701 (script integer maths) |
| zombies solo r1–r5, r10 | 6, 8, 13, 18, 24, 36 | 4, 9, 14, 19, 24, 24 |
| spawn delay | 2.0 × 0.95^(r-1), min 0.1 | 3.0 × 0.95^(r-1), floor 0.076 |
| first spawn | 1 s after round start | 6.75 s (round 1), 0.5 s (later) |
| spawn at AI limit | timer paused | retried every spawn delay |
| initial wait before round 1 | 4 s | immediate |
| gait | own ramp (sprinters from round 6 up to 60%) | `zombie_move_speed` roll (sprinters 11% in r6, all from r10) |
| speeds walk/run/sprint | 1.1 / 3.0 / 4.6 m/s | 1.03 / 1.90 / 3.57 m/s |
| zombie hit | 40, one swing per 1 s from 1.1 m, also through windows | 60 per `fire` notetrack of a random melee anim (2–5 hits per anim), reach 1.63 m, not through windows |
| regen | 60 hp/s after 3.5 s | full after 2.4 s; at ≤20% wait 5 s then 0.5 s; 0.3–0.5 s invulnerability |
| tear time | 1.3/1.0/0.8 s per board by gait | tear anim: board at 1.1–1.27 s, 2.3–3.0 s per board |
| repair | 0.7 s per board, 10 (×2 DP), no cap | 0.4 s then 1 s per board, 10/20, cap min(50 × round, 500) per round |
| kill points | 50/100/130 | + torso 60, neck 70 (`KillKind::Torso/Neck`, `body_location`) |
| Insta-Kill points | normal kill points | `insta_kill_points`: 60 (helper; weapons code to call it) |
| Double Points | ×2, re-grab resets timer | ×2, re-grab ×4 (stacks), repairs/nuke unaffected |
| nuke | 400 points, nuked zombies never drop | 0 points, nuked zombies roll for drops |
| drops | 2.5% per kill, random kind, max 4 | score trigger (2000 × 1.14^n) or 3%, shuffled cycle, max 4 |
| power-up on ground | 26 s, blinking last 6 s | 15 s + 11.5 s stepped blinking |
| pick-up | 1.3 m | 64 units 3D (≈1.27 m flat) |

## 10. Still open

* Crawlers, head-pop bleed-out, fire damage, failsafe kill, playable-area check for drops — rules documented above, need
  code outside `rules.rs` (zombies/weapons/level).
* `insta_kill_points`, `KillKind::Torso/Neck` and `body_location` exist in `rules.rs`, but `weapons.rs` still awards
  `Body`/`Head` kills (owned by the weapons work).
* Difficulty: Regular assumed. A setting could switch `regen_delay`/`very_hurt_ratio`/invulnerability to Hardened/Veteran.
* AI melee damage (60) and `ai_meleeRange` are engine-side; confirmed only by outcome (two hits down).
