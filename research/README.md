# Research: World at War asset formats and animation

This folder holds the reverse-engineering work behind `crates/waw_assets` and the
animation code in `crates/zm_game`. It has notes, the Python prototypes used to
work out each format, verification reports, and reference renders.

All findings were checked against the user's own Steam install of Call of Duty:
World at War (PC, v1.6). Nothing here is needed to build or run the game.

## Layout

| Path | Contents |
|---|---|
| `t4/T4_ZONE_NOTES.md` | **Start here.** The fastfile (`.ff`) and zone format: container, blocks, pointers, asset table, and the Material, GfxImage, GfxWorld, XModel and sound layouts. Each claim is marked VERIFIED or ASSUMED. |
| `t4/T4_XANIM_NOTES.md` | XAnimParts: keyframe streams, value encodings, timeline, how anims apply to skeletons, root motion, viewmodels, weapon anim slots, zombie anims. |
| `t4/T4_LOAD_SPEC.txt` | A language-neutral load order for every T4 asset struct (member order, alignment, sizes, counts). |
| `t4/T4_LAYOUTS.txt`, `t4/t4_layouts.json` | The 32-bit struct layouts: 232 structs, with sizes, offsets and member types. |
| `t4/*.py` | Python prototypes and verification scripts. The file table is in §1 of `T4_ZONE_NOTES.md` and of `T4_XANIM_NOTES.md`. |
| `t4/*.txt` (reports) | `verify_all_zones.txt` (all 128 `.ff` files walk to exact EOF), `verify_all_xanims.txt`, `assets_walk*.txt` (asset lists in load order), `xanim_list_*`, `weapon_anims_nacht.*`, `world_materials.txt`, `trace_gfxworld.txt`. |
| `t4/xanim_out/*.png` | Software renders that confirm the posing rules: the zombie walk, the zombie anim grid, additive anims, and the Kar98k and MP40 viewmodel poses. |
| `t4/nacht_topdown.png` | A top-down render of the parsed Nacht GfxWorld. |
| `t4/ref/` | OpenAssetTools source and headers that were consulted (GPL-3.0, © Laupetin and contributors). |
| `t4/gen/` | Scripts that built OAT's ZoneCodeGenerator, and its generated T4 loader code (`out/`, `out2/`). |
| `t4/oat/*.txt` | OAT `Unlinker` asset list and dump logs, used as ground truth for comparisons. |
| `hud/WAW_ZOMBIE_HUD.md` | The zombie HUD: who draws each element (script hudelem or engine menu ownerdraw), positions in 640x480, images, colours and timings, and the `Font_s` format and text rendering. Scripts: `survey_zones.py`, `dump_menus.py`, `dump_strings.py`, `decode_font.py`, `dump_weapon_hud.py`, `dump_hud_images.py`, `render_hud_mock.py`. Their output goes to `hud/local/`, which is git-ignored. |
| `gameplay/WEAPONS.md` | Every Nacht weapon's numbers (damage and falloff, hit-location multipliers, fire/rechamber, reload and add times, ammo, spread, ADS, handling, penetration), their sources (zone WeaponDef = IWD weapon file, zombie scripts, wiki) and the discrepancies found. `gen_weapon_defaults.py` regenerates the tables and `crates/zm_core/src/weapon_defaults.rs` from an install. |
| `extraction/extract_zombies.py` | The first extractor: it dumps rawfiles from the zombie `.ff` files and copies zombie content out of the IWDs. |
| `screenshots/` | Milestones of the Bevy port, from the prototype bunker to real Nacht, skinned zombies and the viewmodel fix. |
| `data/` | **Local only.** Textures and lightmaps decoded while testing the IWI decoder. |

## What is not in git

The `.gitignore` keeps everything derived from the game's own files out of the
repository. That covers:
- model and world OBJs and the decompressed `zone.bin`;
- the map-entity text and the sound, world and model JSON dumps;
- OAT's asset dumps and binaries, and the OAT source clone;
- `data/`.

These files stay on disk for local work. To regenerate them from an install:
1. `py t4/decomp.py` writes `zone.bin`.
2. `py t4/zonewalk.py`, `parse_world.py`, `parse_xmodel.py <name>`, `parse_sounds.py` and `render_xanim.py` write the dumps and renders.

Several scripts hard-code the install path
`D:\SteamLibrary\steamapps\common\Call of Duty World at War`; edit it if yours
differs.

## Key findings

**Fastfiles**
- Format: `IWffu100`, version 387, then one zlib stream.
- The zone has a 36-byte header (size, external size, 7 block sizes). It is followed by the script strings (string 0 is a null pointer) and the asset table.
- Blocks: TEMP 0, RUNTIME 1–3 (these never consume file bytes), VIRTUAL 4, LARGE 5.
- Pointer values:
  - `0` is null;
  - `-1` means the data follows inline;
  - `-2` means inline plus a VIRTUAL slot;
  - any other value is a back-reference `((v-1)>>29, (v-1)&0x1FFFFFFF)`.
- The walker consumes all four zombie zones, plus every other `.ff` in `zone/english`, exactly to EOF.

**Asset types used**
| Number | Type | Number | Type |
|---|---|---|---|
| 4 | xanim | 14 | gameworld_sp |
| 5 | xmodel | 16 | mapents |
| 6 | material | 17 | gfxworld |
| 7 | techset | 23 | localize |
| 9 | sound | 24 | weapon |
| 10 | loadedsound | 26 | fx |
| 11 | clipmap | 32 | rawfile |
| 13 | comworld | | |

**Images**
- `.iwi` version 6. Formats:
  - `0x01` ARGB, `0x02` RGB, `0x03` LA, `0x04` L8, `0x05` A8;
  - `0x0B`, `0x0C`, `0x0D` = DXT1, DXT3, DXT5;
  - `0x06`–`0x0A` are wavelet and are not supported.
- The largest mip is stored last.
- The colour-map sampler hash is `0xa0ab1041`.

**World and lightmaps**
- World vertices are 44 bytes.
- The primary lightmap is an L8 sun-shadow mask. The secondary is a 512×1024 ARGB image made of two stacked halves (indirect light).

**Models**
- Model vertices are 32 bytes, with half-float UVs (u = high 16 bits).
- Skinning uses rigid vertex lists (bone/64) or blend streams.

**Animations**
- Bones are sorted by quaternion type, and the data streams are read as FIFOs.
- Rotations are int16/32767 values in xyzw order. They are *absolute* local rotations. A "half" quaternion is (0, 0, z, w).
- Translations are *offsets* from the bind pose.
- Frames run from `0` to `numframes` inclusive. The delta part holds root motion.

**Viewmodels**
- The arms model is `viewmodel_usa_marine_arms`. The gun's `j_gun` attaches to `tag_weapon`.
- `tag_torso` moves only through the `ads_up`/`ads_down` anims.
- The arms model stores **zero local translations**; its animations carry the full bone offsets. A skeleton must therefore take bind locals from the model's own local quat/trans. Locals derived from the base (model-space) matrices double every offset and leave the arms in a splayed bind pose.

**Coordinates**
- The game uses inches with Z up, X forward and Y left.
- Bevy space is `(x, z, -y) * 0.0254`. Rotations convert as `C·G·Cᵀ`.

## Licensing

OpenAssetTools is GPL-3.0. `t4_loaders_gen.py` and `T4_LOAD_SPEC.txt` are
mechanical derivatives of its generated code, which is why this project is
GPL-3.0-only. The Rust loaders were written against these notes as a format
description.
