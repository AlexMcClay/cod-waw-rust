# Undead Rounds: status (2026-10-02)

Goal: rebuild CoD: World at War Zombies in Rust (Bevy), starting with Nacht der Untoten.

## Decisions

- The game code is a clean-room Bevy reimplementation (no porting of CoDWaW.exe or the GSC scripts).
  Every asset (map, models, textures, sounds, weapon stats) is read at runtime from the user's own
  install. The dist folder ships no game files.
- License: GPL-3.0, because the fastfile reader follows OpenAssetTools' zone definitions (GPL-3.0).
- WaW install: `D:\SteamLibrary\steamapps\common\Call of Duty World at War` (found automatically).

## Done

- `waw_assets`: install finder, IWD virtual file system, IWI texture decoder, and a complete World at
  War zone reader. It reads all four Zombies maps and the rest of the shared zones to the exact last
  byte, with zero unresolved references.
- Main menu (loadscreen art, menu music), options (saved), pause menu, game over, and a loading screen
  that waits for shaders.
- Nacht der Untoten loaded from the install:
  - geometry, textures, lightmap, props, boards and doors;
  - entities: windows, spawners, wall-buys with real prices, box, path nodes;
  - collision and navigation.
- Real skinned zombies (4 bodies, random heads), procedurally animated on their real joints.
- Real weapon view models and weapon fire sounds; zone sound aliases mapped in `sounds.cfg`.
- `build_release.bat` → `undead_rounds\dist\UndeadRounds\UndeadRounds.exe`.

- Real animations (XAnim decoding): zombies walk/sprint/attack/tear boards/climb/die with the game's
  animations; first-person arms + gun play the weapon's own idle, fire, rechamber, reload, raise,
  sprint and knife animations, and aim down the sights along `ads_up`.

## In progress

- Fine-tuning animation timing (reload length vs the weapon file, notetrack sounds).

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
- Perks/power (Verrückt and later maps), co-op.
