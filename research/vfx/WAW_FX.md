# World at War effects (FxEffectDef) and how Undead Rounds plays them

Everything here was read from the user's own install (Steam v1.6) with the Rust zone
walker. Dumps live in `research/vfx/local/` (git-ignored):

```
cargo run --release -p waw_assets --example fxdump -- nazi_zombie_prototype research/vfx/local/fx_nacht.txt
cargo run --release -p waw_assets --example fxdump -- common research/vfx/local/fx_common.txt
cargo run --release -p waw_assets --example rawfile -- common maps/_fx.gsc > research/vfx/local/_fx.gsc
```

Claims are marked **VERIFIED** (checked against data or in game) or **INFERRED**
(consistent with all the data we looked at, but not proven).

## 1. Where effects live

| What | Where |
|---|---|
| Effect definitions | asset type 26 `FxEffectDef`. Nacht's zone has 190 (17 are `,name` stubs that point into `common.ff`); `common.ff` has 132. |
| Impact table | asset type 27 `FxImpactTable`, one in `common.ff` (after its 153 menus; the walker now walks menus, type 21/22, so `common.ff` reads to its last byte). |
| Weapon effects | `WeaponDef` pointers: `viewFlashEffect` 0x17c, `worldFlashEffect` 0x180, `viewShellEjectEffect` 0x28c, `worldShellEjectEffect` 0x290, `view/worldLastShotEjectEffect` 0x294/0x298, `projExplosionEffect` 0x688, `projDudEffect` 0x690, `projTrailEffect` 0x7bc, `projIgnitionEffect` 0x7d8; `impactType` 0x150. |
| Script effects | `level._effect["key"] = loadfx("path")` in the zone's rawfile scripts (`_zombiemode.gsc`, `_zombiemode_powerups.gsc`, `nazi_zombie_prototype_fx.gsc`...), and `add_zombie_powerup(name, model, hint, fx)`. |
| Placed (ambient) effects | `maps/createfx/<map>_fx.gsc` rawfile: `createOneshotEffect` / `createLoopEffect` with `origin`, `angles`, `fxid`, `delay` (Nacht: 103). |

## 2. Format (VERIFIED layout, T4_LAYOUTS.txt)

`FxEffectDef` (36 bytes): name, flags, totalSize, `msecLoopingLife`, element counts
(looping, one-shot, emission) and `elemDefs` (256 bytes each). Elements are grouped by
position: looping first, then one-shot, then emitted.

`FxElemDef` (256 bytes) holds, per element: flags; `spawn` (looping: `intervalMsec,
count`; one-shot: count range); distance ranges (`spawnRange`, `fadeInRange`,
`fadeOutRange`); `spawnDelayMsec` and `lifeSpanMsec` (int ranges); `spawnOrigin[3]`,
`spawnOffsetRadius/Height`, `spawnAngles[3]`, `angularVelocity[3]`, `initialRotation`,
`gravity`, `reflectionFactor` (float ranges = `base + amplitude * random`); `atlas`;
`elemType`; `visualCount`; velocity samples (`velIntervalCount + 1` x 96 bytes) and look
samples (`visStateIntervalCount + 1` x 48 bytes); visuals; `effectOnImpact`,
`effectOnDeath`, `effectEmitted` (names); emit distance; trail cross-section;
`sortOrder`, `lightingFrac`.

Element types (WaW numbering, VERIFIED from which visual each type loads): 0 billboard,
1 oriented sprite, 2 rotated sprite, 3 tail, 4 trail, 5 cloud, 6 model, 7 omni light,
8 spot light, 9 sound (visual = alias name), 10 decal (visual = mark materials: model,
world), 11 runner (visual = effect name). Counts in Nacht + common: 1256 billboards,
178 tails, 143 rotated, 69 oriented, 65 omni lights, 60 decals, 59 models, 38 runners,
28 clouds, 19 sounds, 7 trails.

Flags (as IW3): 0x2 spawn relative to effect, 0x10/0x20 spawn offset sphere/cylinder,
0xC0 run relative to world (0) / spawn (0x40) / effect (0x80) / offset (0xC0), 0x100 use
collision, 0x800 draw with viewmodel, 0x1000000 / 0x2000000 local / world velocity
graph, 0x4000000 gravity, 0x10000000 non-uniform size.

## 3. Semantics worked out from the data

* **Colours are B, G, R, A bytes** (VERIFIED: the Kar98k flash light is `[73,158,243]` =
  orange; world vertex colours use the same D3DCOLOR order).
* **Look sample ranges**: floats are `base + amplitude * r`; colours are a point
  between the two stored colours (`lerp(base, amp, r)`; alpha has its own random)
  (INFERRED: `255 + 255` alphas only make sense as two end points).
* **Velocity samples**: `velocity` is units per millisecond; `totalDelta` is the
  running integral where each interval counts as 1 (VERIFIED: with velocities
  0.05, 0.0119, 0.0127 the deltas are 0, 0.031, 0.043). Position at life fraction `f`
  over `N` intervals: `(d_i + v_i u + (v_{i+1}-v_i) u^2 / 2) * life_ms / N`.
  `rotationTotal` in the look samples is integrated the same way.
* **Angles are radians** (`spawnAngles` like `3.1415927`, `4.712389`), angular
  velocity radians per millisecond, `initialRotation` radians (VERIFIED by value range).
* **Gravity** is a multiple of `g_gravity` (800 in/s²; sparks 0.5-1.3 fall, smoke
  -0.1 rises) (INFERRED).
* **Sizes** are half extents in inches; tails: `size[0]` half width, `size[1]` length,
  extending from the particle along its velocity (INFERRED from the god rays and the
  box light, whose rays start at the source and go along `-X`).
* **Looping elements** spawn one particle every `intervalMsec`, up to `count`
  (`0x7fffffff` = forever) while the effect's `msecLoopingLife` lasts (`0x7fffffff` =
  forever; e.g. blood spurt 1000 ms).
* **Distance fades**: `fadeInRange` fades particles out with distance
  (`base` .. `base + amp`), `fadeOutRange` fades them out up close; `spawnRange` is the
  distance within which elements spawn (INFERRED: always consistent with `spawnRange`).
* **Atlas**: `colIndexBits` / `rowIndexBits` give the grid (`1 << bits`), `behavior`
  bits 0-1 the start frame (fixed `index`, random, by spawn order), 4 = play the frames
  over the particle's life, 8 = loop only `loopCount` times, else `fps`.
* **Blend** comes from the material's technique set name: `effect_add*` additive,
  `effect_blend`/`effect` alpha, `screen`, `multiply`; `zfeather` = soft particles;
  `falloff`, `eyeoffset`, `nofog` variants exist.
* **Impact table rows** (16, INFERRED from their contents): 0/1 small bullet entry/exit,
  2 underwater, 3/4 large, 5/6 armour piercing, 7/8 shotgun ("20mil", large flesh
  exits), 9 grenade bounce (footstep dust), 10 grenade explosion (`grenadeexp_*` by
  surface), 11 rifle grenade/rocket, 12.. dud, mortar, tank. Each row: 31 effects by
  surface type plus 4 flesh effects (body non-fatal, body fatal, head non-fatal, head
  fatal; exit rows only have the fatal ones).
* **Surface types**: the 31 names are in `waw_assets::t4::fx::SURFACE_TYPES`; the
  order is VERIFIED by the grenade's per-surface bounce sounds
  (`grenade_bounce_<surface>`). World materials carry `surfaceTypeBits` (one bit per
  type) at `MaterialInfo + 0x10`.
* **createfx** (`maps/_fx.gsc`, `maps/_utility.gsc` in common.ff): one-shots are
  `spawnFx` + `triggerFx(ent, delay)` where the default delay -15 starts the effect 15 s
  in the past (pre-warmed); loop effects are `playLoopedFx(fx, delay, ...)`, replaying
  every `delay` seconds (default 0.5). Angles give `forward` (X) and `up` (Z); most
  are `(270, 0, 0)`, i.e. X pointing up out of the floor.
* `PlayFX(fx, origin)` without a direction points the effect's X axis up.

## 4. What Nacht uses

* Weapons: every gun has view/world flashes and shell ejects (e.g. Kar98k:
  `weapon/muzzleflashes/fx_kar98k_view`, `weapon/shellejects/rifle_view`, impact type
  2). View flashes are a spark-field billboard, smoke puffs and a 48 ms orange omni
  light of radius 80; shell ejects are a runner carrying `*_blurred01`, a shell model
  with gravity 0.5 and a `shell_eject` sound.
* Grenades (`stielhandgranate`, impact type 6) have no `projExplosionEffect`: the
  explosion comes from impact row 10 by the surface underneath.
* Ray gun: `misc/fx_exp_raygun_impact` projectile explosion, own view flash.
* Scripts: eye glow `misc/fx_zombie_eye_single` on `J_Eyeball_LE`; head pop
  `impacts/flesh_hit_head_fatal_lg_exit` + `misc/fx_zombie_bloodsplat` at `j_neck`;
  `bloodspurt`; window boards `impacts/large_woodhit` three times around the board on
  break and repair; power-ups `misc/fx_zombie_powerup_on` (on the drop),
  `powerup_grab` / `powerup_wave` on pickup, nuke `misc/fx_zombie_mini_nuke`; the box
  light `env/light/fx_ray_sun_sm_short` at the weapon spawn point, angles + (90,0,0).
* Ambient (createfx, 103): fog banks, god rays, ceiling light shafts, small fires with
  smoke and embers, room smoke, wire sparks.

## 5. The player (`crates/zm_game/src/fx/`)

* `data.rs` (in the map-load task): gathers every effect reachable from the weapons,
  the impact rows they use, the script names and the createfx list (173-194 effects,
  140 materials, 110 textures for Nacht, in ~0.07 s); resolves `,name` stubs across
  zones; builds a 64-inch grid of the map's triangles with their surface types so an
  impact finds its surface and normal.
* `mod.rs`: `FxEvent` (play by name or `level._effect` key, muzzle flash, bullet impact,
  flesh impact, explosion); instances simulate in game space; particles of one material
  in one instance are one dynamic mesh of quads (pooled entities), sorted back to front
  for alpha blending; lights are point lights; models use the map's model meshes;
  sounds go to the alias player; decals are quads on the surface (30 s, 96 max);
  runners and on-death effects spawn child instances that follow their runner.
* `material.rs` / `fx.wgsl`: texture x vertex colour, additive / blended / multiplied,
  soft particles from the camera's depth prepass for `zfeather` materials.
* `hooks.rs`: eye glow, power-ups, boards and box light from game state, without
  touching those systems. Gameplay writes `FxEvent`s for muzzle flashes, bullet and
  knife hits and explosions; `fx::live()` turns off the old stand-ins.

## 6. Not done / open

* Trails (7 elements) are drawn as billboards; emitted elements (`effectEmitted`,
  distance-based emission) and `effectOnImpact` (needs particle collision) are not
  played; models do not bounce (`reflectionFactor`).
* Spot lights are point lights; effect lights only light models (the world uses its
  own lightmap shader).
* Lit particles (`lightingFrac`) are darkened by a constant instead of the light grid.
* `fxt_light_spot_beam` (zombie eye beam) is IWI format 9, which the IWI decoder does
  not read; materials without a readable texture are skipped.
* `wind`, `spawnFrustumCullRadius`, effect priority and `nofog` are ignored.
* World flashes / shell ejects (other players, AI) are unused.
