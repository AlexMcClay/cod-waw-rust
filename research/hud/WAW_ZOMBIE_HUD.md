# World at War zombies HUD (Nacht der Untoten, solo)

This file covers the HUD that WaW zombies draws, as found in the user's install and in the map's own scripts.
It has two parts: what each element is, and how to rebuild it (positions, images, fonts, colours, timings).

* **VERIFIED** means one of three things: it was read directly from the install (zone walk, IWD, IWI decode or `.cfg`), it was read from the
  map's GSC (decompiled scripts in `extracted/fastfiles/...` or rawfiles dumped from `common.ff`), or it was
  checked by rendering.
* **ASSUMED** means it is inferred: from IW-engine (CoD4) knowledge, from names, or from a plausible layout. It
  was not checked against the T4 binary. No code from `CoDWaW.exe` was decompiled. Only its plain strings were
  searched (dvar names and material names).

Everything game-derived that the scripts produce (dumps, decoded PNGs, the mock-up) goes to `research/hud/local/`.
That folder is git-ignored.

## 0. Scripts in this folder

| Script | What it does |
|---|---|
| `hudlib.py` | Shared helpers: the IWD index (`images/<name>.iwi` mapped to its pak), a pure-Python IWI v6 decoder (ARGB32/RGB24/LA16/L8/A8/DXT1/3/5) and `walk_zone()`. |
| `survey_zones.py [zones]` | Lists the fonts, menulists, menus, localize entries, rawfiles and materials (with their images) in each zone. Writes `local/survey_<zone>.txt`. |
| `dump_menus.py <zone> [menu...]` | Prints each `menuDef_t`/`itemDef_s` as readable text: rects, alignment, ownerdraw ids, colours, fonts, textscale, background material and image, and decoded `visible when` expressions. Writes `local/menus_<zone>.txt`. |
| `dump_strings.py loc <zone>` / `raw <zone> <file>` | Dumps the localized strings, or individual rawfiles such as `maps/_hud_util.gsc`. Writes `local/localize_<zone>.txt` and `local/raw/...`. |
| `decode_font.py` | Reads every `Font_s` (glyph tables to `local/fonts/<font>.json`), decodes the atlas image, and renders a test string per font (`local/fonts/sample_<font>.png`). |
| `dump_weapon_hud.py` | Lists the HUD-related `WeaponDef` fields (display name, hudIcon, clip type, reticles, low-ammo threshold). Writes `local/weapon_hud_<zone>.txt`. |
| `dump_hud_images.py` | Checks that every HUD image exists in the IWDs, decodes previews, and writes `local/images/summary.txt`. |
| `render_hud_mock.py` | Builds a 640x480 mock-up (at 2x scale) of the solo HUD from the real assets and the positions below. Writes `local/hud_mock.png`. |

Run them with `py <script>` from this folder. They import `../t4/zonewalk.py`.

## 1. Who draws what

| Element | Drawn by | Source |
|---|---|---|
| Round chalk marks (rounds 1-10) | **script hudelems** (`SetShader`) | `_zombiemode.gsc` `create_chalk_hud`, `chalk_one_up`, `chalk_round_hint` |
| Round number (rounds 11 and up) | **script hudelem** (`SetValue`, fontscale 32) | same |
| "Round" intro text | **script hudelem** | `chalk_one_up` (intro branch) |
| Score with blood-splatter bar | **engine ownerdraw 288** in menu `competitivemodescores` | `common.ff` menulist `ui/hud.txt` |
| Point popups (+10 / -1500) | **script hudelems** | `_zombiemode_score.gsc` `score_highlight` / `create_highlight_hud` |
| Weapon name | **engine ownerdraw 81** (`weapname_lowdef`) in menu `weaponinfo` | `common.ff` `ui/hud.txt` |
| Clip graphic (bullet icons) | **engine ownerdraw 117** (`clipGraphic`) | `weaponinfo` |
| Reserve ammo number | **engine ownerdraw 119** (`ammostock`) | `weaponinfo` |
| Grenade icon and count | **engine ownerdraw 103 / 105** (`offhandFragIcon`, `offhandfragammo`) | `weaponinfo` |
| "Reload" / "LOW AMMO" | **engine ownerdraw 120** (`lowammowarning`) | `weaponinfo` |
| Use-trigger hint ("Press & hold F to buy ...") | **engine ownerdraw 72** (`cursorhints` menu). The text comes from script `SetHintString`. | `common.ff` `ui/hud.txt`, `_zombiemode_weapons.gsc`, `_zombiemode_blockers.gsc` |
| Power-up text ("Double Points: 30" and similar) | **script hudelems** (`createFontString("objective", 2)`) | `_zombiemode_powerups.gsc` |
| Nuke white flash | **script hudelem** (`white`, fullscreen) | `_zombiemode_powerups.gsc` `nuke_flash` |
| Low-health red overlay | **script hudelem** (`overlay_low_health`, fullscreen) | `maps/_gameskill.gsc` (common.ff), started by `_load.gsc` |
| Crosshair | **engine** (weapon `reticleSide`/`reticleCenter`) | `WeaponDef` |
| Game over text | **script hudelems** | `_zombiemode.gsc` `end_game` |
| Compass, stance | hidden: `compass 0` and `hud_showStance 0` are set by `onPlayerConnect_clientDvars` | `_zombiemode.gsc` |

All of the above is VERIFIED from scripts and menus, except the ownerdraw ids. The ids are numbers in the
menu data (VERIFIED), but their meanings are ASSUMED. They follow CoD4's `menudefinition.h` numbering and agree
with the item names in the menus (`clipGraphic`=117, `ammostock`=119, `weapname`=81 and so on).

**Menu location (VERIFIED).** Nacht has no `_patch` zone, so it uses the HUD menulist `ui/hud.txt` in
`zone/english/common.ff`. That menulist holds 33 menus: `friendly_arrows` ... `overheadmap`. Factory, Sumpf and
Asylum each ship a copy in `nazi_zombie_*_patch.ff`. Its `weaponinfo` and `competitivemodescores` are
byte-for-byte the same after decoding (`diff` of the dumps). The `ui/hud.txt` "files" in
`extracted/.../_patch/raw/ui/` are not real rawfiles: they are 32 bytes of `0xFF`, a mis-extraction of the
menulist asset. `ui.ff` holds only main-menu menus. `code_post_gfx.ff` holds `ui/code.txt` and the fonts.

### 1.1 The 640x480 virtual screen (ASSUMED IW behaviour)

The menus and hudelems lay out a 640x480 virtual screen.

* `horzAlign`/`vertAlign` choose the reference edge:
  * `left`/`top` mean x and y are measured from the left or top edge.
  * `right`/`bottom` mean they are measured from the right or bottom edge, so x and y are negative.
  * `center`/`middle` mean they are measured from 320 or 240.
  * `fullscreen` stretches the 640x480 rect to the whole screen.
* For script hudelems, `alignX`/`alignY` set which point of the element sits at that position (`left|center|right`,
  `top|middle|bottom`).
* Menu rects (`rectDef_s`) have the same `horzAlign`/`vertAlign` meaning, coded as numbers:
  * 1 = LEFT/TOP
  * 2 = CENTER
  * 3 = RIGHT/BOTTOM
  * 4 = FULLSCREEN
  * 0 = SUBLEFT/SUBTOP, the default, which means the coordinates are absolute in 640x480.
* An `itemDef`'s `window.rect` already contains the absolute position, so the menu offset has already been added
  (VERIFIED: `rect` vs `rectClient`).
* On widescreen, scale by `screenH/480`. Pin left- and right-aligned elements to the screen edges and keep
  centre-aligned ones centred.

## 2. Each element in detail

The colour `DARK_RED` below is `(0.423, 0.004, 0)`. Every zombie script uses it; the comments show it replaced
`(0.8, 0, 0)`. **VERIFIED.**

### 2.1 Round counter (chalk)

All VERIFIED from `_zombiemode.gsc`, except where marked otherwise.

* **Two hudelems are created at game start** by `round_start()`. Both come from `create_simple_hud()`
  (`foreground=1`, `sort=1`, `hidewheninmenu=0`), and both have `alignX left`, `alignY bottom`, `horzAlign left`,
  `vertAlign bottom`, `color DARK_RED`, `alpha 0` and `SetShader("hud_chalk_1", 64, 64)`.
  * `chalk_hud1` is at x=0, y=0.
  * `chalk_hud2` is at x=64, y=0.
* **Images.** The materials `hud_chalk_1..5` map to the images `chalkmarks_1..5`. These are white 128x128 tally
  marks (1-4 strokes; 5 is four strokes plus a diagonal) and are drawn at 64x64. The images are in `iw_01.iwd`
  (VERIFIED). The colour comes from the hudelem `color`, which multiplies the white image.
* **Rounds 1-5.** `chalk_hud1` shows `hud_chalk_<round>`.
* **Rounds 6-10.** `chalk_hud1` keeps showing `hud_chalk_5`. `chalk_hud2` (x=64) shows `hud_chalk_<round-5>`.
* **Round 11 and up.**
  * `chalk_hud2` fades out (0.5 s) and is destroyed.
  * `chalk_hud1.fontscale = 32` (this is set on every `chalk_one_up` for rounds <6 or >10).
  * `chalk_hud1 SetValue(round)` makes it a number in `DARK_RED`.
  * There is no `.font`, so it uses the hudelem font `default` (ASSUMED to be `fonts/normalFont`).
  * Position: `left`/`bottom`, x=-5, y=0. See the intro below for why x is -5.
* **Changing round** (`chalk_one_up`, each round start, not the intro):
  1. Fade the element out over 0.5 s.
  2. Wait 0.5 s.
  3. Play sound `chalk_one_up`.
  4. For round 11 and up, call `SetValue(round)`.
  5. Fade in over 0.5 s.
  6. Immediately call `SetShader` with the new chalk (rounds 2-10), so the new tally is what fades in.
* **Intro** (round 1, `level.first_round`):

  | t (s) | What happens |
  |---|---|
  | 0 | "Round" hudelem (`&ZOMBIE_ROUND` = "Round"; `center`/`bottom` aligned, `horzAlign center`, `vertAlign bottom`, x=0, **y=-265**, `fontscale 16`, white) fades in over 1 s. |
  | 1 | The "Round" colour fades white to `DARK_RED` over 3 s. The chalk (`hud_chalk_1`) is moved to `horzAlign center`, **x=-5, y=-200**, still `alignX left` / `alignY bottom`. Its top-left is therefore (315, 216) and it is 64x64. |
  | 1.5 | Sound `chalk_one_up`; the chalk fades in over 0.5 s. |
  | 4.5 | "Round" fades out over 1 s. |
  | 4.75 | Notify `intro_hud_done`. The chalk does `MoveOverTime(1.75)` to `horzAlign left`, y=0. `x` stays at **-5**, because `hud.x = 0` is commented out in the script. |
  | 6.75 | "Round" is destroyed. |

  `MoveOverTime` is ASSUMED to interpolate between the two alignment frames, as IW hudelems do. Afterwards the
  chalk sits at x=-5..59, y=416..480.

  `round_text()` exists but its calls are commented out (`&ZOMBIE_ROUND_BEGIN` "Let The Onslaught Begin",
  `&ZOMBIE_ROUND_END`). It would draw at center/middle, y=-100, fontscale 16, white fading to red.
* **End of round** (`chalk_round_hint`). Let `time = zombie_between_round_time = 10`.
  1. The colour fades to white (1,1,1) over 2.5 s (`time*0.25`).
  2. Sound `end_of_round`.
  3. The marks pulse 10 times: alpha 1 to 0 over 0.5 s, then 0 to 1 over 0.5 s.
  4. The colour fades back to `DARK_RED` (alpha 1) over 2.5 s.

  The whole sequence takes 15 s, but the next round starts after 10 s. The next `chalk_one_up` therefore overlaps
  the pulsing tail; both threads write alpha.

  Both chalk elements pulse during rounds 6-10. Otherwise only `chalk_hud1` pulses.

### 2.2 Score (engine)

* **Menu** `competitivemodescores` (VERIFIED):
  * menu rect `-103 -71 0 0`, align RIGHT / BOTTOM;
  * one item, `playerscores`: `ownerdraw 288`, rect **-103 -71 100 0**, RIGHT / BOTTOM, forecolor (1,1,1,1),
    textscale 0.55, fontEnum 0;
  * visible when `!uiactive && ui_hud_hardcore==0 && miniscoreboardhide==0 && hud_missionFailed==0 &&
    (arcademode==1 || zombiemode==1)`.
* `_zombiemode_score.gsc` confirms the position with `score_x = -103; score_y = -71; // Location from hud.menu`.
  In co-op, rows are 18 units apart: `y = num*-18 + score_y`.
* **Background images** (the engine names them; VERIFIED in exe strings and as materials in
  `nazi_zombie_prototype.ff`):
  * `scorebar_zom_1..4` map to images `scorebar_zom_1..4`: a white brush stroke, DXT5 512x64.
  * `scorebar_zom_long_1..4`: DXT5 1024x64.
  * All are in `iw_07.iwd`.
  * The index is the player number. The `long` variant is ASSUMED to be for co-op rows that also show a name.
* **Colours** (VERIFIED from `iw_00.iwd/coop_arcademode.cfg`, "ZOMBIE MODE DVARS"):
  * `cg_ScoresColor_Zombie 0.424 0.004 0` and `cg_ScoresColor_TransparencyZombie 0.8`. These are ASSUMED to be
    the bar tint and alpha, giving the red blood splatter.
  * `cg_ScoresColor_Gamertag_0 1 1 1` (player 1 white), `_1 0.486 0.812 0.933`, `_2 0.965 0.792 0.314`,
    `_3 0.514 0.925 0.533`. These are ASSUMED to be the score text colour per player.
* **Font, size and the bar's exact rect: ASSUMED.**
  * The image is 8:1 and rows are 18 units, so draw the bar about 112x20 to the right of x=-103 (x -107..+5,
    centred on y=-71).
  * Draw the digits left-aligned starting near x=-95, about 15 units high, white, in the `normalFont` family.
  * The score changes instantly; there is no count-up.
  * `local/hud_mock.png` shows this layout.

### 2.3 Point popups (script; VERIFIED from `_zombiemode_score.gsc`)

* **Created on every score change** by `set_player_score_hud`, with `value = score - old_score`. This includes the
  initial call at spawn, where the value is 0 and the popup is red.
* **The hudelem** is `NewHudElem()`:
  * `foreground 1`, `sort 0`, `fontScale 8`, no `.font` (`default`);
  * `alignX right`, `alignY middle`, `horzAlign right`, `vertAlign bottom`;
  * starts at **x=-103, y=-71** (solo; in co-op y is `-71 - 18*row`).
* **value >= 1:** `color (0.9, 0.9, 0.0)` (yellow), `label = &"SCRIPT_PLUS"` ("+"), `SetValue(value)`. The text
  reads "+10", "+60" and so on.
* **value < 1:** `color DARK_RED`, no label. The text reads "-1500" or "0".
* **Motion:** `MoveOverTime(0.5)` with linear motion to
  * `x -= 20 + RandomInt(40)`, so it moves 20..59 units left;
  * `y -= (-15 + RandomInt(30))`, so the y change is between -14 and +15.
* **Fade:** after 0.25 s, `FadeOverTime(0.25)` takes alpha to 0. At 0.5 s the popup is destroyed.
* **Point values** (`_zombiemode.gsc`; all values are rounded up to 10 and multiplied by `zombie_point_scalar`,
  which is 2 during Double Points):
  * damage 5, which becomes 10;
  * ADS damage 5*1.25, which becomes 10;
  * kill 50, plus a bonus of head 50, neck 20, torso 10 or melee 80 (burn 10);
  * start score 500.

### 2.4 Weapon name, clip graphic, reserve ammo, grenades, low ammo (engine, menu `weaponinfo`)

**Menu-level fields** (VERIFIED):
* menu rect `0 0 0 0` RIGHT / BOTTOM;
* visible when `!gamemsgwndactive(2) && chaplinCheat==0 && ammoCounterHide==0 && hud_missionFailed==0 &&
  !flashbanged && !uiactive && ui_hud_hardcore==0`;
* the zombie script sets `ammoCounterHide 0`.

**Items:**

| item | ownerdraw | rect x y w h (align) | forecolor | font / textscale / style | other |
|---|---|---|---|---|---|
| `clipGraphic` | 117 | -79 -4 1 1 (R/B) | 1 1 1 0.65 | - | bullet icons for the rounds in the clip |
| `offhandFragIcon` | 103 | -104 -38 24 24 (R/B) | 1 1 1 0.65 | - | background `hud_us_grenade` |
| `offhandSmokeIcon` | 104 | -134 -38 24 24 (R/B) | 1 1 1 0.65 | - | `hud_us_smokegrenade` (not used in Nacht) |
| `offhandsmokeammo` | 106 | -114 -8 25 25 (R/B) | 1 1 1 0.75 | fontEnum 6 (objective), 0.3095, style 3 | |
| `offhandfragammo` | 105 | -84 -8 25 25 (R/B) | 1 1 1 0.75 | fontEnum 6, 0.3095, style 3 | grenade count, e.g. "3" |
| `ammostock` | 119 | -75 4 25 25 (R/B) | 1 1 1 0.75 | fontEnum 6, 0.3095, style 3 | reserve ammo, e.g. "128" |
| `weapname_lowdef` | 81 | -305 -40 290 40 (R/B) | 1 1 1 0.75 | fontEnum 6, 0.3095, style 3 | display name, e.g. "Thompson" |
| `lowammowarning` | 120 | -10 15 100 30 (CENTER/CENTER) | 1 1 1 1 | fontEnum 0 (auto), 0.3095, style 3, textAlignMode 9 | visible only when `g_gameskill` is 0 or 1, or the map is `training` |

`fontEnum` values are ASSUMED to follow CoD4: 0 auto, 1 normal, 2 big, 3 small, 4 bold, 5 console, 6 objective.

**Text size (ASSUMED IW formula).** A menu textscale `s` gives a pixelHeight box of `48*s` virtual units, so 0.3095
gives 14.86. The glyph scale is therefore `48*s / font.pixelHeight`. For objectiveFont that is 0.531, so capitals
are about 9 units high and digits about 9.6. With `fontEnum 0` the engine chooses the font by scale:
* if `s <= ui_smallFont` it uses smallFont;
* otherwise if `s >= ui_extraBigFont` it uses extraBigFont;
* otherwise if `s >= ui_bigFont` it uses bigFont;
* otherwise it uses normalFont.

The PC values are `ui_smallFont 0.375`, `ui_bigFont 0.5832`, `ui_extraBigFont 1.0` (VERIFIED in
`iw_00.iwd/default_480p.cfg` and `dvar_defaults.cfg`). So 0.3095 with fontEnum 0 selects **smallFont**.

**Text origin (ASSUMED, consistent with the glyph data).** An ownerdraw's text `y` is the bottom of the font's
pixelHeight box. Digits end about 0.21*height above it.
* `ammostock` at y=+4 puts the digit bottoms right at the screen edge.
* `offhandfragammo` at y=-8 puts "3" just under the right corner of the grenade icon.
* `weapname` at y=-40 is right-aligned to the rect's right edge (x=-15). That puts it under the score bar and
  above the grenade icon, which matches the reference screenshot.

The layout is in `local/hud_mock.png`.

**Clip graphic (ASSUMED).**
* The engine picks the icon from `WeaponDef.ammoCounterClip` because `ammoCounterIcon` is null for every Nacht
  weapon (VERIFIED). The mapping by name is:
  * 1 MAGAZINE: `ammo_counter_bullet`, 4x8;
  * 2 SHORTMAGAZINE: `ammo_counter_riflebullet`, 32x8;
  * 3 SHOTGUN: `ammo_counter_shotgunshell`, 16x8;
  * 4 ROCKET: `ammo_counter_rocket`, 64x16;
  * 5 BELTFED: `ammo_counter_beltbullet`, 8x4.
* All of these are in `iw_00.iwd`, and the material names are in the exe (VERIFIED).
* Draw one icon per round in the clip, growing **left** from x=-79 with the bottom at y=-4, at alpha 0.65.
* Examples: Thompson clip 1, 20 rounds; Kar98k 2; shotgun 3; zombie_colt 1 (VERIFIED from `WeaponDef`).

**Weapon name** (VERIFIED). It is the localized `szDisplayName`: `WEAPON_THOMPSON` = "Thompson",
`WEAPON_COLT45` = "Colt M1911", `WEAPON_KAR98K` = "Kar98k", `WEAPON_RAY_GUN` = "Ray Gun",
`WEAPON_GERMANGRENADE` = "Stielhandgranate". These come from the `localize` assets in `common.ff` and
`nazi_zombie_prototype.ff`. Whether the name fades a few seconds after a weapon switch is ASSUMED: the
`hud_fade_ammodisplay` / `hud_fadeout_speed` dvars exist, but their defaults are in code.

**Grenade** (VERIFIED). `stielhandgranate` has `hudIcon = hud_us_grenade`. The image is `hud_us_grenade`, DXT3
64x64, in `iw_04.iwd`; it shows the pineapple silhouette, not a stick grenade. The menu draws it at 24x24, white,
alpha 0.65. The player gets the grenade at round start with clip 0 (`round_start`). Survivors are refilled by
`award_grenades_for_survivors`. The count is the offhand ammo.

**Low ammo** (VERIFIED). The strings are `PLATFORM_RELOAD` = "Reload" (clip low, reserve left) and
`PLATFORM_LOW_AMMO_NO_RELOAD` = "LOW AMMO". The weapon's `lowAmmoWarningThreshold` is 0.33 (VERIFIED), ASSUMED to
mean clip/clipSize <= 0.33.

The colours pulse between two dvars: `lowAmmoWarningColor1/2`, `...NoReloadColor1/2`, `...NoAmmoColor1/2`, with
`lowAmmoWarningPulseFreq/Min/Max`. The dvar names are VERIFIED in the exe; their defaults are in code and were not
read. The ASSUMED CoD4 defaults are:
* Color1 (0.70, 0.70, 0.70, 0.8), Color2 (1, 1, 1, 1);
* NoReload (0.70, 0.70, 0.30, 0.8) to (1, 1, 0.5, 1);
* NoAmmo (0.8, 0.25, 0.25, 0.8) to (1, 0.25, 0.25, 1);
* freq 1.7, min 0, max 1.5.

Position: centred horizontally on the screen centre, about 15..45 units below it (textAlignMode 9 =
MIDDLE_CENTER). The rect's `-10` x offset is ASSUMED to be ignored or negligible. The reference screenshot shows
"Reload" centred.

### 2.5 Hint strings (use triggers)

**Drawing** (VERIFIED):
* Menu `cursorhints` holds item `chRect`, ownerdraw 72, rect **0 70 40 40 CENTER/CENTER**, textscale 0.3095,
  fontEnum 0 (so smallFont, see 2.4), textStyle 3 (shadowed), white.
* It is visible when `chaplinCheat==0 && mapname != "credits"`.
* Text is centred horizontally on x=320 at about y=310..325 (ASSUMED: centred on the rect's x, below the icon
  slot).
* Every zombie trigger uses `SetCursorHint("HINT_NOICON")`, so no icon is drawn (VERIFIED).

**Strings** (VERIFIED, from the `localize` assets in `nazi_zombie_prototype.ff`):
* Wall weapons: `ZOMBIE_WEAPON_<NAME>_<COST>`, e.g. `'Press & hold &&1 to buy Thompson [Cost: 1500]'`,
  `'... Kar98k [Cost: 200]'`, `'... M1897 Trench Gun [Cost: 1500]'`, `'... Double-Barreled Shotgun [Cost: 1200]'`.
  There are 66 weapon strings; see `local/localize_nazi_zombie_prototype.txt`.
* After the first purchase the hint changes to `&"ZOMBIE_WEAPONCOSTAMMO", cost, ammo_cost`, which reads
  `'For Weapon [Cost: &&1], For Ammo [Cost: &&2]'`. Here `&&1` and `&&2` are the script parameters (numbers).
* Barriers: `ZOMBIE_BUTTON_REWARD_BARRIER_<n>` reads `'Press & hold &&1 to Rebuild Barrier [Reward: 10..50]'`.
  The plain `ZOMBIE_BUTTON_REWARD_BARRIER` has no reward text.
* Doors and debris: `ZOMBIE_BUTTON_BUY_OPEN_DOOR_<cost>`, `..._CLEAR_DEBRIS_<cost>`, `..._OPEN_AREA_<cost>`, e.g.
  `'Press & hold &&1 to Open Door [Cost: 1000]'`.
* Others: `ZOMBIE_TRADE_WEAPONS` `'Press &&1 to trade weapons'`, `ZOMBIE_RANDOM_WEAPON_950`
  `'Press &&1 for a Random Weapon [Cost: 950]'`, `ZOMBIE_UNDEFINED`.

The cost is part of the localized string, not formatted by the engine. It is a plain integer with no thousands
separator ("[Cost: 10000]").

`&&1` with no script parameter is replaced by the engine with the key bound to `+activate`, e.g. "F" (ASSUMED).
The key names are the `KEY_*` localize entries in `code_post_gfx.ff`. Zombie strings have no `^3` colour codes,
so the whole string is white. Campaign `PLATFORM_*` strings do use `^3 ... ^7`.

### 2.6 Power-ups (script; VERIFIED from `_zombiemode_powerups.gsc`)

Nacht shows **no power-up icons**. The icon row comes from later games. Power-ups show text only.

| Power-up | Hudelem | Text | Position | Lifetime |
|---|---|---|---|---|
| Double Points | `createFontString("objective", 2)` (font `objective`, i.e. objectiveFont; fontScale 2; sort 0.5) | label `ZOMBIE_POWERUP_DOUBLE_POINTS` "Double Points: " + `SetValue(seconds)`, so "Double Points: 30" counting down once per second | `setPoint("TOP", undefined, 0, 350)`: alignX center, alignY top, horzAlign center, vertAlign top, x 0, **y 350** | Fades in over 0.5 s. Destroyed when the timer reaches -1 (30 s, reset to 30 on re-pickup). |
| Insta-Kill | same | "Insta-Kill: " + seconds | **y 380** (350+30) | same, 30 s |
| Max Ammo | same | label only: "Max Ammo!" | **y 290** (350-60) | Fades in over 0.5 s. After 0.5 s it moves to y=270 and fades to 0 over 1.5 s, then is destroyed. |
| Nuke | white `newhudelem`, `fullscreen`/`fullscreen`, `SetShader("white", 640, 480)`, foreground | none (`ZOMBIE_POWERUP_NUKE` is precached but not shown) | fullscreen | Alpha 0 to 0.8 over 0.2 s, hold until t=0.5, then to 0 over 1.0 s. Destroyed at 1.6 s. |

The colour is the default white (no `.color` is set).

### 2.7 Damage / low-health overlay (script, `common.ff` `maps/_gameskill.gsc`; VERIFIED)

* `healthOverlay()` creates a client hudelem with `SetShader("overlay_low_health", 640, 480)`, aligned left/top,
  `fullscreen`/`fullscreen`, alpha 0. The image is `overlay_low_health`, DXT5 512x512 in `iw_05.iwd`: a dark-red
  vignette, transparent in the middle.
* The overlay flashes while `player_has_red_flashing_overlay` is set. That flag is set when `health/maxHealth <=
  level.healthOverlayCutoff`. The cutoff depends on difficulty: 0.01 easy, 0.2 normal, 0.25, 0.3 hardened,
  0.5 veteran.
* `redFlashingOverlay` repeats `fadeFunc(severity, mult)` with pulseTime 0.8:
  * fadeIn = 0.08 s to alpha `mult`;
  * stay = 0.8*(0.1 + 0.2*sev);
  * fade to `mult*(0.8 + 0.1*sev)` over 0.8*(0.1 + 0.1*sev);
  * fade to `mult*(0.5 + 0.3*sev)` over 0.24 s;
  * then the remaining time.
* The sequence is: `(1,1)` once, then `(.9,1)` until `longRegenTime` (5000 ms) has passed, then `(.65,.8)`, then
  `(0,.6)`, then a 0.5 s fade to 0. Being damaged again restarts it.
* The "You are Hurt. Get to Cover!" warning appears only on easy and normal, early in the campaign and not in
  co-op, so in practice it is not seen in zombies.
* `damage_feedback` (the hit-marker material) exists in `common.ff`, but no zombie or SP script uses it. **There is
  no hit marker in WaW zombies** (VERIFIED by grep).

### 2.8 Crosshair (engine; data VERIFIED, behaviour ASSUMED)

* Every Nacht gun has `reticleSide = reticle_side_small`: image `side_small`, DXT5 8x8 in `iw_07.iwd`, a short
  vertical bar.
* The gun settings are `iReticleSideSize 8`, `iReticleCenterSize 4`, `iReticleMinOfs 0`, and no center reticle.
* Grenades use `reticle_center_cross`: image `center_cross`, 32x32.
* The engine draws four side ticks around the centre, offset by the current spread, and fades them while firing
  or aiming down sights (`cg_crosshairAlpha`, `cg_crosshairAlphaMin`, `cg_crosshairDynamic`, `adsCrosshairInFrac`).
  That explains why no crosshair is visible in the reference shot while firing from the hip.
* `crosshairColorChange 1`, so the crosshair turns red over enemies (`cg_crosshairEnemyColor`).

### 2.9 Game over (script; VERIFIED)

* `GAME OVER` (`ZOMBIE_GAME_OVER`): center/middle, y=-10, fontScale 3, white, fades in over 1 s.
* `You Survived &&1 Rounds` (or `ZOMBIE_SURVIVED_ROUND` "You Survived 1 Round"): y=+20, fontScale 2, fades in
  over 1 s.
* The intermission background is `SetShader("black", 640, 480)` with `fullscreen` alignment.

### 2.10 The hudelem `fontScale` puzzle (UNRESOLVED; read this before sizing script text)

* `maps/_hud.gsc` sets `level.fontHeight = 12`, and `createFontString` uses `height = int(level.fontHeight *
  fontScale)` (VERIFIED). So for normal values (1.2-5, which the campaign scripts use everywhere) a hudelem is
  **12 virtual units per fontScale unit**. This matches CoD4 and fits `GAME OVER` (3, 36 units) and `survived` (2,
  24 units) exactly: their centres are 30 apart.
* The zombie-only values cannot follow that rule: popups 8, "Round" 16, chalk number 32. A popup would be 96 units
  high, and the round number 384, more than the screen.
* The exe only says `font scale was %g; should be > 0` (VERIFIED), so any positive value is accepted.
* **Recommendation (ASSUMED):** render zombie hudelems at **about 2 units per fontScale unit**:
  * popups 16 units high, which fits the 18-unit co-op score rows;
  * "Round" 32 units;
  * round numbers 64 units, the same as the 64x64 chalk images they replace.
* Keep 12 per unit for fontScale <= 5: game over, power-up text (`objective` 2 gives 24 units), the
  player-zombie "Zombie down!" text (1.8).
* Calibrate against the reference screenshot if the numbers look wrong.

## 3. Fonts

### 3.1 Assets (VERIFIED: `zone/english/code_post_gfx.ff`, asset type 20 `Font_s`)

| Font_s | pixelHeight | glyphs | material | atlas image |
|---|---|---|---|---|
| `fonts/smallDevFont` | 16 | 96 | `fonts/devfonts` | `devfonts` 256x256 DXT5 (`iw_02.iwd`) |
| `fonts/bigDevFont` | 24 | 96 | `fonts/devfonts` | same |
| `fonts/consoleFont` | 16 | 254 | `fonts/gamefonts_pc` | `gamefonts_pc` 512x512 DXT5 (`localized_english_iw00.iwd`) |
| `fonts/smallFont` | 12 | 254 | `fonts/gamefonts_pc` | same |
| `fonts/normalFont` | 30 | 191 | `fonts/gamefonts_pc` | same |
| `fonts/boldFont` | 30 | 191 | `fonts/gamefonts_pc` | same |
| `fonts/bigFont` | 32 | 191 | `fonts/gamefonts_pc` | same |
| `fonts/extraBigFont` | 32 | 191 | `fonts/gamefonts_pc` | same |
| `fonts/objectiveFont` | 28 | 191 | `fonts/gamefonts_pc` | same |

Each font also has a `glowMaterial` (`fonts/gamefonts_pc_glow`, `fonts/devfonts_glow`) that uses the same image.

* **One shared atlas.** All game fonts share the single 512x512 atlas. Its GfxImage in the zone has
  `resourceSize 0` (no inline pixels), so the pixels come from **`images/gamefonts_pc.iwi` in
  `localized_english_iw00.iwd`**. The atlas is localized, so other languages ship their own.
* **Alpha only.** RGB is 255 everywhere; the glyph shapes are in **alpha only** (VERIFIED, decoded to
  `local/fonts/gamefonts_pc.png`; `gamefonts_pc_on_black.png` is a preview).
* **Typeface.** It is a humanist sans (the WaW UI face) at several sizes, not a serif. The "serif" impression in
  the reference shot is this face.
* **Same face, different sizes.** `normalFont`, `boldFont`, `bigFont` and `extraBigFont` are the same face at
  different raster sizes; bold is a separate raster. The console font is a monospace face.
* **No chalk font.** Round numbers are this UI font tinted dark red.

### 3.2 Layout (from `research/t4/T4_LAYOUTS.txt`; verified by decoding)

```
struct Font_s  (24 bytes)          struct Glyph (24 bytes)
 0x00 char*      fontName            0x00 u16  letter        (code point, Latin-1 here: 32..255)
 0x04 int        pixelHeight         0x02 i8   x0            (left bearing, pixels)
 0x08 int        glyphCount          0x03 i8   y0            (top of glyph relative to the text origin; negative = up)
 0x0C Material*  material            0x04 u8   dx            (advance, pixels)
 0x10 Material*  glowMaterial        0x05 u8   pixelWidth
 0x14 Glyph*     glyphs              0x06 u8   pixelHeight
                                     0x07 pad
                                     0x08 f32  s0, 0x0C f32 t0, 0x10 f32 s1, 0x14 f32 t1   (atlas UVs)
```

* **Load order in the zone** (`T4_LOAD_SPEC.txt`, `ASSET Font_s`; the Python walker follows it):
  1. the 24-byte header;
  2. push VIRTUAL, then fontName (xstring);
  3. the `material` asset;
  4. the `glowMaterial` asset;
  5. `glyphs` (alloc 4, `Glyph[glyphCount]`).
* **Atlas texel size** is exactly `(s1-s0)*512 = pixelWidth`.
* **Rust walker note.** In `code_post_gfx.ff` all 9 fonts (zone offsets ~681k-718k) come *before* its first
  menulist (`ui/code.txt` at ~870k). A Rust walker that stops at menulists still reaches the fonts once it has a
  `Font_s` loader. The menu numbers in this document are stable, so hard-coding them avoids writing
  MenuList/menuDef loaders.
* **Sorted table.** Glyphs are sorted by `letter`, and entries 0..94 are exactly ASCII 32..126, so
  `glyphs[c-32]` works directly. Search the rest. 191-glyph fonts stop at 255; 96-glyph dev fonts stop at 127.
  Map unknown characters to '?'.
* **Text origin.** For every font, `y0 + pixelHeight <= 1` (descenders end at 0 or +1) and `min(y0) ≈
  -pixelHeight`. So the text origin `y` is the **bottom of the line box (the descender line)**, and the box spans
  `[y - pixelHeight, y]`. objectiveFont figures: capitals y0 -23, h 17 (baseline ≈ y-6); digits h 17-18; space dx
  7; '0' dx 13.

### 3.3 Rendering text

```
scale   = targetHeight / font.pixelHeight     // menus: targetHeight = 48 * textscale (ASSUMED IW rule)
penX    = x                                   // x = left edge (adjust for right/centre alignment using sum(dx)*scale)
for ch in string:
    g = glyph(ch) or glyph('?')
    if g.pixelWidth and g.pixelHeight:
        quad at (penX + g.x0*scale, y + g.y0*scale), size (g.pixelWidth*scale, g.pixelHeight*scale)
        uv (g.s0, g.t0) - (g.s1, g.t1); colour = rgb * atlas.alpha
    penX += g.dx * scale
```

* The text width is `sum(dx) * scale`.
* `^0..^9` colour escapes (`^3` yellow, `^7` white) appear only in campaign strings and should be skipped when
  measuring.
* textStyle 3 (SHADOWED) is ASSUMED to draw a black copy offset by +1,+1 first.
* `decode_font.py` implements exactly this; `local/fonts/sample_*.png` are the checked results.
* Hudelem font names: `default`, `bigfixed`, `smallfixed`, `objective`, `big`, `small` (VERIFIED as strings near
  the hudelem field table). They map to `normalFont`, `bigFont`, `smallFont`, `objectiveFont`, `bigFont` and
  `smallFont` (ASSUMED).

## 4. HUD images (all VERIFIED present unless noted; formats from the IWI header)

| Material (zone) | Image | Format | IWD |
|---|---|---|---|
| `hud_chalk_1..5` (nazi_zombie_prototype.ff) | `chalkmarks_1..5` | DXT3 (1,2) / DXT5 (3-5), 128x128 | iw_01 |
| `scorebar_zom_1..4` (prototype ff; engine-named) | `scorebar_zom_1..4` | DXT5 512x64 | iw_07 |
| `scorebar_zom_long_1..4` | `scorebar_zom_long_1..4` | DXT5 1024x64 | iw_07 |
| `hud_us_grenade` (common, prototype `,`ref) | `hud_us_grenade` | DXT3 64x64 | iw_04 |
| `hud_us_smokegrenade` | `us_smokegrenade` | DXT3 64x64 | iw_09 |
| `ammo_counter_bullet / riflebullet / shotgunshell / rocket / beltbullet` (common) | same names | DXT3 4x8 / DXT5 32x8 / 16x8 / 64x16 / 8x4 | iw_00 |
| `ammo_counter_tesla` (exe only; not used in Nacht) | `ammo_counter_tesla` | DXT5 16x8 | iw_24 |
| `hud_bullets_pistol/rifle/sniper/spread/support_*` (common; CoD4-style, ASSUMED unused) | same | ARGB32/LA16 32x128 | iw_04 |
| `overlay_low_health` (common) | `overlay_low_health` | DXT5 512x512 | iw_05 |
| `overlay_low_health_compass` | `compasshealthoverlay` | DXT5 512x256 | iw_02 |
| `damage_feedback` (common; unused in zombies) | `damage_feedback` | DXT5 64x128 | iw_02 |
| `damage_feedback_j` (exe string) | - | **missing** from the IWDs | - |
| `reticle_side_small` | `side_small` | DXT5 8x8 | iw_07 |
| `reticle_center_cross` | `center_cross` | DXT3 32x32 | iw_01 |
| `hud_icon_thompson`, `_kar98k`, `_mp40`, `_shotgun`, `_bar`, `_double_barrel`, `hud_icon_colt` | `hud_thompson` 128x64, `hud_kar98k` 128x32, `hud_colt` 64x64, ... | DXT5 | iw_04 (weapon `hudIcon`, used by the dpad/kill feed; not drawn on the zombie HUD) |
| `hint_usable` / `hint_health` / `hint_mantle` | same | DXT5 64x64 | iw_03 (not used: HINT_NOICON) |
| `zombie_intro` (precached in `_zombiemode.gsc`) | `zombie` | DXT5 1024x1024 | localized_english_iw00 |
| `white` / `black` (nuke flash, game-over background) | `$white` built in / `black` | - | **not in the IWDs**. `white` is the engine's `$white`; draw a solid colour. |
| font atlas | `gamefonts_pc` | DXT5 512x512 | localized_english_iw00 |

Power-up drop models: `zombie_bomb`, `zombie_skull`, `zombie_x2_icon`, `zombie_ammocan`. These are world models,
not HUD icons. Nacht has **no HUD power-up icons**.

## 5. Summary table for the implementer

Coordinates are 640x480 virtual units with (horzAlign, vertAlign) and (alignX, alignY).

| Element | Source | Image / font | Colour | Position | Behaviour |
|---|---|---|---|---|---|
| Chalk tally (rounds 1-5) | script | `chalkmarks_<n>` at 64x64 | (0.423,0.004,0), alpha 1 | (left,bottom), align (left,bottom), x=-5, y=0 | Change: fade out 0.5 s, sound, fade in 0.5 s. End of round: to white over 2.5 s, 10 blinks of 1 s each, back to red over 2.5 s. |
| Chalk 2nd group (rounds 6-10) | script | `chalkmarks_<n-5>` 64x64 | same | (left,bottom), x=64, y=0 | first group stays at 5 marks |
| Round number (11 and up) | script | `default` font (normalFont), fontScale 32 (about 64 units, see 2.10) | same | (left,bottom), align (left,bottom), x=-5, y=0 | same fades; 2nd chalk removed at round 11 |
| Intro "Round" + chalk | script | "Round" fontScale 16 (about 32 units); chalk 64x64 | white, to red over 3 s | "Round": (center,bottom), align (center,bottom), y=-265. Chalk: (center,bottom), x=-5, y=-200. | timeline in 2.1; chalk slides to bottom-left over 1.75 s |
| Score | engine ownerdraw 288 | `scorebar_zom_1` tinted; digits UI font (~15 units, ASSUMED) | bar (0.424,0.004,0) alpha 0.8; text white | rect (-103,-71,100,0) (right,bottom) | instant update |
| Point popup | script | `default` font, fontScale 8 (about 16 units), "+" label | +: (0.9,0.9,0); -: (0.423,0.004,0) | (right,bottom), align (right,middle), start (-103,-71) | 0.5 s linear move by dx -20..-59, dy -14..+15; fade to 0 during the last 0.25 s |
| Weapon name | engine ownerdraw 81 | objectiveFont, textscale 0.3095 (14.86 units) | (1,1,1,0.75), shadowed | rect (-305,-40,290,40) (right,bottom); right-aligned at x=-15, text bottom y=-40 (ASSUMED) | localized display name |
| Clip graphic | engine ownerdraw 117 | `ammo_counter_*` by `ammoCounterClip` | (1,1,1,0.65) | anchor (-79,-4) (right,bottom); icons grow leftwards (ASSUMED) | one icon per round in the clip |
| Reserve ammo | engine ownerdraw 119 | objectiveFont 0.3095 | (1,1,1,0.75), shadowed | rect (-75,4,25,25) (right,bottom); text bottom y=+4 | number only |
| Grenade icon | engine ownerdraw 103 | `hud_us_grenade` 24x24 | (1,1,1,0.65) | rect (-104,-38,24,24) (right,bottom) | hidden when there are no grenades (ASSUMED) |
| Grenade count | engine ownerdraw 105 | objectiveFont 0.3095 | (1,1,1,0.75), shadowed | rect (-84,-8,25,25) (right,bottom) | |
| Reload / LOW AMMO | engine ownerdraw 120 | auto (smallFont) 0.3095 | pulses lowAmmoWarningColor1 to 2 (ASSUMED values) | rect (-10,15,100,30) (center,center), text middle-centre (~x 320, y 255-285) | when clip <= 33%; only on gameskill 0/1 |
| Use hint | engine ownerdraw 72 + `SetHintString` | auto (smallFont) 0.3095 | white, shadowed | rect (0,70,40,40) (center,center), centred on x=320 | "Press & hold F to buy Thompson [Cost: 1500]"; no icon |
| Power-up timers | script | objectiveFont, fontScale 2 (24 units) | white | (center,top), align (center,top), y=350 (x2), 380 (insta), 290 (max ammo) | 0.5 s fade in; countdown text; Max Ammo floats to y=270 and fades over 1.5 s |
| Nuke flash | script | solid white fullscreen | white | fullscreen | 0 to 0.8 alpha over 0.2 s, hold to 0.5 s, to 0 over 1 s |
| Low-health overlay | script (`_gameskill`) | `overlay_low_health` stretched to 640x480 | texture colours | fullscreen | pulse described in 2.7 |
| Crosshair | engine | 4 x `side_small` 8x8 | white, red over enemies | screen centre, offset by spread | fades while firing or aiming |
| Game over | script | `default` font, fontScale 3 / 2 | white | (center,middle), y=-10 / +20 | fade in over 1 s |

**Zones to load for HUD data:**
* `code_post_gfx.ff`: fonts.
* `common.ff`: `ui/hud.txt` menus, ammo-counter, grenade, overlay and reticle materials, `_hud_util.gsc` /
  `_gameskill.gsc`.
* `nazi_zombie_prototype.ff`: chalk and scorebar materials, zombie localize strings, weapons.

**Images:** all from `main/iw_*.iwd`, plus the font atlas from `main/localized_english_iw00.iwd`.

**Current Bevy HUD (`crates/zm_game/src/hud.rs`), main differences (not modified):**
* It uses Bevy's default font, not the WaW atlas.
* The round counter is a big red number, not chalk images.
* The score is yellow and 40 px, with no blood bar.
* The popups are 24 px and move differently.
* It has power-up icon slots, which Nacht does not have.
* It has a hit-marker, which WaW zombies does not have.
* The damage overlay is a flat colour rather than `overlay_low_health`.
