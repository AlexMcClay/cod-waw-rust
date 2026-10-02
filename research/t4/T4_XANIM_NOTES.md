# T4 (CoD: World at War PC) XAnimParts — keyframe decoding and playback notes

Companion to `T4_ZONE_NOTES.md`; it uses the zone walker from that document (`zonewalk.py`).
**VERIFIED** means the claim was checked against the user's install with the scripts in this folder.
**ASSUMED** means the claim comes from reference code or is plausible but was not independently proven.
Reference used as format documentation: OpenAssetTools (GPL-3.0) `src/ObjWriting/XAnim/FlatXAnimReader.cpp`,
`XAnimDumper.cpp.template`, `CompiledXAnimWriter.cpp` and `T4_Assets.h` (in `gen/OAT`).

## 0. Status summary

| Item | Status | Evidence |
|---|---|---|
| Stream decoding (all part types, all 7 data streams) | **VERIFIED** | Every stream is consumed **exactly** (used == count) for all **364** xanims in Nacht and **10,342** xanims in all 128 zones of `zone/english` (4,553 distinct names). That includes 521 anims with numframes ≥ 256 and 502 anims that use the 16-bit pooled `indices[]` path. There were 0 mismatches and 0 exceptions, every key track is sorted, the last key equals numframes, and the dataShort checkpoint values match the index pool (`verify_all_xanims.py` → `verify_all_xanims.txt`) |
| Value-level decode | **VERIFIED** | For all 364 Nacht anims, `compare_oat_xanim.py` parses OAT's independently dumped raw xanims (`oat/dump/xanim/*`, binary v17) and compares them with `decode_xanim.py`. Results: 411,416 keys, identical frame indices, identical int16 quat components (max diff 0), trans max diff 3e-5, identical bone order and names, and identical notetracks |
| Quaternion encoding (int16/32767, half = (0,0,z,w)) | **VERIFIED** | Raw \|q\| = 32767.0 ± 0.6 over 319,585 keys. HALF_QUAT_NO_SIZE constants reproduce bind rotations to 0.0° (e.g. `j_thumb_le_2/3`, `j_elbow_bulge_le`) |
| Trans encoding `mins + size * u8/u16` | **VERIFIED** | The per-axis raw max is 255 / 65535 in most tracks. Feet touch the ground (z ≈ 0.3) |
| Rotations are absolute local (replace bind); NO_QUAT = identity | **VERIFIED** (empirical) | 3,470 *_NO_SIZE bone tracks equal the bind local rotation (< 0.5°, typically 0.0°); the other 4,350 are posed (e.g. clenched fingers). 533 of 558 NO_QUAT bones have an identity bind rotation (the exception, `tag_sync`, has no geometry). Rendered poses are correct |
| Translations are offsets added to the bind local translation | **VERIFIED** (empirical) | With this rule the feet rest on z = 0 and limb lengths are preserved. With "absolute" translations the pelvis would sit 9 units above the ground and knees would collapse |
| Delta part = root motion (entity space, yaw-only) | **VERIFIED** | During walk/sprint the stance foot stays fixed in world space (to ±0.3 u). On `traverse_v2` (15° yaw), rotating the pose by the delta yaw keeps the planted foot's y within 0.3 u. Without the rotation it drifts by 4.5 u |
| Notify time = fraction of the anim (0..1) | **VERIFIED** | `footstep_*` notes land on foot-plant frames (`frame = time * numframes`). The values match OAT's frame numbers |
| Loop semantics (frame numframes == frame 0) | **VERIFIED** | In all 20 looped Nacht anims, frame N equals frame 0 (max 3e-6°) |
| Viewmodel: gun root `j_gun` attached to arms `tag_weapon`; `tag_torso` driven only by ads_up/ads_down | **VERIFIED** (visual + data) | Renders show the gun held in both hands, hip pose at lower right, and ADS aligned with the sights. No idle/fire/reload anim contains `tag_torso` |
| Additive anims (assetType 6), composition `q_base * q_add`, `t += t_add` | ASSUMED (supported) | The rotation is near identity. Comparing base + additive with the `_delta` full anim gives 2.6–11° error for `q_base*q_add` and 40–74° for the reverse order |
| Interpolation between keys = (n)lerp | ASSUMED | The standard IW engine behaviour. Visually smooth |
| WeaponDef.szXAnims index meaning | **VERIFIED** (names) | OAT `WEAP_ANIM_*` enum and field names. An automated keyword check over all 44 Nacht weapons (idle/fire/reload/raise/drop/firstRaise/rechamber/adsUp/adsDown) found only semantically expected exceptions: grenade fire = `*_throw`, and zombie_melee reload = idle |

## 1. Files

| File | What |
|---|---|
| `decode_xanim.py` | **Decoder.** `decode(W, rec)` → bones, rotation/translation keys, delta (root motion), notifies, consumption counts. `sample_bone()` / `sample_keys()` interpolate. `py decode_xanim.py [zone] [anim]` |
| `xanim_pose.py` | Skeleton build (merge head model / attach gun), posing, root motion, linear-blend skinning (decodes `vertsBlend` / rigid `vertList`), OBJ writer |
| `render_xanim.py` | Software-rendered PNGs and OBJs. `py render_xanim.py zombie <anim>`, `grid`, and `viewmodel <gunmodel> <prefix>` |
| `render_additive.py` | Additive idles layered on their base poses → `xanim_out/zombie_additive.png` |
| `test_walk.py`, `test_delta.py` | Numeric foot-plant / root-motion / notetrack checks |
| `cmp_bind.py`, `noquat_stats.py`, `vm_bones.py` | Anim vs bind-pose comparisons and bone listings |
| `verify_all_xanims.py` → `verify_all_xanims.txt` | Consumption check over every zone |
| `compare_oat_xanim.py` | Value-level cross-check against OAT's raw xanim dump |
| `list_xanims.py` → `xanim_list_nacht.{txt,json}`, `xanim_list_common.{txt,json}` | Every anim with numframes, fps, length, loop, delta, assetType, bone count, root-motion displacement and notetracks |
| `weapon_anims.py` → `weapon_anims_nacht.{txt,json}` | `WeaponDef.szXAnims[35]` for all 44 Nacht weapons, plus gun/hand models |
| `xanim_out/` | `ai_zombie_walk_v1_frames.png`, `zombie_anims.png`, `zombie_additive.png`, `viewmodel_kar98_poses.png`, `viewmodel_mp40_poses.png`, and skinned OBJs (`*_fNNN.obj`, raw CoD units, Z up) |

## 2. XAnimParts asset (type 4) — header and zone layout (VERIFIED by walk)

Header, 88 bytes, read into TEMP:
```
0x00 char* name
0x04 u16 dataByteCount        0x06 u16 dataShortCount     0x08 u16 dataIntCount
0x0A u16 randomDataByteCount  0x0C u16 randomDataIntCount 0x0E u16 numframes
0x10 u8  bLoop                0x11 u8  bDelta             0x12 u8 boneCount[10]  (PART_TYPE_COUNT; [9] = ALL = total bones)
0x1C u8  notifyCount          0x1D u8  assetType          0x1E u8 isDefault
0x20 u32 randomDataShortCount 0x24 u32 indexCount         0x28 f32 framerate     0x2C f32 frequency (= framerate/numframes)
0x30 u16* names   0x34 u8* dataByte   0x38 i16* dataShort   0x3C i32* dataInt   0x40 i16* randomDataShort
0x44 u8* randomDataByte   0x48 i32* randomDataInt   0x4C indices (u8* if numframes < 256 else u16*)
0x50 XAnimNotifyInfo* notify   0x54 XAnimDeltaPart* deltaPart
```
The pointees follow inline with VIRTUAL pushed, in this order. None of them are back-references in practice, and all are plain arrays:
1. `name` string
2. `names`: alloc 2, `u16[boneCount[9]]` script-string indices
3. `notify`: alloc 4, `XAnimNotifyInfo[notifyCount]` (8 B: `u16 name` (script string) + 2 pad, `f32 time`)
4. `deltaPart`: alloc 4, `XAnimDeltaPart` (8 B: `trans*`, `quat*`), then:
   * `trans` (if non-null): alloc 4, then a **variable-size** `XAnimPartTrans`, read as one blob:
     `u16 size; u8 smallTrans; pad` +
     size == 0: `vec3 frame0` (12 B)
     size > 0: `vec3 mins; vec3 size; u32 framesPtr` (28 B) + inline indices `(size+1) × (u8 if numframes<256 else u16)`.
     After that comes the frames array: small → alloc 1, `u8[3][size+1]`; otherwise alloc 4, `u16[3][size+1]`.
   * `quat` (if non-null): alloc 4, then a variable-size `XAnimDeltaPartQuat`:
     `u16 size; pad` +
     size == 0: `XQuat2 frame0` (2 × i16)
     size > 0: `u32 framesPtr` + inline indices `(size+1) × (u8|u16)`, then frames alloc 4, `XQuat2[size+1]` (2 × i16 each).
5. `dataByte` alloc 1 · `dataShort` alloc 2 · `dataInt` alloc 4 · `randomDataShort` alloc 2 · `randomDataByte` alloc 1 · `randomDataInt` alloc 4 (**always 0 in WaW**) · `indices` (alloc 1 `u8[indexCount]` if numframes < 256, else alloc 2 `u16[indexCount]`; in practice only the u16 form is ever non-empty)

## 3. Decoding the bone tracks (VERIFIED: exact consumption on 10,342 anims)

Treat the six arrays as independent FIFO streams. `B = names[]` gives the bone order (bone i = `script_strings[names[i]]`). The bones are **sorted by quat part type**: `boneCount[0]` NO_QUAT bones first, then HALF_QUAT, FULL_QUAT, HALF_QUAT_NO_SIZE and FULL_QUAT_NO_SIZE (`Σ boneCount[0..4] = boneCount[9]`). Translation tracks are stored separately, grouped by trans type, and each entry names its bone (`Σ boneCount[5..8] = boneCount[9]`).

```
byteIdx = numframes < 256

INDICES(n):                         # key frame numbers, strictly increasing, first 0, last numframes
    if byteIdx:            return n × u8  from dataByte
    if n - 1 >= 64:        idx = n × u16 from indices[]                       # pooled
                           skip ((n-2)//256 + 2) × i16 from dataShort          # checkpoint table: idx[0], idx[256], idx[512], ..., idx[n-1]
                           return idx                                         #   (used by the game's binary search; VERIFIED equal)
    else:                  return n × u16 from dataShort

# ---- 1. rotations, bones 0..N-1 in names[] order
for each NO_QUAT bone:            nothing                                            -> rotation = identity
for each HALF_QUAT bone:          n = u16(dataShort)+1;  idx = INDICES(n);  n × (z,w)      i16 from randomDataShort
for each FULL_QUAT bone:          n = u16(dataShort)+1;  idx = INDICES(n);  n × (x,y,z,w)  i16 from randomDataShort
for each HALF_QUAT_NO_SIZE bone:  1 × (z,w)     i16 from dataShort           (constant)
for each FULL_QUAT_NO_SIZE bone:  1 × (x,y,z,w) i16 from dataShort           (constant)

# ---- 2. translations (each entry starts with its bone index into names[])
for each SMALL_TRANS entry (boneCount[5]):  b = u8(dataByte); n = u16(dataShort)+1; mins = 3×f32(dataInt); size = 3×f32(dataInt)
                                            idx = INDICES(n); n × u8[3]  from randomDataByte
for each TRANS entry (boneCount[6]):        same, but n × u16[3] from randomDataShort
for each TRANS_NO_SIZE entry (boneCount[7]):b = u8(dataByte); 3 × f32 from dataInt (constant)
for each NO_TRANS entry (boneCount[8]):     b = u8(dataByte)                   -> translation offset 0
```
Ordering within a stream is exactly the order of the steps above. For example, a FULL_QUAT bone's size short comes before its INDICES shorts, which come before the next bone's size. Every bone gets exactly one trans entry (verified).

Part-type usage in Nacht (25,379 bone tracks): FULL_QUAT 14,746, FULL_QUAT_NO_SIZE 8,535, NO_QUAT 1,724, HALF_QUAT_NO_SIZE 262, HALF_QUAT 112. Trans: TRANS_NO_SIZE 16,402, NO_TRANS 6,037, SMALL_TRANS 2,465, TRANS 475.

## 4. Value encodings (VERIFIED)

* **Quaternions** are stored as `int16 / 32767` in **x, y, z, w** order. The stored vectors already have length 32767 (measured 32767.0 ± 0.6), so normalising is optional. **Half quats** store `(z, w)`, which means a rotation about the local Z axis: `q = (0, 0, z, w) / 32767`. Consecutive keys may flip sign; use shortest-path (dot < 0 → negate) when interpolating.
* **Translations**: `value[c] = mins[c] + size[c] * raw[c]`, where raw is u8 (SMALL_TRANS) or u16 (TRANS). In the zone, `size` is already the per-step size, i.e. range/255 or range/65535 (OAT's raw writer divides by `0.0039215689` / `1.5259022e-5`). TRANS_NO_SIZE holds the float vector directly.
* **Frame indices** are absolute frame numbers. Keys exist only where the exporter kept them, so interpolate linearly between neighbouring keys. Every multi-key track starts at frame 0 and ends at frame `numframes` (verified on 17,798 tracks).
* **Delta trans**: the same encoding as bone trans (`smallTrans` flag selects u8/u16). size == 0 means the constant `frame0`.
* **Delta quat**: `XQuat2 (z, w)` int16, a yaw-only rotation about world Z. size == 0 means the constant `frame0`.
* **Notify**: `{u16 scriptString name; f32 time}`, where time is normalised 0..1 (`frame = time * numframes`). The linker appends a synthetic `("end", 1.0)` to every anim.

## 5. Timeline and looping (VERIFIED)

* Frames run 0..`numframes` **inclusive** (numframes + 1 samples). Duration = `numframes / framerate` seconds, and `frequency = framerate / numframes` = 1/duration (e.g. 30/58 = 0.5172 for `ai_zombie_idle_v1`).
* `numframes = 0` means a single static pose (most weapon `*_idle` anims are 0 or 1 frame).
* **Looped** anims (`bLoop = 1`): frame `numframes` equals frame 0, so the cycle length is `numframes / framerate` and wrapping is seamless. Non-looped anims hold their last frame.
* Sampling at normalised time t ∈ [0,1]: `f = t * numframes`. Find the keys `k0 ≤ f ≤ k1`, then nlerp the quats and lerp the translations. Before the first key or after the last key, clamp (this never happens because the first key is 0 and the last is numframes).
* 16-bit indices are used iff `numframes ≥ 256`, independently for each track.

## 6. Applying an anim to a skeleton (VERIFIED empirically, see §10)

Bones are matched **by name** (anim `names[]` ↔ XModel `boneNames`). The anim may contain bones the model lacks (ignore them), and the model may have bones the anim lacks (keep them at bind).

```
for every skeleton bone k:
    if bone in anim:
        localRot[k]   = animQuat(f)                  # ABSOLUTE local rotation (replaces bind); NO_QUAT -> identity (0,0,0,1)
        localTrans[k] = bindLocalTrans[k] + animTrans(f)   # anim trans is an OFFSET; NO_TRANS -> + 0
    else:
        localRot[k], localTrans[k] = bind local (XModel quats / trans)
    global[k] = global[parent] ∘ (localRot, localTrans)      # q_g = q_p * q_l ; t_g = t_p + rotate(q_p, t_l)
root bone (tag_origin / tag_view) = identity = entity space
skinning: v' = Σ w_i · (G_anim[b_i] · G_bind[b_i]^-1) · v,  with G_bind = XModel.baseMat of the model that owns the vertex
```
Evidence:
* `FULL/HALF_QUAT_NO_SIZE` constants equal the bind local rotation (< 0.5°, typically exactly 0.0°) for 3,470 bone tracks (e.g. thumbs, helmet, `tag_stowed_back`). That only happens if anim rotations are absolute local rotations. 533 of 558 NO_QUAT bones (zombie + viewmodel anims) have an identity bind rotation, so the exporter emits NO_QUAT for identity tracks. The one exception is `tag_sync` (bind 180°, no geometry).
* Translation offsets: for example the zombie `j_elbow_bulge_le` anim trans is 0.002 u while its bind is 11.8 u, and `j_mainroot` is 9.3 vs 37.4. With "bind + offset" the planted feet sit at z = 0.28–0.4 in every walk/sprint frame (`test_walk.py`).
* **Merged models** (zombie body + head): the head model `char_ger_honorgd_zombiehead1_1` has root `j_spine4` and contains `j_neck`, `j_head`, `j_head_end` and `j_helmet`, which the body also has. Share the existing bones by name and hang new head bones (`j_jaw`, `j_brow_*`, …) off them. The head's `baseMat`/vertices are in **its own space**, where `j_spine4` is the identity. Skin each model with its own `baseMat` inverse; do not compare its baseMat with the body's.
* **Attached models** (viewmodel gun): the gun XModel root `j_gun` becomes a child of the arms' `tag_weapon` with an identity local transform. Every viewmodel anim lists `j_gun` as NO_QUAT/NO_TRANS, so that local stays identity. The anims also animate gun bones (`j_bolt`, `j_clip`, `j_round`, `j_stripper_clip`, `tag_brass`, …) by name.

### 6.1 Root motion (delta part) (VERIFIED)

Present iff `bDelta` (VERIFIED for all 364; 49 Nacht anims, all AI). At frame f: `deltaQuat(f)` is a yaw about Z and `deltaTrans(f)` is a translation. Both are **cumulative from the anim start** (key 0 ≈ (0,0,0) / identity) and are expressed in the entity's orientation at anim start:
```
entityPos(f) = startPos + R(startYaw) · deltaTrans(f)
entityYaw(f) = startYaw + yaw(deltaQuat(f))         # yaw = 2*atan2(z, w)
world(bone) = entityPos(f) + R(entityYaw(f)) · global[bone]
```
The anim pose itself contains no root motion: tag_origin is not animated and `j_mainroot` stays over the origin. For looped anims, accumulate the end-of-cycle displacement each cycle (frame numframes holds the full-cycle displacement). Delta trans can be constant (`size 0`), small/u8 or full/u16. Delta quat is often absent (37 of 49), which means no turning. Traverse anims carry +Z root motion (window climb: `traverse_v1` ends at z = +10.9, `traverse_v2` at +18.6), followed by a `gravity on` notetrack.

### 6.2 assetType (ASSUMED names, values VERIFIED)

The observed values are **1** (all 313 viewmodel anims, no delta), **2** (all AI anims with a delta part) and **6** (`ai_zombie_idle_v1`, `ai_zombie_idle_crawl`: **additive**; near-identity rotations, translations ~0.01–0.3, no delta). Additive layers are applied to a base pose (`ai_zombie_idle_base` / `ai_zombie_idle_crawl_base`) as `localRot = baseRot * addRot` (post-multiply in bone-local space) and `localTrans += addTrans`. That order matches the full `_delta` versions 5–10× better than the reverse order (ASSUMED otherwise; the exact reference pose of additives is not proven). A Bevy port can also simply play the non-additive `ai_zombie_idle_v1_delta` / `ai_zombie_idle_crawl_delta`, which are full anims.

## 7. Viewmodels (VERIFIED visually, `xanim_out/viewmodel_*_poses.png`)

* The skeleton is the arms model `viewmodel_usa_marine_arms` (70 bones, root `tag_view`, children `tag_torso` → `j_shoulder_*` / `tag_weapon`, and `tag_cambone` → `tag_camera`). The WeaponDef's `gunXModel[0]` (e.g. `viewmodel_ger_kar98_rifle`, root `j_gun`) is attached at `tag_weapon`. `WeaponDef.handXModel` is `,viewmodel_hands_no_model` (or `,viewmodel_hands_cloth` for grenades/flamethrower/panzerschreck), so the actual arms come from the player (script `SetViewModel`, ASSUMED). Nacht contains only `viewmodel_usa_marine_arms` (+ `viewmodel_usa_marine_player`).
* Space: the `tag_view` origin is the **eye/camera**, with +X forward, +Y left and +Z up. Render the viewmodel in camera space with that origin.
* **`tag_torso` is animated only by `*_ads_up` / `*_ads_down`** (1-bone anims). Idle/fire/reload/raise anims do not contain it. Hip position = `ads_up` frame 0 (e.g. kar98: trans (2.59, −2.83, −3.46), i.e. forward, right, down). Full ADS = `ads_up` last frame (kar98: (3.54, −0.32, −0.19), which puts the sights on the view axis). `ads_down` is the reverse path. Play the weapon state anim and override `tag_torso` from the ADS anim at time = ADS fraction (`ads_up` while zooming in, `ads_down` while zooming out). This ADS-fraction playback is ASSUMED engine behaviour, but the data supports it exactly (ads_up end == ads_down start, ads_up start == ads_down end).
* Gun-part bones are animated by name: bolt (`j_bolt`), magazine (`j_clip`, moved away during reload), stripper clip (`j_stripper_clip`/`_rounds`, parked out of view when unused). `WeaponDef.hideTags` (u16 script strings) lists tags to hide.
* `viewmodel_zombie_*` is the zombie_melee "weapon" (fists): gun model `,viewmodel_usa_colt45_pistol`, with `tag_torso` moved to (−4.6, 0, −1.4).

### 7.1 WeaponDef.szXAnims[35] (offset 0x50) index meaning (VERIFIED by name match, OAT `WEAP_ANIM_*`)

| idx | name | idx | name | idx | name |
|---|---|---|---|---|---|
| 0 | root (unused, null) | 12 | reloadEnd | 24 | sprintOut |
| 1 | idle | 13 | raise (`*_pullout`) | 25 | deploy (bipod) |
| 2 | emptyIdle | 14 | firstRaise | 26 | breakdown (bipod) |
| 3 | fire | 15 | drop (`*_putaway`) | 27 | detonate |
| 4 | holdFire (grenade `pullpin`) | 16 | altRaise | 28 | nightVisionWear |
| 5 | lastShot | 17 | altDrop | 29 | nightVisionRemove |
| 6 | rechamber (bolt actions) | 18 | quickRaise (`*_pullout_fast`) | 30 | adsFire |
| 7 | melee (`viewmodel_knife_slash`, common.ff) | 19 | quickDrop (`*_putaway_fast`) | 31 | adsLastShot |
| 8 | meleeCharge (`viewmodel_knife_stick`) | 20 | emptyRaise | 32 | adsRechamber |
| 9 | reload | 21 | emptyDrop | 33 | adsUp |
| 10 | reloadEmpty | 22 | sprintIn | 34 | adsDown |
| 11 | reloadStart (shotgun/clip loaders) | 23 | sprintLoop | | |

Anim names are case-insensitive (`viewmodel_kar98_ADS_up` refers to asset `viewmodel_kar98_ads_up`). Note that `zombie_melee` maps adsUp→`viewmodel_zombie_ADS_down` and adsDown→`..._ADS_up` in the data. 81 references point to anims that are not in Nacht: `viewmodel_knife_slash/stick` (melee for 25 weapons), `viewmodel_mk2_idle` and all `viewmodel_colt45_*`. **All of them are in `common.ff`** (load common.ff first, as for models). Full tables: `weapon_anims_nacht.txt` / `.json`.

Main weapons (gun model → anims; all in Nacht unless noted). Frame counts and lengths are in `xanim_list_nacht.txt`:
* kar98k → `viewmodel_ger_kar98_rifle`: idle(1f) fire(10) lastshot(10) rechamber(31) reload(79) pullout(15) first_raise(30) putaway(10) pullout_fast(3) putaway_fast(4) fire_ads(10) ads_up(9) ads_down(16)
* mp40 → `viewmodel_ger_mp40_smg`: idle(0) idle_empty(1) fire(6) lastshot(6) reload(78) reload_empty(100) pullout(14) first_raise(50) putaway(10) …_fast(4/4) pullout_empty(15) putaway_empty(10) ads_up_pc(9) ads_down_pc(11)
* thompson, stg44 (`mp44`), bar, m1garand, m1carbine, gewehr43 (`g43_noscope`), springfield, kar98k_scoped_zombie (`kar98scoped`), doublebarrel(+_sawed_grip), shotgun (`trenchgun`: reload_start/loop/end + rechamber), fg42/mg42/30cal bipod (deploy/breakdown), ptrs41, panzerschrek, m2_flamethrower, sw_357, walther (`walther_p38`), ray_gun (idle 1, fire 12, reload 119, raise 30, drop 20, quick_raise/drop 10, ads_up 8, ads_down 9), grenades (`mk2_throw/pullpin`, `germangrenade_*`, `russiangrenade_*`, `molotov_*`, `livegrenade_tossback`), zombie_colt (common.ff `viewmodel_colt45_*`: idle 0, fire 5, reload 70, reload_notempty 62, pullout 13, putaway 11).

## 8. Zombie / AI anims in Nacht

These are all bDelta anims at 30 fps on the 108-bone humanoid rig (`char_ger_honorgd_body*` + `zombiehead*`; `ch_dazed_*` use 107 bones). "root" is the total root-motion displacement (x fwd, y left, z up) over the anim, and "u/s" is the horizontal speed. The appearance column was checked in the renders (`xanim_out/zombie_anims.png`, `ai_zombie_walk_v1_frames.png`).

| anim | frames | sec | loop | type | root Δ | u/s | notetracks | looks like |
|---|---|---|---|---|---|---|---|---|
| `ai_zombie_walk_v1` | 124 | 4.13 | Y | 2 | (156, 1, 0) | 38 | amb_vocals, step_zombie, footstep_right/left_large | shambling walk, arms swing out (verified frame by frame) |
| `ai_zombie_walk_v2` | 94 | 3.13 | Y | 2 | (122, 0, 0) | 39 | footsteps | walk |
| `ai_zombie_walk_v3` | 107 | 3.57 | Y | 2 | (134, 5, 0) | 37 | footsteps | walk |
| `ai_zombie_walk_v4` | 86 | 2.87 | Y | 2 | (137, −4, 0) | 48 | footsteps | walk |
| `ai_zombie_walk_fast_v1..v3` | 82/74/68 | 2.7/2.5/2.3 | Y | 2 | (177,−23,0)/(199,−1,0)/(178,−8,0) | 65/81/78 | footsteps | fast walk ("run" speed tier) |
| `ai_zombie_sprint_v1`, `_v2` | 64 / 59 | 2.13 / 1.97 | Y | 2 | (303,1,0) / (274,3,0) | 142 / 139 | footstep_*_large at 6/27/46 and 15/36/58 (= foot plants) | sprint, leaning forward |
| `ai_zombie_attack_v1` | 54 | 1.80 | | 2 | none | – | attack_vocals, attack_whoosh, **fire** (hit frame) | standing two-arm swipe |
| `ai_zombie_attack_v2` | 215 | 7.17 | | 2 | (2, 2, 0) | – | multiple fire | long attack combo |
| `ai_zombie_attack_forward_v1`, `_v2` | 70 / 100 | 2.3 / 3.3 | | 2 | (24,0,0) / (63,1,0) | | fire | lunging attack |
| `ai_zombie_door_tear_v1` | 276 | 9.20 | | 2 | (−19, −8, 0) | | remove_boards, **board_one … board_five** | tearing boards off the window (board notetrack = remove a board) |
| `ai_zombie_door_tear_v2` | 353 | 11.77 | | 2 | (26, −4, 0) | | board_one … board_five | board tearing, long version |
| `ai_zombie_door_tear_high/left/right/low` | 78/72/91/68 | 2.3–3.0 | | 2 | 0 | | remove_boards, **board** | single-board pull at a given board position |
| `ai_zombie_door_pound_v1`, `_v2` | 179 / 201 | 6.0 / 6.7 | | 2 | ~0 | | – | pounding on the window |
| `ai_zombie_traverse_v1` | 58 | 1.93 | | 2 | (65, 1, **11**) | | footsteps, **gravity on**, blend | climb/vault through the window (rises, then falls under gravity) |
| `ai_zombie_traverse_v2` | 44 | 1.47 | | 2 | (70, −6, **19**), yaw +15° | | gravity on, blend | faster vault |
| `ai_zombie_traverse_crawl_v1` | 100 | 3.33 | | 2 | (69, 16, 0) | | bodyfall large, gravity on | crawler goes through the window |
| `ai_zombie_death_v1`, `_v2` | 60 / 68 | 2.0 / 2.27 | | 2 | (56,10,0) yaw −10.5° / (55,0,0) | | **start_ragdoll** | falls forward onto the ground |
| `ai_zombie_crawl_death_v1`, `_v2` | 41 / 36 | 1.4 / 1.2 | | 2 | (10,0,0)/(4,−5,0) | | – | crawler death |
| `ai_zombie_shot_arm_left/right` | 59 / 58 | ~1.95 | | 2 | (58,0,0)/(72,0,0) | | – | pain reactions (arm shot off) |
| `ai_zombie_shot_leg_left/right_2_crawl` | 53 / 122 | 1.8 / 4.1 | | 2 | (67,14,0)/(67,0,0) | | – | legs shot → transition to crawler |
| `ai_zombie_crawl`, `_crawl_v1`, `_crawl_sprint` | 166 / 122 / 69 | 5.5/4.1/2.3 | Y | 2 | (91,1,0)/(56,−2,0)/(91,2,0) | 16/14/40 | crawl_hands, footsteps | crawler locomotion |
| `ai_zombie_attack_crawl`, `_lunge` | 79 / 43 | 2.6 / 1.4 | | 2 | 0 / none | | fire, board | crawler attack |
| `ai_zombie_idle_base` | 1 | 0.03 | | 2 | none | | – | base idle pose (for the additive) |
| `ai_zombie_idle_v1` | 58 | 1.93 | Y | **6** | none | | – | additive idle sway (layer on idle_base) |
| `ai_zombie_idle_v1_delta` | 59 | 1.97 | Y | 2 | none | | amb_vocals | full idle (use this instead of base + additive) |
| `ai_zombie_idle_crawl_base`, `_crawl`, `_crawl_delta` | 81 / 80 / 81 | 2.7 | –/Y/Y | 2/**6**/2 | 0 | | – | crawler idle (base / additive / full) |
| `ch_dazed_a..d` | 200/134/112/152 | 6.7/4.5/3.7/5.1 | Y | 2 | (173,−10,0) … | 26–39 | footstep_*_small | "dazed" stumbling walks (107-bone rig) |
| `ch_dazed_a..d_death` | 64/80/133/135 | 2.1–4.5 | | 2 | … (b: yaw −183°) | | – | dazed deaths |

Notetrack conventions (from the data): `sndnt#<alias>` plays a sound alias, `footstep_left/right_large|small` marks foot plants (verified), `fire` is the melee hit frame, `board`/`board_one…five` triggers board removal, `start_ragdoll` starts ragdoll, `gravity on` re-enables gravity after a traverse, `blend` is where to start blending out, and `end` (synthetic, time 1.0) marks the end.

## 9. Coordinate conventions

* CoD units are inches with **Z up, X forward, Y left** (right-handed). Model/bone space and the root motion all use the same convention. For the viewmodel, `tag_view` = eye, looking along +X.
* Quaternions are (x, y, z, w) in that basis. Composition: `q_global = q_parent * q_local` (Hamilton product, column-vector rotation `v' = q v q*`), the same convention as XModel `baseMat` (`T4_ZONE_NOTES.md` §7.4).
* Bevy (Y up, −Z forward, X right) is a proper rotation of the CoD basis: `bevy = (−y, z, −x)` for positions. Quaternion vector parts map the same way: `(qx,qy,qz,qw) → (−qy, qz, −qx, qw)`. OAT's OBJ uses `(x, z, −y)` (also a proper rotation), which keeps +X forward. Either works if positions, rotations and root motion all use the same mapping.

## 10. Verification results (numbers)

* `py decode_xanim.py` (Nacht): `anims 364, bones 25379, keys 411416, stream mismatches 0, unsorted tracks 0, key frame > numframes 0, with delta 49, checkpoints ok True`.
* `py verify_all_xanims.py` (all 128 zones in zone/english): `TOTAL anims 10342, unique 4553, nf>=256 521, pooled16 502, checkpoints_ok True`, with no `fail`/`mismatch` entries. common.ff alone holds 1,612 anims (all OK).
* `py compare_oat_xanim.py`: `anims compared 364, with differences 0, keys compared 411416, max |quat component diff| 0, max trans diff 3.14e-05`.
* `py test_walk.py ai_zombie_walk_v1`: planted feet stay fixed in world space (left ball x = 2.90/2.90/2.81 for frames 0–10, 46.6±0.1 for 30–50, 103.5±0.2 for 70–90, 158.5 for 115–120). Ball height while planted is 0.28–0.40.
* `py test_delta.py ai_zombie_sprint_v1`: `footstep_right_large` at frames 6/27/46 ↔ right ball z = 0.3–1.0 at 8/28/48. `footstep_left_large` at 15/36/58 ↔ left ball planted at 16–20/36–40/60–64.
* `py test_delta.py ai_zombie_traverse_v2`: planted right foot (frames 20–32, on the ledge z ≈ 12.8) y = −8.2/−8.3/−8.0/−8.0 with yaw applied. Without yaw it is −6.4 → −3.7.
* Pictures:
  * `xanim_out/ai_zombie_walk_v1_frames.png`: 8 frames, side and front, mesh plus bones.
  * `xanim_out/zombie_anims.png`: sprint, attack, door tear, traverse, death and crawl with root motion.
  * `xanim_out/zombie_additive.png`.
  * `xanim_out/viewmodel_kar98_poses.png` and `viewmodel_mp40_poses.png`: eye view, side and top views of hip idle, fire, rechamber/reload_empty, reload, pullout and ADS.
* OBJs: `xanim_out/ai_zombie_walk_v1_f000.obj`, `_f071.obj`, `viewmodel_kar98_*_fNNN.obj`, `viewmodel_mp40_*`.

## 11. Rust/Bevy implementation checklist

1. Parse the header and arrays (§2). Copy the six streams plus `indices` into Vecs.
2. Decode with §3 into `Vec<BoneTrack{name, rot: Keys<Quat>, trans: Keys<Vec3>}>` and assert that every stream is fully consumed. That assertion is your regression test (10,342/10,342 pass).
3. Optionally resample to dense per-frame arrays (numframes + 1 samples) for Bevy `AnimationClip` (`Keyframes::Rotation/Translation` with times `frame/framerate`). Translation keys must be `bindLocalTrans + offset`. Bones not in the clip keep their bind transform. NO_QUAT bones must get an explicit identity rotation curve, because Bevy would otherwise keep the bind rotation.
4. Root motion: keep the delta part out of the skeleton and drive the entity transform from it (§6.1), or bake it into the root for cutscene-like playback.
5. Viewmodel: build one skeleton from the arms plus the gun (`j_gun` → `tag_weapon`). Play the state anim and override `tag_torso` from ads_up/ads_down at the ADS fraction.
6. Load `common.ff` first, because the knife melee, colt and mk2_idle anims live there.

## 12. Open items

* Exact engine blend semantics for additive anims (assetType 6) and for multi-anim blending weights (only base * add was tested).
* The `assetType` enum names (1 = viewmodel/"relative"?, 2 = "delta", 6 = "additive") are inferred from usage, not from code.
* How the game chooses the arms model (`SetViewModel` in script) was not traced; GSC rawfiles were not decoded here.
* `tag_sync` (NO_QUAT although its bind rotation is 180°) is an animation-sync tag without geometry. Identity is used.
