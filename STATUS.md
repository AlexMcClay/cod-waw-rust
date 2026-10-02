# Undead Rounds: status (2026-10-02)

Goal: rebuild CoD: World at War Zombies in Rust (Bevy), starting with Nacht der Untoten.

## Decisions

- The game code is a clean-room Bevy reimplementation (no porting of CoDWaW.exe or the GSC scripts).
  Every asset (map, models, textures, sounds, fonts, weapon stats) is read at runtime from the user's
  own install. The dist folder ships no game files.
- License: GPL-3.0, because the fastfile reader follows OpenAssetTools' zone definitions (GPL-3.0).
- WaW install: `D:\SteamLibrary\steamapps\common\Call of Duty World at War` (found automatically).
- Research notes live in `research/` (see its README); game-derived dumps there are git-ignored.

## Done

- `waw_assets`: install finder, IWD virtual file system, IWI texture decoder, and a complete World at
  War zone reader. It reads all four Zombies maps and the rest of the shared zones to the exact last
  byte, with zero unresolved references; fonts and material render states included.
- Main menu (loadscreen art, menu music), options (saved), pause menu, game over, and a loading screen
  that waits for shaders.
- Nacht der Untoten loaded from the install:
  - geometry, textures, lightmap, props, boards and doors; foliage cut out with the materials' own
    render states;
  - entities: windows, spawners, wall-buys with real prices, the map's own mystery box (lid opens,
    weapons cycle as their world models), path nodes;
  - collision (world, solid props and scenery brushes) and navigation.
- Real skinned zombies and first-person arms/guns playing the game's own animations; only bolt-action
  and pump guns rechamber.
- Movement with the game's constants: run/sprint/crouch/prone, jump, acceleration and friction.
  Buying, rebuilding and the box only work on the player's own floor.
- Sound as the game plays it: sound aliases with their volume/pitch/3D falloff, weapon and zombie
  animation notetracks, the map's music flow and ambience, scripted box/board/power-up sequences.
  xWMA sounds decoded with Windows' WMA decoder.
- HUD laid out like WaW's: chalk tallies, red score bar with popups, weapon name, clip icons, reserve,
  Reload warning, use hints, power-up text, nuke flash, low-health overlay, all with the game's fonts.
- Lighting from the map's own data: lightmap pages and primary lights in a custom world shader, the
  light grid (with its primary light baked in) as an irradiance volume for models, sky-box model, fog
  from the art script and the film grade from the vision file. Nothing is Nacht-specific.
- Weapons use the zone's own WeaponDef stats (damage/range falloff, hit-location multipliers, fire
  rate, clip/reserve, reload timing, spread, ADS, penetration); grenades; the PPSh.
- Player collides with the map's clipMap brushes; prone speed fixed; stairs climb like ramps.
- Zombie/round rules read from Nacht's own scripts (`_zombiemode_prototype.gsc` in patch.ff plus
  `mp/zombiemode.csv`): health curve, counts, spawn delay, gaits, attack damage, regen, points by hit
  location, power-up drop rules (see `research/gameplay/ZOMBIE_MECHANICS.md`).
- Particle effects from the game's FX assets: muzzle flashes, shell ejects, impacts by surface type,
  blood, grenade explosions, power-up/box/board effects and all 103 placed ambient effects
  (see `research/vfx/WAW_FX.md`).
- Zombie navigation on the map's AI collision: walkable node links only (18-unit step), one shared
  route field to the player anywhere on the map (through opened doors), windows picked by route
  length, smooth stairs. Dev switches: `UNDEAD_TEST_ZOMBIE_PATH`, `UNDEAD_TEST_OPEN_DOORS`,
  `UNDEAD_TEST_ZOMBIE_SPEED=walk|run|sprint`.
- Player aim time, zoom, move speed and sprint length come from the held weapon.
- Scoped weapons show their scope overlay (`adsOverlayShader`) instead of the gun when fully aimed.
- Models sample the light grid in their own shader (model_material.rs): Bevy's IrradianceVolume
  dropped screen tiles for a map-sized volume (black, flickering blocks). Effect lights go through a
  pool of 8, like the game's few dynamic lights. Decal sort layers get a depth offset.
- Dismemberment from the game's scripts: characters (AI type -> character -> xmodelalias) give the
  body/head and the gib models; arms, legs, guts and heads come off by the spawner/death script
  rules (head pops at <=10% health and bleed out, kill-shot gibs, explosions by nearest joint),
  severed parts fly with blood trails, and a zombie that loses a leg crawls (crawl, crawl melee,
  crawl vault, crawl death). Per-bone hit boxes, solid zombie bodies, blended animations.
- Directional sound: our own stereo panning for 3D aliases (Bevy's spatial audio is inverted/weak).
- Test runs (`UNDEAD_CAPTURE`, `UNDEAD_TEST_*`) hand control to the player on their first input;
  `UNDEAD_TEST_ADS=1` holds aim.
  `UNDEAD_COLLISION_MAP=<file.ppm>` maps what stops the player around the start.
- `build_release.bat` → `undead_rounds\dist\UndeadRounds\UndeadRounds.exe`.

## Open

- A GPU "not enough memory" crash (wgpu `create_texture`) was logged once at 19:36. It did not
  reproduce: restarting five times in a row kept GPU memory flat at ~775 MB.

## Next (queued)

- About 20 small outdoor path-node clusters are not linked to the rest (no spawner uses them).
- Ray Gun self-damage.
- Not yet from the rules: burning damage, the stuck-zombie cleanup.
- FX gaps: trails drawn as sprites, no particle collision, IWI format 9 textures (light beams),
  distortion (heat haze) elements and character blood decals are not drawn.
- Perks/power (Verrückt and later maps), co-op.
