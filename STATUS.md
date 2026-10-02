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
- `build_release.bat` → `undead_rounds\dist\UndeadRounds\UndeadRounds.exe`.

## Open

- A GPU "not enough memory" crash (wgpu `create_texture`) was logged once at 19:36. It did not
  reproduce: restarting five times in a row kept GPU memory flat at ~775 MB.

## Next (queued)

- **Lighting to match the real Nacht** (reference screenshot from the user):
  - cold, desaturated moonlit look; dark interior; hard moonlight with bar shadows through the windows;
  - pale grey exponential fog outside;
  - visible sky (the skybox model's materials are currently skipped as "sky");
  - values from `maps/createart/nazi_zombie_prototype_art.gsc`: fog start 165, half-plane 835,
    half-height 200, base height 75, colour (0.5, 0.5, 0.5); vision set "zombie"; glow/bloom
    cutoff 0.5, intensity 2;
  - the lightmap bake should weigh the primary (sun shadow) page more and the indirect page less;
    point lights dimmer and neutral.
- Grenades (the HUD has the slot), muzzle flash effects (currently a glowing sphere).
- Perks/power (Verrückt and later maps), co-op.
