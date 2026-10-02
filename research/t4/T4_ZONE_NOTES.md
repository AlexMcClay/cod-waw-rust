# T4 (CoD: World at War, PC 32-bit) fastfile zone format — empirically verified notes

Target file: `zone/english/nazi_zombie_prototype.ff` (Nacht der Untoten), user's own install.
Everything marked **VERIFIED** was checked against the real file with the scripts in this folder.
**ASSUMED** = taken from reference material and not independently confirmed.

## 0. Status summary (TL;DR)

| Item | Status | Evidence |
|---|---|---|
| Container (.ff header + single zlib stream) | VERIFIED | `decomp.py` → 87,241,669-byte zone |
| XFile header, block sizes | VERIFIED | final emulated VIRTUAL offset `0x49ceeb1` and LARGE `0x41c00` equal the header block sizes exactly |
| Script strings, asset table | VERIFIED | string #0 is a **null pointer** (that is your 2-byte drift); table at 11,587; data starts at 36,707 |
| Asset type enum | VERIFIED | 6 = Material, 7 = MaterialTechniqueSet, 8 = GfxImage, 17 = GfxWorld ... (see §4) |
| Sequential walk of **all** assets | VERIFIED | `zonewalk.py` consumes the zone to the last byte (87,241,669/87,241,669), loads 5,649 assets (3,140 top-level + inline deps), names/types/order **identical** to OpenAssetTools `Unlinker --list`. **Also all 128 `.ff` files in `zone/english`** (SP, MP, all 4 zombie maps, common, code_post_gfx, ui...) walk to exact EOF with VIRTUAL/LARGE/PHYSICAL totals equal to their headers and 0 unresolved refs (`verify_all_zones.py` → `verify_all_zones.txt`) |
| Back-reference encoding | VERIFIED | `v-1 → block = v>>29, offset = v & 0x1FFFFFFF`; 32,864 data refs + 16,422 struct refs + 11,399 alias refs, all into block 4 (VIRTUAL), all resolved |
| GfxWorld geometry | VERIFIED | 91,002 verts, 203,895 indices, 67,965 tris, 3,741 surfaces, 163 materials; bbox == GfxWorld.mins/maxs; spawn ray-cast hits a tile floor at z=1 and a plaster ceiling at z=129; top-down render (`nacht_topdown.png`) shows the Nacht bunker |
| Material → texture → image name | VERIFIED | all 163 world materials' texture tables identical to OAT's material JSON dump; all 177 world image names exist as `images/<name>.iwi` in the install IWDs |
| Image pixel data | VERIFIED | 632 images: 16-byte loadDef header with `resourceSize=0` (streamed from IWD); 47 inline (lightmaps, reflection probes, ...); 30 `,`-references with no loadDef |
| XModel (verts/tris/UVs/materials/bones) | VERIFIED (skin weights: ASSUMED) | all 887 LODs of all 288 fully-inline XModels give triangle sets identical to OAT's OBJ export; bone hierarchy math checked (§7.4) |
| Sounds (alias → file / inline RIFF) | VERIFIED | 1,720 alias lists / 5,105 aliases; 1,206 inline LoadedSounds byte-identical to OAT's dump; 157/158 streamed names exist in IWDs (the last one is `null.wav`, path rule in §8) |
| Runtime-block (non-file) offset totals | NOT matched | emulated RUNTIME block ends at `0xd42f0` vs header `0xc8410`; irrelevant for parsing (runtime blocks never consume file bytes) — see §10 |

## 1. Files in this folder

| File | What |
|---|---|
| `T4_ZONE_NOTES.md` | this document |
| `T4_XANIM_NOTES.md` | XAnimParts keyframe decoding / playback (decode_xanim.py, xanim_pose.py, render_xanim.py) |
| `zonewalk.py` | **the sequential walker runtime** (stream/block emulator, pointer resolution, generic asset-pointer loader). `py zonewalk.py [zone.bin|x.ff] [-v]` |
| `t4_loaders_gen.py` | per-asset loaders (28 asset structs) — *mechanically translated* by `cpp2py.py` from OAT's ZoneCodeGenerator output (`gen/out/T4/XAssets/*/*_load_db.cpp`) |
| `t4_layouts.json`, `T4_LAYOUTS.txt` | 32-bit struct layouts: size, member offsets, member C types (232 structs) |
| `T4_LOAD_SPEC.txt` | language-neutral rendering of every loader: exact member order, alignments, element sizes, counts, conditions — **use this as the porting spec** |
| `t4assets.py` | helpers: material/image info, `R_HashString`, packed normal / half-float texcoord decoding, back-ref data access |
| `parse_world.py` | GfxWorld → `nacht_world.obj/.mtl` (static world), `nacht_brushmodels.obj` (submodels placed by MapEnts), `world_surfaces.json`, `world_materials.txt`, `world_brushmodels.json`, `world_smodels.json` |
| `parse_xmodel.py` | XModel → `<name>_lod0.obj` + `_info.json` (bones, materials, LODs). `py parse_xmodel.py <name> [lod] [zone.ff]` |
| `parse_sounds.py` | alias lists → `sounds.json`; verifies inline RIFF bytes vs OAT |
| `trace_asset.py` | prints every stream read (zone offset, size, block, block offset, loader fn) for one top-level asset. `trace_gfxworld.txt` = full GfxWorld trace |
| `verify_all_xmodels.py`, `verify_all_zones.py`, `compare_*.py`, `check_*.py`, `probe_*.py`, `raycast_spawn.py`, `render_topdown.py` | verification scripts referenced below |
| `viewmodel_usa_colt45_pistol_lod0.obj` | Colt viewmodel exported from **common.ff** (3,449 verts / 3,062 tris; identical to OAT) |
| `gen_layouts.py`, `cpp2py.py`, `make_spec.py` | generators for the json/py/txt above |
| `assets_walk.txt` | every asset in load order: `type, name, zone_start, zone_end` |
| `nacht_mapents.txt` | MapEnts entity string (1,037 entities) |
| `ref/` | downloaded OAT headers/sources (reference only), `gen/` OAT clone + generated C++, `oat/` OAT release binaries + full dump of the zone (ground truth), `zone.bin` decompressed zone |

**Licensing note:** OpenAssetTools is GPL-3.0. `t4_loaders_gen.py` and `T4_LOAD_SPEC.txt` are mechanical derivatives of OAT-generated code. For a clean-room Rust implementation, use them (and the layouts) as a *format description*; write the Rust loaders yourself (or generate them from your own schema) rather than transliterating code.

## 2. Container and zone header (VERIFIED)

```
.ff file:  char magic[8] = "IWffu100"; u32 version = 387 (0x183); then ONE zlib stream (zlib.decompress(data[12:]))
zone (decompressed), little-endian:
  0x00 u32 size            = len(zone) - 36            (87,241,633)
  0x04 u32 externalSize    = 197,292,681 (streamed-image/sound bookkeeping, not used for parsing)
  0x08 u32 blockSize[7]    = [0x200364, 0xc8410, 0, 0, 0x49ceeb1, 0x41c00, 0]
  0x24 XAssetList (16 bytes, read raw — not inside any block)
```
Block indices (XFILE_BLOCK_*): `0 TEMP, 1 RUNTIME, 2 LARGE_RUNTIME, 3 PHYSICAL_RUNTIME, 4 VIRTUAL, 5 LARGE, 6 PHYSICAL`.

## 3. Stream / block semantics (the rules a walker must emulate) (VERIFIED)

The zone is a depth-first serialization; the loader keeps **one 32-bit offset per block** and a **block stack**.

1. **Reading**: every read of N bytes goes into the block on top of the stack and advances that block's offset by N.
   * TEMP, VIRTUAL, LARGE, PHYSICAL: the N bytes are consumed from the file.
   * RUNTIME / LARGE_RUNTIME / PHYSICAL_RUNTIME: **no file bytes** are consumed (zero-filled reservation). Example: `GfxWorld.reflectionProbeTextures`, `lightmapPrimaryTextures`, `cellCasterBits`, `dpvs.smodelVisData[3]`, `surfaceMaterials`, ...
2. **Alloc(align)** (done before each inline pointee): round the *block offset* of the top block up to `align`. **The file is never padded** — alignment only affects block offsets (which matter for back-references). Typical aligns: 1 (char data/strings), 2 (u16 arrays), 4 (structs), 16 (`GfxPackedVertex`, `XSurfaceTri16`, `MaterialConstantDef`, collision nodes), 128 (`raw_uint128` runtime), 2048 (`PrimedSound.buffer`, LARGE block). Exact align per member: `T4_LOAD_SPEC.txt` (`X.f = alloc(A)`).
3. **Push/Pop**: `push(TEMP)` saves the TEMP offset; `pop()` of TEMP restores it (TEMP memory is reused per asset). Other blocks are never rewound. Every asset header struct is read into **TEMP**; its members are read with **VIRTUAL** pushed (unless the spec pushes another block).
4. **Pointer field values** in serialized structs:
   * `0` → null.
   * `0xFFFFFFFF` (-1, FOLLOWING) → pointee is serialized inline, right here in the stream, at this point of the depth-first order.
   * `0xFFFFFFFE` (-2, INSERT) → like -1, **and** first reserve a 4-byte pointer slot in VIRTUAL (`align VIRTUAL to 4; slot=(4, off); off += 4`; no file bytes). Later references to this asset point at that slot. Only used for asset pointers / pointers whose struct lives in TEMP (738 occurrences here).
   * anything else → back-reference to already-loaded memory: `o = v - 1; block = o >> 29; offset = o & 0x1FFFFFFF` (3 block bits). In this zone **every** back-reference has block 4 (VIRTUAL).
   * Non-"reusable" pointer members (per zonecode) are only tested for `!= 0`; any non-zero value means inline data follows (always -1 in practice).
5. **Three kinds of back-reference** (which one applies is fixed per member, see spec):
   * `ptr_native` — points at raw data (vertex arrays, strings, u16 index arrays...). Resolve through a map *(block offset → file offset)* built while reading.
   * `ptr_lookup` — points at a previously loaded struct (e.g. shared `textureTable`, `XRigidVertList`, `pathnode_tree_t`).
   * `alias_lookup` — used for **asset pointers** (and temp-block members): the offset points at a *pointer slot* (an XAsset table entry's header field, a -2 INSERT slot, or a pointer member of an already-loaded struct in VIRTUAL); the referenced object is whatever was stored in that slot.
6. **Strings** (`const char*`): -1 → `alloc(1)` + NUL-terminated bytes inline; otherwise a `ptr_native` back-reference (the linker de-duplicates strings: e.g. GfxWorld's name is a back-ref to ComWorld's `maps/nazi_zombie_prototype.d3dbsp` at zone 37,402,559 — that's why you only saw it once).
7. **Generic asset pointer** (every `Material*`, `GfxImage*`, `XModel*`, ... and every XAsset table entry):
   ```
   push(TEMP)
   if ptr == -1 or ptr == -2:
       align TEMP to 4
       if ptr == -2: slot = insert_ptr()            # 4 bytes in VIRTUAL, no file bytes
       read <AssetStruct> header (N bytes, into TEMP) ; then load its members (push VIRTUAL ... pop)
       register asset; if slot: slot := asset
   else: asset = alias_lookup(ptr)
   pop()
   ```
   **Exception — StringTable (type 33):** its header is *not* loaded via TEMP: no push, `-1` → `alloc(4)` in the current block (VIRTUAL) and the 16-byte header is read there; any other value is a plain `ptr_native` back-reference. VERIFIED: with this rule `code_post_gfx.ff` and `common.ff` (which contain string tables) end with VIRTUAL == header size and 0 unresolved refs; with TEMP they drift by 16 bytes per table. (Not present in nazi_zombie_prototype.)
   Asset names beginning with `,` are **references** to assets that live in another zone (e.g. `,viewmodel_usa_colt45_pistol`, `,$identitynormalmap`, `,null.wav`): they are serialized as tiny stubs (name only / no data). Strip the comma for file lookups.

## 4. XAssetList, script strings, asset table (VERIFIED)

```
zone+36: i32 scriptStringCount = 552; u32 scriptStrings = -1; i32 assetCount = 3140; u32 assets = -1
push(VIRTUAL)
  alloc(4); read u32[552] string pointers            (zone 52..2259)    -> ptr[0] == 0 (NULL!), ptr[1..551] == -1
  for each ptr == -1: alloc(1); NUL-terminated string  ('tag_view', 'tag_ads', ..., 'tag_turret')
  alloc(4); read XAsset[3140] = {u32 type; u32 header=-1}   at zone 11,587 (VIRTUAL offset of entry i = base + 8*i; header slot = +4)
  for each entry: generic asset pointer load (§3.7) of TYPE_STRUCT[type]
pop()
```
First asset data byte: zone offset **36,707** (= 11,587 + 8*3,140). `ScriptString` values (u16) used by XModel bone names / XAnim names index this 552-entry table (index 0 = null).

**Asset type enum (VERIFIED by full walk; = OAT `T4::XAssetType`):**
```
0 xmodelpieces  1 physpreset  2 physconstraints  3 destructibledef  4 xanimparts  5 xmodel  6 material
7 techniqueset  8 image  9 sound(snd_alias_list_t)  10 loaded_sound  11 clipmap  12 clipmap_pvs  13 comworld
14 gameworld_sp  15 gameworld_mp  16 map_ents  17 gfxworld  18 lightdef  19 ui_map  20 font  21 menulist
22 menu  23 localize  24 weapon  25 snddriverglobals  26 fx  27 impactfx  28 aitype  29 mptype  30 character
31 xmodelalias  32 rawfile  33 stringtable  34 packindex
```
Top-level table counts here: 1→6, 3→2, 4→364, 5→288, **6 (material)→15, 7 (techniqueset)→340**, 9→1720, 11→1, 13→1, 14→1, 17→1, 18→2, 23→126, 24→44, 26→190, 32→39. There are **no top-level images**; images (and most materials) are loaded **inline as dependencies** at their first use. Including inline dependencies the zone contains: sound 1720, loadedsound 1249, image 709, material 542, xanim 364, techniqueset 340, xmodel 297, fx 190, localize 126, weapon 44, rawfile 39, physpreset 13, physconstraints 7, destructibledef 2, lightdef 2, comworld 1, gfxworld 1, gameworldsp 1, mapents 1 (inline in clipmap), clipmap 1. Table order around the world: …1514 comworld, 1515-1516 lightdef, 1517-1682 techsets, **1683 gfxworld** (zone 41,801,463..56,873,697), 1684 gameworldsp, 1685 clipmap (MapEnts text at zone 58,746,489), 1686-2609 sounds …

Loaders needed to walk *this* zone: PhysPreset, PhysConstraints, DestructibleDef, XAnimParts, XModel, Material, MaterialTechniqueSet, GfxImage, snd_alias_list_t, LoadedSound, clipMap_t, ComWorld, GameWorldSp, MapEnts, GfxWorld, GfxLightDef, LocalizeEntry, WeaponDef, FxEffectDef, RawFile (20 structs; the other 8 in `t4_loaders_gen.py` are for other zones).

## 5. Material (asset type 6) (VERIFIED)

Header 112 bytes (in TEMP):
```
0x00 MaterialInfo info (32):  0x00 char* name | 0x04 u8 gameFlags | 0x05 u8 sortKey | 0x06 u8 textureAtlasRowCount
                              0x07 u8 textureAtlasColumnCount | 0x08 u64 drawSurf | 0x10 u32 surfaceTypeBits
                              0x14 u32 layeredSurfaceTypes | 0x18 u16 hashIndex
0x20 i8  stateBitsEntry[59]   (technique index -> stateBits index, -1 = none)
0x5B u8  textureCount   0x5C u8 constantCount   0x5D u8 stateBitsCount   0x5E u8 stateFlags   0x5F u8 cameraRegion
0x60 MaterialTechniqueSet* techniqueSet
0x64 MaterialTextureDef*   textureTable     (reusable)
0x68 MaterialConstantDef*  constantTable    (reusable)
0x6C GfxStateBits*         stateBitsTable   (reusable)
```
Inline order after the header (push VIRTUAL):
1. `info.name` string
2. `techniqueSet` → generic asset ptr (usually a back-ref alias; techsets are top-level)
3. `textureTable`: -1 → `alloc(4)`, `MaterialTextureDef[textureCount]` (16 B each), **then for each entry in order** its `u.image` (generic asset ptr → GfxImage inline or alias), or `u.water` (water_t, 68 B) if `semantic == 11`; else `ptr_lookup`
4. `constantTable`: -1 → `alloc(16)`, `MaterialConstantDef[constantCount]` (32 B: u32 nameHash, char name[12], vec4 literal); else `ptr_native`
5. `stateBitsTable`: -1 → `alloc(4)`, `GfxStateBits[stateBitsCount]` (8 B: u32 loadBits[2]); else `ptr_native`

`MaterialTextureDef` (16 B): `0x0 u32 nameHash | 0x4 char nameStart | 0x5 char nameEnd | 0x6 u8 samplerState | 0x7 u8 semantic | 0x8 u8 isMatureContent | 0xC union{GfxImage* image; water_t* water}`.
* `nameHash = R_HashString(sampler name)`: `h=0; for c in name: h = (c | 0x20) ^ (33*h)` (u32). VERIFIED: `colorMap 0xa0ab1041, normalMap 0x59d30d0f, specularMap 0x34ecccb3, colorMap1 0xb60d1850, colorMap2 0xb60d1853, normalMap1 0x9434aede, specularMap1 0xd2866322`. `nameStart/nameEnd` = first/last char of the sampler name ('c'..'p' for colorMap).
* semantic: `0 2D, 1 FUNCTION, 2 COLOR_MAP, 5 NORMAL_MAP, 8 SPECULAR_MAP, 11 WATER_MAP`.
* **Diffuse texture = slot with nameHash 0xa0ab1041** (layered world materials like `*11n_10n` also have colorMap1/colorMap2 for blend layers 1/2; the per-vertex blend data is in `GfxWorld.vld`).

Example (top-level asset #237, `zombie_intro`, from `trace_asset.py 237`):
```
zone 2407126 +112  TEMP     Material header
zone 2407238 +13   VIRTUAL  "zombie_intro\0"
                            techniqueSet = 0x8000326d -> alias (block 4, off 0x326c)
zone 2407251 +16   VIRTUAL  MaterialTextureDef[1]   (nameHash a0ab1041 = colorMap, image ptr = -1)
zone 2407267 +36   TEMP     GfxImage header        (texture.loadDef = -2 -> 4-byte VIRTUAL slot reserved)
zone 2407303 +7    VIRTUAL  "zombie\0"             (image name)
zone 2407310 +16   TEMP     GfxImageLoadDef        (format 'DXT5', resourceSize 0 => pixels streamed from IWD)
zone 2407326 +8    VIRTUAL  GfxStateBits[1]
```

### 5.1 GfxImage (asset type 8; always inline under a material/world here) (VERIFIED)
Header 36 B (TEMP): `0x00 i32 mapType (3=2D,4=3D,5=CUBE) | 0x04 GfxImageLoadDef* texture.loadDef | 0x08 u16 picmip | 0x0A bool noPicmip | 0x0B u8 semantic | 0x0C u8 track | 0x10 int cardMemory[2] | 0x18 u16 width | 0x1A u16 height | 0x1C u16 depth | 0x1E u8 category | 0x1F bool delayLoadPixels | 0x20 char* name`.
Order (reordered!): **name string first**, then `texture.loadDef` (pushed into **TEMP**; -1/-2 → `alloc(4)`, read 16 B header `{i8 levelCount; i8 flags; u16 dimensions[3]; i32 format; u32 resourceSize}` then `resourceSize` bytes of pixel data; else alias).
* `format` is a D3D format / FourCC (`'DXT1'=0x31545844`, `'DXT5'`, 21 = A8R8G8B8 ...).
* Here: 632 images have `resourceSize = 0` → pixels are in `main/iw_*.iwd` as `images/<name>.iwi`; 47 have inline pixels (lightmaps `*lightmap0_primary/secondary`, `*reflection_probe*`, `$outdoor`, ...); 30 are `,name` refs with `loadDef = 0`.
* World image names (incl. `~...` composite specular names and `$identitynormalmap`) → **all 177 found** in the IWDs (`images/<lowercase name without leading ','>.iwi`).

### 5.2 MaterialTechniqueSet (type 7) (VERIFIED by walk)
Header 248 B: `0x0 char* name | 0x4 u8 worldVertFormat | 0x5 bool hasBeenUploaded | 0x6 u8 unused | 0xC MaterialTechnique* techniques[59]`. Each technique is a **variable-size** struct read in one piece: 8-byte header `{char* name; u16 flags; u16 passCount}` + `passCount * 20` bytes of `MaterialPass` (vertexDecl*, vertexShader*, pixelShader*, u8 perPrim/perObj/stable arg counts, u8 customSamplerFlags, MaterialShaderArgument* args); then per pass: vertexDecl (104 B), vertexShader/pixelShader (name + D3D bytecode inline), args (8 B each, union by type); then the technique name. Not needed for rendering but must be walked.

## 6. GfxWorld (type 17) (VERIFIED)

Header 800 B (TEMP) at zone **41,801,463**. Fields used for rendering (offsets):
```
0x000 char* name (back-ref)       0x004 char* baseName ("nazi_zombie_prototype")
0x008 int planeCount 5052         0x00C int nodeCount 1855
0x010 int indexCount 203895       0x014 u16* indices
0x018 int surfaceCount 3741       0x01C int streamInfo
0x020 int skySurfCount 4          0x024 int* skyStartSurfs   0x028 GfxImage* skyImage   0x02C u8 skySamplerState
0x030 char* skyBoxModel           0x034 u32 vertexCount 91002
0x038 GfxWorldVertexData vd  {0x038 GfxWorldVertex* vertices; 0x03C void* worldVb (not serialized)}
0x040 u32 vertexLayerDataSize 147916
0x044 GfxWorldVertexLayerData vld {0x044 u8* data; 0x048 void* layerVb}
0x04C u32 vertexStream2DataSize   0x050 SunLightParseParams sunParse (136 B)   0x0D8 GfxLight* sunLight
0x0DC float sunColorFromBsp[3]    0x0E8 u32 sunPrimaryLightIndex   0x0EC u32 primaryLightCount 21
0x0F0 int cullGroupCount 0        0x0F4 u32 reflectionProbeCount   0x0F8 GfxReflectionProbe* reflectionProbes
0x0FC GfxTexture* reflectionProbeTextures (runtime)   0x100 u32 coronaCount   0x104 GfxLightCorona* coronas
0x108 GfxWorldDpvsPlanes dpvsPlanes {0x108 int cellCount; 0x10C cplane_s* planes; 0x110 u16* nodes; 0x114 u32* sceneEntCellBits}
0x118 int cellBitsCount           0x11C GfxCell* cells       0x120 int lightmapCount   0x124 GfxLightmapArray* lightmaps
0x128 GfxLightGrid lightGrid (56 B)   0x160 GfxTexture* lightmapPrimaryTextures   0x164 lightmapSecondaryTextures (runtime)
0x168 int modelCount 145          0x16C GfxBrushModel* models
0x170 float mins[3]  0x17C float maxs[3]   (-6464,-5797,-188)..(5824,5787,1472)   0x188 u32 checksum
0x18C int materialMemoryCount 163 0x190 MaterialMemory* materialMemory
0x194 sunflare_t sun (96 B)       0x1F4 float outdoorLookupMatrix[4][4]   0x234 GfxImage* outdoorImage
0x238..0x250 runtime-only pointers (cellCasterBits, sceneDynModel, sceneDynBrush, primaryLight*ShadowVis, nonSunPrimaryLightForModelDynEnt)
0x254 GfxShadowGeometry* shadowGeom   0x258 GfxLightRegion* lightRegion
0x25C GfxWorldDpvsStatic dpvs (100 B):
      +0x00 u32 smodelCount 1506   +0x04 u32 staticSurfaceCount 3481
      +0x08 litSurfsBegin 0 / +0x0C litSurfsEnd 3442 / +0x10 decalSurfsBegin 3442 / +0x14 decalSurfsEnd 3466 / +0x18,+0x1C emissive 3466..3466
      +0x20 u32 smodelVisDataCount  +0x24 u32 surfaceVisDataCount  +0x28 smodelVisData[3]  +0x34 surfaceVisData[3]  +0x40 lodData (runtime)
      +0x44 u16* sortedSurfIndex  +0x48 GfxStaticModelInst* smodelInsts  +0x4C GfxSurface* surfaces
      +0x50 GfxCullGroup* cullGroups  +0x54 GfxStaticModelDrawInst* smodelDrawInsts  +0x58 surfaceMaterials (rt)  +0x5C surfaceCastsSunShadow (rt)
0x2C0 GfxWorldDpvsDynamic dpvsDyn (48 B)   0x2F0 u32 worldLodChainCount  0x2F4 worldLodChains  0x2F8 worldLodInfoCount  0x2FC worldLodInfos
0x300 u32 worldLodSurfaceCount  0x304 u32* worldLodSurfaces  0x308 float waterDirection  0x30C GfxWaterBuffer waterBuffers[2]  0x31C Material* waterMaterial
```
(All members: `T4_LAYOUTS.txt` → `struct GfxWorld`.)

**Inline order** (push VIRTUAL; `[RT]` = RUNTIME block, no file bytes), with this zone's file offsets:
1. name (back-ref) · baseName (22 B @41,802,263)
2. `indices` alloc 2, `u16[indexCount]` (@41,802,285, 407,790 B)
3. `skyStartSurfs` alloc 4 `int[skySurfCount]` · `skyImage` (asset ptr, -2 here) · `skyBoxModel` string
4. `sunLight` (GfxLight 64 B; its `def` → GfxLightDef asset ptr)
5. `reflectionProbes` alloc 4 `GfxReflectionProbe[reflectionProbeCount]` (16 B) then each probe's `reflectionImage` (inline cube images, 131,064 B pixels each)
6. [RT] reflectionProbeTextures · `coronas` (32 B each)
7. `dpvsPlanes`: planes alloc 4 `cplane_s[planeCount]` (20 B) (reusable) · nodes alloc 2 `u16[nodeCount]` · [RT] sceneEntCellBits
8. `cells` alloc 4 `GfxCell[cellCount]` (56 B) then per cell: aabbTree (40 B each; smodelIndexes u16[]), portals (68 B; vertices vec3[]), cullGroups int[], reflectionProbes u8[]
9. `lightmaps` alloc 4 `GfxLightmapArray[lightmapCount]` (8 B) then per entry primary/secondary images (inline, 2 MiB lightmaps go through TEMP)
10. `lightGrid`: rowDataStart u16[maxs[rowAxis]-mins[rowAxis]+1] · rawRowData u8[rawRowDataSize] · entries (4 B) · colors (168 B)
11. [RT] lightmapPrimaryTextures, lightmapSecondaryTextures
12. `models` alloc 4 `GfxBrushModel[modelCount]` (56 B, @51,974,652)
13. `materialMemory` alloc 4 `MaterialMemory[163]` (8 B: Material*, int memory) then each `material` (→ **all world materials are serialized here, inline**)
14. `vd.vertices` alloc 4 `GfxWorldVertex[vertexCount]` (44 B, **@52,067,078, 4,004,088 B**)
15. `vld.data` alloc 1 `u8[vertexLayerDataSize]` (@56,071,166)
16. `sun` (sunflare_t: spriteMaterial, flareMaterial asset ptrs) · `outdoorImage`
17. [RT] cellCasterBits, sceneDynModel, sceneDynBrush, primaryLightEntityShadowVis, primaryLightDynEntShadowVis[2], nonSunPrimaryLightForModelDynEnt
18. `shadowGeom` GfxShadowGeometry[primaryLightCount] (12 B; sortedSurfIndex u16[], smodelIndex u16[]) · `lightRegion` GfxLightRegion[primaryLightCount] (hulls 80 B, axis[])
19. `dpvs`: [RT] smodelVisData[3], surfaceVisData[3], lodData · sortedSurfIndex u16[staticSurfaceCount] · smodelInsts (28 B) · **surfaces** alloc 4 `GfxSurface[surfaceCount]` (52 B, @56,540,613) then each surface's `material` (aliases to step 13) · cullGroups (32 B) · **smodelDrawInsts** alloc 4 `GfxStaticModelDrawInst[smodelCount]` (92 B, @56,735,145) then each `model` (XModel asset ptr) · [RT] surfaceMaterials, surfaceCastsSunShadow
20. `dpvsDyn` ([RT] only) · worldLodChains (24 B) · worldLodInfos (12 B) · worldLodSurfaces u32[] · waterBuffers[2] (buffer vec4[bufferSize/16]) · waterMaterial

(Full read-by-read trace with block offsets: `trace_gfxworld.txt`.)

**GfxWorldVertex (44 B):** `0x00 float xyz[3] | 0x0C float binormalSign | 0x10 u8 color[4] (OAT reads bytes 0..3 as R,G,B,A — ASSUMED) | 0x14 float texCoord[2] | 0x1C float lmapCoord[2] | 0x24 PackedUnitVec normal | 0x28 PackedUnitVec tangent`. World texcoords are plain floats (unlike XModel). PackedUnitVec decode (VERIFIED, |n| = 1.0001 ± 0.006): `s = (b3 + 192) / 32385; n = ((b0-127)*s, (b1-127)*s, (b2-127)*s)`.

**GfxSurface (52 B):** `0x00 srfTriangles_t tris {0x00 int vertexLayerData; 0x04 int firstVertex; 0x08 u16 vertexCount; 0x0A u16 triCount; 0x0C int baseIndex} | 0x10 int pad | 0x14 Material* material | 0x18 u8 lightmapIndex | 0x19 u8 reflectionProbeIndex | 0x1A u8 primaryLightIndex | 0x1B u8 flags | 0x1C float bounds[2][3]`.
Triangle t of a surface: `vertex = firstVertex + indices[baseIndex + 3*t + k]` (indices are **local** to the surface; max local index < vertexCount: VERIFIED 0 violations). `vertexLayerData` = byte offset into `vld.data` for layered (blend) materials.

**Brush models** `GfxBrushModel` (56 B): `0x00 float writable.mins[3] | 0x0C writable.maxs[3] | 0x18 float bounds[2][3] | 0x30 u32 surfaceCount | 0x34 u32 startSurfIndex`.
* `models[0]` = static world: surfaces 0..3480 (= `dpvs.staticSurfaceCount` 3481). `models[1..144]` = submodels: surfaces 3481..3740 (260 surfaces, 1,236 tris); `startSurfIndex = 0xFFFFFFFF, surfaceCount = 0` for trigger-only models.
* Submodel vertices are stored **in local space** (relative to the entity). Place them with the MapEnts entity whose `"model" "*N"` matches (127 `script_brushmodel` = window boards `pf86_auto*`, 16 `trigger_use` debris, 1 `trigger_multiple`) using its `origin` and `angles`. `nacht_brushmodels.obj` does this.

**Static models** `GfxStaticModelDrawInst` (92 B): `0x00 float cullDist | 0x04 GfxPackedPlacement placement {0x04 float origin[3]; 0x10 float axis[3][3]; 0x34 float scale} | 0x38 XModel* model | 0x3C ModelLodFade | 0x40 int cachedLightSettingIndex | 0x46 u8 reflectionProbeIndex | 0x47 u8 primaryLightIndex | 0x48 int flags | 0x4C u32 smodelCacheIndex[4]`. 1,506 instances of 70 XModels (`world_smodels.json`; e.g. `static_peleliu_shelves_ammo` at (261.9, 1089.3, 1.0)). World position = origin + scale * (x*axis[0] + y*axis[1] + z*axis[2]).

**Results** (`parse_world.py`, output in `parse_world_out.txt`): 91,002 vertices, 203,895 indices = 67,965 triangles, 3,741 surfaces, 163 distinct materials; `nacht_world.obj` = 3,481 static surfaces / 66,729 tris; bbox of referenced vertices equals GfxWorld mins/maxs exactly. Spawn sanity (`raycast_spawn.py`): a vertical ray at (-37, 202) hits `wc/berlin_floors_rock_tile2` at z = 1.0 and `wc/berlin_ceilings_plaster2` at z = 129.0 (player start z = 57 sits inside the start room). Surface → material → colorMap mapping: `world_materials.txt` / `world_surfaces.json` (e.g. `wc/berlin_trim_metal → berlin_trim_metal_c`, `*11n_10n → peleliu_wall_concrete_smooth_dirty_c` + layer-1 `peleliu_wall_concrete_grungy_edges_c`).

## 7. XModel (type 5) (VERIFIED)

T4 has **no XModelSurfs asset** — `XSurface`s are embedded in the XModel.

Header 228 B (TEMP):
```
0x00 char* name | 0x04 u8 numBones | 0x05 u8 numRootBones | 0x06 u8 numsurfs | 0x07 u8 lodRampType
0x08 u16* boneNames (ScriptString)  0x0C u8* parentList  0x10 i16[4]* quats  0x14 float* trans
0x18 u8* partClassification  0x1C DObjAnimMat* baseMat  0x20 XSurface* surfs  0x24 Material** materialHandles
0x28 XModelLodInfo lodInfo[4] (28 B each: 0x0 float dist; 0x4 u16 numsurfs; 0x6 u16 surfIndex; 0x8 int partBits[4]; 0x18 u8 lod, smcIndexPlusOne, smcAllocBits, unused)
0x98 XModelCollSurf_s* collSurfs  0x9C int numCollSurfs  0xA0 int contents  0xA4 XBoneInfo* boneInfo  0xA8 float radius
0xAC vec3 mins  0xB8 vec3 maxs  0xC4 u16 numLods  0xC6 i16 collLod  0xC8 u8 streamInfo  0xCC int memUsage
0xD0 u8 flags  0xD1 bool bad  0xD4 PhysPreset*  0xD8 PhysGeomList* physGeoms  0xDC PhysGeomList* collmap  0xE0 PhysConstraints*
```
Inline order (push VIRTUAL): name · boneNames alloc 2 `u16[numBones]` · parentList alloc 1 `u8[numBones-numRootBones]` · quats alloc 2 `i16[4][numBones-numRootBones]` · trans alloc 4 `float[(numBones-numRootBones)*4]` (**4** floats reserved per bone, but see 7.4) · partClassification `u8[numBones]` · baseMat alloc 4 `DObjAnimMat[numBones]` (32 B: vec4 quat, vec3 trans, float transWeight) · **surfs** alloc 4 `XSurface[numsurfs]` then per surface (below) · materialHandles alloc 4 `u32[numsurfs]` then each Material asset ptr · collSurfs (44 B; collTris 48 B each) · boneInfo alloc 4 `XBoneInfo[numBones]` (40 B) · physPreset (asset) · physGeoms / collmap (PhysGeomList 20 B → PhysGeomInfo 68 B → BrushWrapper...) · physConstraints (asset). All bone/vertex arrays are "reusable" (may be back-refs to identical data of another model; resolve with `ptr_native`, length from the counts).

### 7.1 XSurface (64 B)
`0x00 u8 tileMode | 0x01 bool deformed | 0x02 u16 vertCount | 0x04 u16 triCount | 0x06 u8 zoneHandle | 0x08 u16 baseTriIndex | 0x0A u16 baseVertIndex | 0x0C XSurfaceTri16* triIndices | 0x10 XSurfaceVertexInfo vertInfo {0x10 i16 vertCount[4]; 0x18 u16* vertsBlend} | 0x1C GfxPackedVertex* verts0 | 0x20 void* vb0 (runtime) | 0x24 u32 vertListCount | 0x28 XRigidVertList* vertList | 0x2C void* indexBuffer | 0x30 int partBits[4]`.
Order (reordered): vertInfo.vertsBlend alloc 2 `u16[vc0 + 3*vc1 + 5*vc2 + 7*vc3]` → verts0 alloc **16** `GfxPackedVertex[vertCount]` → vertList alloc 4 `XRigidVertList[vertListCount]` (12 B: u16 boneOffset, vertCount, triOffset, triCount; collisionTree* → 40 B tree + nodes (16 B, align 16) + leafs u16) → triIndices alloc **16** `u16[3][triCount]`.

### 7.2 GfxPackedVertex (32 B) — VERIFIED against OAT OBJ
`0x00 float xyz[3] | 0x0C float binormalSign | 0x10 u8 color[4] (as world) | 0x14 u32 texCoord | 0x18 PackedUnitVec normal | 0x1C PackedUnitVec tangent`.
texCoord (T4 "VU" packing): `u = half(texCoord >> 16), v = half(texCoord & 0xFFFF)` (IEEE binary16). Normal: same PackedUnitVec decode as the world.

### 7.3 Triangles, LODs, materials
* Triangle indices are **local to the surface** (0..vertCount-1); `baseVertIndex`/`baseTriIndex` are not needed for decoding.
* LOD n uses surfaces `surfs[lodInfo[n].surfIndex .. + lodInfo[n].numsurfs]`; surface i uses `materialHandles[i]`.
* Skinning: `vertsBlend` stream (u16): first `vertCount[0]` vertices with 1 bone `[bone]`, then `vertCount[1]` with 2 bones `[b0, b1, w1]`, then 3 bones `[b0,b1,w1,b2,w2]`, then 4 bones `[b0,b1,w1,b2,w2,b3,w3]`; bone value = byte offset → `bone index = value / 64` (sizeof DObjSkelMat); weight = w/65535, w0 = 1 - Σ others. Rigid models use `vertList[]` instead: each entry covers `vertCount` consecutive vertices bound 100% to bone `boneOffset/64`. (ASSUMED from OAT's converter; triangle/position data VERIFIED.)
* Example: `char_ger_honorgd_zomb_behead` (zone 3,209,144): 28 bones, 12 surfaces, 4 LODs; LOD0 = 3 surfaces / 425 verts / 668 tris, materials `mc/mtl_char_ger_honorguard_head1_1` (colorMap `char_ger_honorguard_head1_c`), `mc/mtl_char_jap_impinf_gore_blend`, `mc/mtl_char_jap_impinf_gore_limbs`. `viewmodel_usa_marine_arms`: 70 bones, 3547 verts / 5840 tris. `viewmodel_usa_ray_gun`: 5809 verts / 4994 tris (rigid vertLists).
* **VERIFIED**: `verify_all_xmodels.py` exports every LOD of every inline XModel (288 models, 887 LODs) and compares triangle position sets with OAT's OBJ dump: 887/887 identical (OAT writes Y-up `(x, z, -y)`).
* 9 XModels are `,`-references (e.g. `,viewmodel_usa_colt45_pistol`, `,viewmodel_hands_no_model`, `,viewmodel_usa_kbar_knife`, `,viewmodel_hands_cloth`) and are not in this zone: they live in **`zone/english/common.ff`** (VERIFIED: `py parse_xmodel.py viewmodel_usa_colt45_pistol 0 <...>/common.ff` → 7 bones, 5 surfaces, 3,449 verts / 3,062 tris, material `mc/mtl_weapon_colt45` (colorMap `colt45_c`); triangle set identical to OAT's dump of common.ff). Your engine must therefore load `common.ff` (and probably `code_post_gfx.ff`, `ui.ff`) before the map zone, and resolve `,name` references by name across loaded zones.

### 7.4 Bones (VERIFIED with `check_bones.py`)
* bone names: `script_strings[boneNames[i]]`; root bones are the first `numRootBones`.
* parent of bone i (i ≥ numRootBones) = `i - parentList[i - numRootBones]` (relative!).
* local rotation = `quats[i-numRootBones]` as int16 x,y,z,w / 32767; local translation = `trans[3*(i-numRootBones) .. +3]` — **stride 3 floats** (2703/2769 bones reproduce baseMat exactly with stride 3 vs 156 with stride 4); the stream merely reserves 4 floats per bone.
* `baseMat[i]` = global bind pose (quat xyzw — normalize, trans). global(child) = global(parent) ∘ local(child) holds (quat error < 2e-5).

## 8. Sounds (VERIFIED)

`snd_alias_list_t` (type 9, 12 B): `0x0 char* aliasName | 0x4 snd_alias_t* head | 0x8 int count`. Order: aliasName · head alloc 4 `snd_alias_t[count]` (184 B each) then per alias its members.
`snd_alias_t` (184 B): `0x00 char* aliasName | 0x04 int aliasNameHash | 0x08 char* subtitle | 0x0C char* secondaryAliasName | 0x10 char* chainAliasName | 0x14 SoundFile* soundFile | 0x18 u32 sequence | 0x1C float volMin | 0x20 volMax | 0x24 pitchMin | 0x28 pitchMax | 0x2C distMin | 0x30 distMax | 0x34 distReverbMax | 0x38 slavePercentage | 0x3C probability | 0x40 lfePercentage | 0x44 centerPercentage | 0x48 envelopMin | 0x4C envelopMax | 0x50 envelopPercentage | 0x54 minPriority | 0x58 maxPriority | 0x5C minPriorityThreshold | 0x60 maxPriorityThreshold | 0x64 reverbSend | 0x70 occlusionLevel | 0x74 occlusionWetDry | 0x78 u32 moveTime | 0x7C startDelay | 0x80 speakerMap | 0x84 int flags | 0x88..0x94 falloff curve ids | 0x98 limitType | 0xA0 entityLimitCount | 0xA8 randomizeType` (rest gaps).
Per alias order: aliasName, subtitle, secondaryAliasName, chainAliasName strings → `soundFile` (reusable; -1 → alloc 4, `SoundFile` 20 B: `0x0 u8 type (1 LOADED, 2 STREAMED, 3 PRIMED) | 0x1 u8 exists | 0x4 union u`):
* type 1: `u.loadSnd` = LoadedSound asset ptr (type 10; 12 B: `0x0 char* name | 0x4 char* data | 0x8 int data_size`; order: name, data alloc 1 `u8[data_size]`). **data is a complete RIFF file**: `WAVE` (codec 2 = MS-ADPCM ×1056, codec 1 = PCM ×21) or `XWMA` (codec 0x161 WMA2 ×129). 1,206 of them are byte-identical to OAT's dumped .wav/.xwma; 43 are `,`-refs with no data.
* type 2/3: `u.streamSnd` = `StreamedSound` (16 B embedded): `StreamFileName filename {0x0 u32 hash; 0x4 char* dir; 0x8 char* name}` + `0xC PrimedSound* primeSnd` (12 B: name, u8* buffer in **LARGE** block alloc 2048, u32 size). File path in IWDs: `sound/` + (`dir/` if dir non-empty) + `name`, case-insensitive, `\` → `/` (e.g. `SFX\Weapon\MG` + `hmg_overheat.wav`).
* Counts: 1,720 lists, 5,105 aliases (4,913 loaded, 190 streamed, 2 primed); 1,249 distinct LoadedSounds. `sounds.json` has everything.

### 8.1 clipMap_t (type 11): the map's collision (VERIFIED, `crates/waw_assets/src/t4/clipmap.rs`)

Layout in `T4_LAYOUTS.txt` (`clipMap_t` 332 B, `cbrush_t` 80, `cbrushside_t` 12, `cplane_s` 20, `cLeaf_t` 44, `cLeafBrushNode_s` 20, `CollisionPartition` 20, `CollisionAabbTree` 32, `cmodel_t` 72, `dmaterial_t` 72). Findings on Nacht:
* **Brushes** are an axial box (`mins`/`maxs`, with `axialMaterialNum[min/max][xyz]`) plus `numsides` extra sides. `sides` points into the `brushsides` array (resolve the back-reference to a file position, index = (pos - array) / 12); each side's `plane` points into `planes`. Polygonise by clipping each plane's quad by all the others. 2,715 brushes, 0–20 extra sides.
* `contents` per brush (`CONTENTS_*` as in CoD4: SOLID 0x1, GLASS 0x10, CLIPSHOT 0x2000, MISSILECLIP 0x80, PLAYERCLIP 0x10000, MONSTERCLIP 0x20000, AI_NOSIGHT 0x1000, MANTLE 0x1000000, DETAIL 0x8000000...). Nacht: 1,920 solid, 574 player clip without SOLID (541 in the static world; `clip`, `clip_player`, `clip_nosight_*`, `clip_metal`), 135 monster-clip-only (`clip_ai`, 122 in the world), 9 sky, 17 shot/missile clip, 60 non-colliding (`traverse`, `portal`, `mount`).
* **Brush models**: `cmodels[n].leaf.leafBrushNode` roots a `cLeafBrushNode_s` tree (`leafBrushCount > 0`: leaf with `brushes[]`; `0`: split with `childOffset[2]` relative to the node). Submodel brushes are in the entity's local space (window boards are one 4-unit-thick brush each). World = brushes no submodel owns: 2,549.
* **Terrain/patches**: `verts` + `triIndices`, grouped in `partitions` (`firstTri`, `triCount`) under `aabbTrees` (leaf when `childCount == 0`, `u` = partition index; `materialIndex` → `dmaterial_t.contentFlags`). World terrain = the trees under every `cLeaf_t`'s `firstCollAabbIndex/collAabbCount` (model 0's own leaf has none): 19,482 of 19,500 triangles.
* Against the render geometry: ~97% of the opaque static render area lies on solid brushes or terrain, ~98% on any collision; the rest is unreachable (floors under clip brushes), cloth and trims. Stairs are solid step brushes (Nacht's help-room stairs: 9-unit treads, 6-unit risers); there is no player-clip ramp over them, only a 10-unit clip wedge along one side.
* Tools: `examples/clipinfo.rs` (summary, brushes/entities in a box, `brush N` faces), `examples/clipcover.rs` (render vs collision coverage).

## 9. Walking all assets sequentially — algorithm (VERIFIED)

```
z = zlib.decompress(ff[12:]); blockSize = u32[7] @ 8
pos = 36; off[7] = 0; stack = []; tempSaved = []
segments[blk] = [(blockOff, filePos, size)]   # for normal blocks, to resolve native back-refs
structAt[(blk,off)] = struct; slotValue[(blk,off)] = value   # for lookup / alias back-refs
read(n): if stack empty: raw; b = top; bo = off[b]; off[b] += n
         if b is RUNTIME: return zeros (pos unchanged) else take z[pos:pos+n], pos += n, record segment (normal blocks)
alloc(a): off[top] = roundup(off[top], a)          # never touches pos
push(b)/pop(): TEMP offset saved/restored
insert_ptr(): off[4] = roundup(off[4],4); slot=(4,off[4]); off[4]+=4
then: XAssetList as in §4, and for each asset: generic asset-pointer load (§3.7) + its type's Load_* (T4_LOAD_SPEC.txt)
```
Gotchas found while making this work (all handled in `zonewalk.py`):
* The "current struct" variables of a loader are **shared state** — recursive structures (e.g. `pathnode_tree_t`) clobber them exactly like the game does; nested *asset* loads (Material inside XModel) use a fresh loader state.
* Some structs are **variable-size, read in one go** (`MaterialTechnique` = 8 + 20·passCount; `XAnimPartTrans` = 4 + (size>0 ? 28 + (size+1)·(numframes<256 ? 1 : 2) : 12); `XAnimDeltaPartQuat` = 4 + (size>0 ? 4 + (size+1)·(1|2) : 4)).
* `GfxImageLoadDef` header is read as 16 bytes (not sizeof = 20), then `resourceSize` data bytes.
* Embedded fixed pointer arrays (e.g. `MaterialTechniqueSet.techniques[59]`, `WeaponDef` model arrays) are walked element by element in index order.
* Strings and many arrays are back-references; never assume a pointer field is -1.
* After the walk: file pos = 87,241,669 = EOF; VIRTUAL = 0x49ceeb1, LARGE = 0x41c00 (exact header values); TEMP high-water 0x200354 vs header 0x200364 (16-byte slack). 0 unresolved references. Runtime ≈ 2 s in CPython.
* Generality: the same walker passes on all 128 zones of the install (`verify_all_zones.txt`), so the per-type rules are complete for WaW PC, including types absent from Nacht (menus, fonts, string tables, impact fx, snd driver globals, MP game worlds).

## 10. Open items / not resolved

* **RUNTIME block total** (`0xd42f0` emulated vs `0xc8410` in header): only affects runtime-only reservations (never file bytes, never referenced by back-refs here). Varying the element sizes of `raw_byte16`/`raw_uint128` (16/1, 128/16) did not give an exact match (`probe_runtime_sizes.py`); probably alignment/size of `GfxSceneDynModel4`, `raw_*` typedefs. Irrelevant for parsing — a Rust walker can ignore runtime blocks entirely (just don't read file bytes for them).
* Skin-weight decoding (§7.3) and the material state bits/techniques semantics are taken from OAT, not independently checked (positions, triangles, UVs, materials, bones are checked).
* Image pixel decoding (IWI format from IWDs) not covered here.

## 11. Rust porting recommendations

1. Port the stream emulator (§3/§9) first; it is ~150 lines.
2. Implement loaders per struct from `T4_LOAD_SPEC.txt` + `T4_LAYOUTS.txt`. A data-driven approach works well: describe each struct as `(size, [member: offset, kind])` where kind ∈ {string, asset(T), inline-array(T, align, count-expr, reusable: native|lookup), embedded(T), runtime-array, ...} and interpret it — that is exactly what the 2,400-line spec encodes. You only need the 20 structs listed in §4 for this zone (+ their substructs).
3. Keep `(block, offset) → file pos` segments for VIRTUAL so you can resolve shared vertex/index/string data, and `(block, offset) → asset` for alias slots.
4. Validate with the numbers in this file: EOF exactly at 87,241,669; VIRTUAL end 0x49ceeb1; 5,649 assets in `assets_walk.txt` order; GfxWorld header at 41,801,463; vertices at 52,067,078.
