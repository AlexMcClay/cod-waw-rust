# Undead Rounds

A round-based zombie survival game written in Rust with [Bevy 0.16](https://bevyengine.org): a clean-room
recreation of Call of Duty: World at War's Zombies mode. All game code is original; **the art, sounds,
weapon stats, models and the Nacht der Untoten map itself are read at runtime from your own World at War
install.** Nothing from the game is compiled into the executable or shipped with it.

## What it loads from your install

* **Nacht der Untoten**, rebuilt from `zone/english/nazi_zombie_prototype.ff`: the map geometry and its
  textures, the baked lightmap, 1,500 placed props, the window boards and doors, and the gameplay layout
  from the map's entities (windows, zombie spawners, wall weapons and their prices, the mystery box,
  the help-room door and the upstairs debris, 634 path nodes for zombie navigation).
* **Zombies**: the real Honor Guard bodies and heads, skinned to their skeletons.
* **Weapons**: first-person gun models, fire sounds and stats (`weapons/sp/*`).
* **Sounds**: sound aliases from the fastfiles (zombie vocals, boards, purchases, power-ups...) and
  streamed music/announcer lines from the `.iwd` archives (mapped in `sounds.cfg`).
* **Menus**: the zombie loading screen art and the main menu music.

Without an install the game still runs the original prototype map, "The Bunker", with built-in sounds.

## Build and run (Windows)

1. Install Rust from https://rustup.rs (the default MSVC toolchain).
2. `build_release.bat` builds a standalone copy into `dist\UndeadRounds\`: double-click
   `UndeadRounds.exe` there. (`build.bat` builds and runs a development copy.)

The game finds World at War in your Steam libraries automatically. Otherwise set `waw_path` in
`undead.cfg` (next to the exe), pass `--waw <dir>`, or set `UNDEAD_WAW`.

Settings, `log.txt` and `crash.txt` live in `%LOCALAPPDATA%\UndeadRounds`.

## Controls

| Key | Action |
| --- | --- |
| WASD, Shift, Space | move, sprint, jump |
| Mouse, LMB / RMB | look, fire / aim down sights |
| R | reload |
| V or E | knife |
| G or mouse 4 | throw a grenade (hold to cook) |
| 1, 2, Q, mouse wheel | switch weapons (two slots) |
| F | buy wall weapons and ammo, open doors and debris, use the mystery box |
| Hold F at a window | rebuild boards (+10 points each) |
| Esc | pause menu (resume, restart, options, quit) |
| H | show or hide help |

## Layout

```
Cargo.toml              workspace (GPL-3.0)
sounds.cfg              game event -> sound alias / .iwd file mapping
textures.cfg            textures for the prototype bunker
undead.cfg              where your World at War install is
build_release.bat       builds dist\UndeadRounds\
crates/waw_assets       reading the install (no Bevy)
  install.rs            finding the install (Steam libraries, config)
  iwd.rs                .iwd archives as one virtual file system
  iwi.rs                .iwi textures (DXT1/3/5, ARGB, luminance) with mip chains
  zone.rs               .ff fastfile containers
  t4/                   full zone reader for World at War fastfiles: images, materials,
                        models + skeletons, the map (GfxWorld), sound aliases, weapons,
                        animations (raw streams), map entities
  mapents.rs            map entity text
  zombiemap.rs          a Zombies map's layout from its entities
  examples/             command-line tools (zonewalk, ffinfo, iwi2png, aliases, modelinfo...)
crates/zm_core          engine-independent rules, level data, navigation, collision, decoders
  trimesh.rs            triangle-soup collision (ray casts, sphere push-out)
  navgraph.rs           path-node navigation with door-gated links
  nav.rs, level.rs      grid navigation and layout of the prototype bunker
  wav.rs                MS-ADPCM / IMA-ADPCM decoding
  rules.rs, weapons.rs  round, points, power-up and weapon maths
crates/zm_game          the Bevy game
  menu.rs               main menu, options, pause, game over, loading screen
  nacht/                building Nacht from the install (meshes, lightmap, skinned zombies,
                        view models, collision, level and navigation)
  main.rs audio.rs world.rs player.rs weapons.rs zombies.rs interact.rs powerups.rs round.rs hud.rs
```

Tests: `cargo test --workspace`. With an install, `UNDEAD_WAW=<install> cargo test --release -p waw_assets
-- --ignored` checks the zone reader against Nacht (exact end of stream, zero unresolved references,
known vertex/asset counts).

Developer smoke test: `UNDEAD_START=nacht UNDEAD_CAPTURE=shot` starts a game directly, lets a bot play,
saves screenshots (`UNDEAD_CAPTURE_AT=4,9,30`) and exits.

## License and credits

GPL-3.0 (see `LICENSE`). The fastfile load order and struct layouts follow the zone definitions of
[OpenAssetTools](https://github.com/Laupetin/OpenAssetTools) (GPL-3.0). Call of Duty and World at War are
trademarks of Activision; this is an unofficial fan project that needs a legally owned copy of the game.
