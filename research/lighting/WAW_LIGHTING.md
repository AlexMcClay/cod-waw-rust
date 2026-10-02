# WaW (T4, PC) lighting, fog and film grade — what the shaders really do

Target: Nacht der Untoten (`nazi_zombie_prototype.ff`) plus `common.ff` / `code_post_gfx.ff`, from the user's install.
Everything below comes from the game's own compiled shaders (D3D9 SM3 bytecode pulled out of the
MaterialTechniqueSets) and its world data. **VERIFIED** = read directly from bytecode/data or checked numerically.
**ASSUMED** = inferred (usually CPU-side behaviour, which isn't visible in the shaders or the zone; the exe's
`.text` is SteamStub-encrypted, entropy 8.0, so it wasn't disassembled).

## 0. TL;DR

| Topic | Result | Status |
|---|---|---|
| Lightmap halves | `top = lmUV*(1,.5)` (A), `bottom = lmUV*(1,.5)+(0,.5)` (B). `light = A.rgb + B.rgb * Lz`, where `d = (A.a*4.08-2.08, B.a*4.0645-2.0645)` and `Lz = 0.6*exp2(-|d|²)+0.4`. **Not** `A+B`. | VERIFIED |
| Meaning | A = non-directional part, B = light from one dominant direction `L ∝ (d.x, d.y, 1)` in tangent space (d = slope `L.xy/L.z`); `Lz ≈ 1/sqrt(1+|d|²) = cos(angle)`. With a normal map: `A*Nz + B*sat(dot(L,N))`. | VERIFIED (math), meaning ASSUMED |
| `lightmap0_primary` (L8) | Baked **shadow/visibility of the surface's own primary light** (sun, spot or omni), sampled at the full `lmUV`. It is 0 on every surface with no primary light. | VERIFIED |
| Primary light | One per surface (`GfxSurface.primaryLightIndex`), added **at runtime per pixel**: `primary.x * lightColor * falloff(dist/radius) * spot * sat(N·L)`. Of the 3,741 surfaces, 1,338 have a spot or omni primary light (warm orange `1,.5,0` lanterns and white tungsten lamps), 1,066 have the sun and 1,337 have none. Their direct light is **not** in the lightmap. | VERIFIED (shader + data) |
| Overbright | World: none (×1). Models: light-grid colour **×2**. | VERIFIED |
| Gamma | Every shader does its maths on the raw texel values: there are no pow/sRGB constants in any of the 4,090 shaders and no sRGB bit in the sampler state. Lighting, fog and film all work in **gamma space**. | ASSUMED (strong) |
| Fog | In the vertex shader of every lit material. Colour is `lerp(fogColor, lit, f)` with `f = min(1, exp2(c.w - c.y*dist*c.z*H))`. H is a height term `(1-2^-a)/a`, with `a = c.x*(z - eyeZ)`. Fog is applied **before** the film grade. | VERIFIED. The constant mapping is ASSUMED. |
| Film | `postfx_color`: `L = dot(c,(.299,.587,.114))`, `out = (c*k + L)*(base + delta*L) + bias`. For Nacht that is about `lerp(c,L,0.4) * lerp((.84,.92,1.10),(2,2,2),L) + 0.0055`. | Shader VERIFIED. dvar→constant mapping ASSUMED (constrained). |
| Models | `2 * cube(N)` from a 4×4×4 "ambient cube" (56 boundary texels = `GfxLightGridColors.rgb[56]`), plus `alpha * primary-light term`. | VERIFIED (shader). Grid decode VERIFIED. |
| Light grid | 32×32×64-unit cells. Row/column run-length encoding fully decoded: all 18,105 entries are accounted for. | VERIFIED. Spacing/offset ASSUMED (consistent). |

**Why the current remake bake looks flat/blue:** it adds `A+B` (overstates B by about 1/Lz, which is about 1.5× on average) and treats the L8 page as a global sun shadow. It misses the per-surface dynamic primary lights: these supply nearly all of the warm lantern light in Nacht, and the secondary lightmap is the bluish moonlight/bounce remainder. It also misses the film grade (`lightTint 2` brightens mid-tones about 1.4×; `darkTint` pushes shadows blue) and fog.

## 1. Files and tools (all in this folder)

| File | What |
|---|---|
| `extract_shaders.py` | Walks a zone with `../t4/zonewalk.py` and dumps every techset → technique → pass → VS/PS bytecode + args. Writes `local/shaders/<zone>/{vs,ps}/*.vso/.pso` and `techsets.json`. Each arg gets `reg_name` from the shader's CTAB (see §1.1). |
| `disasm_all.py` | Disassembles every dumped shader with the Windows SDK `fxc.exe /dumpbin` into `local/shaders/<zone>/asm/*.asm`. If fxc is missing it falls back to `d3d9_disasm.py`. |
| `d3d9_disasm.py` | Minimal pure-Python SM2/SM3 disassembler (version/instruction/param tokens, def/dcl, CTAB). `--check` compares it with an fxc listing. **It gives instruction streams identical to fxc for all 4,090 shaders** of the three zones. |
| `world_light.py` | Dumps sunParse, sunLight, ComWorld primary lights, per-surface/smodel primary-light histograms, the light-grid header and the reflection probes → `local/world_light.json`. |
| `lightgrid.py` | Decodes `GfxLightGrid` (row/col/z encoding), checks it, and samples it (cube lookup, 8-corner blend) → `local/lightgrid_samples.txt`. |
| `waw_lighting_ref.py` | Numpy reference implementation of every formula below (lightmap decode, primary lights, fog, film). Use it as a porting oracle. |
| `local/` (git-ignored) | Shader bytecode + listings, `lm_secondary_bgra.npy`, `lm_primary.npy`, preview PNGs (`lm_decoded_A_plus_B_Lz_x2.png`, `lm_old_A_plus_B_x2.png`, `lm_Lz.png`, ...), `cube_axis_test.py`. |

Run order: `py extract_shaders.py` (Nacht), `py extract_shaders.py <install>/zone/english/common.ff`,
`py extract_shaders.py <install>/zone/english/code_post_gfx.ff`, `py disasm_all.py`, `py world_light.py`, `py lightgrid.py`,
`py waw_lighting_ref.py`.

### 1.1 Notes on the techset data (VERIFIED)
* Shader counts: Nacht has 340 techsets, 800 VS and 2,339 PS. common.ff has 79/228/586 and code_post_gfx.ff has 65/58/79. No two shaders share a name with different bytecode.
* All shaders are `ps_3_0`/`vs_3_0` built with "D3DX9 Shader Compiler 9.15.779". The CTAB block names every register.
* **The OAT `T4` code-constant/sampler enums are IW3's and are wrong for T4.** For example, arg index 43, which OAT calls `GLOW_SETUP`, is really `fogColor`. Arg 37 is `sunDiffuse` and 107 is `worldMatrix`. Use the CTAB name (`reg_name` in techsets.json), not the enum.
* Lightmap and probe samplers are not args. They are bound through `MaterialPass.customSamplerFlags`: bit0 = reflection probe (s1), bit1 = lightmap primary (s2), bit2 = lightmap secondary (s3). Examples: `lm_r0c0` has flags=4, `lm_sun_r0c0` has 6, `lm_r0c0n0s0_sco` has 5 and `lm_sun_r0c0n0s0_sco` has 7.
* Technique index → shader family, from the technique names stored in the zone:
  * `8 lit` → `lm_*`
  * `10 lit_sun` → `lm_sun_*`
  * `12 lit_sun_shadow` → `lm_sm_sun_*`
  * `14 lit_spot`, `16 lit_spot_shadow`
  * `18 lit_omni`, `20 lit_omni_shadow`
  * `36..42 lit_instanced*`
  * `4 unlit`, `0 depth_prepass`, ...

  Models use the same indices with `lp_*` shaders. **The main lit pass is technique 8/10/14/18**, picked per surface by the type of its primary light (ASSUMED selection rule; the names are VERIFIED).

## 2. World surfaces (techsets `wc_l_sm_*`, `l_sm_*`)

### 2.1 Vertex shader (`lm_tc0_sm3`, `lm_s_tc0n0_sm3_sco`) — VERIFIED
* `Pw = worldMatrix * pos`. **Pw is camera-relative.** The VS uses `|Pw|` as fog distance and the PS uses `normalize(Pw)` as the view vector, so `worldMatrix` contains `-eyePos` and `viewProjectionMatrix` has no translation.
* Inputs:
  * `texcoord` = float4 `(uv, lmapUV)`. `GfxWorldVertex.texCoord` and `.lmapCoord` are adjacent, so they form one float4.
  * `position.w` = `binormalSign`.
* Vertex colour is D3DCOLOR (memory bytes B,G,R,A) and is passed straight through. Lit shaders multiply albedo by `vc.rgb`.
* **Layered "_nc" shaders** (e.g. `l_sm_r0c0n0s0_sco_b1c1...`) do *not* multiply by vertex colour. They use `vc.g` (byte 1) as the layer-1 blend weight: `w1 = colorMap1.a * vc.g`, `albedo = lerp(color0*colorTint, color1*colorTint1, w1)`.
* Tangent frame:
  * `N` = normal × worldMatrix
  * `T` = tangent × worldMatrix
  * `Bn = sign(position.w) * cross(N, T)`
* The fog factor is computed per vertex into `texcoord1.w` (§4).

### 2.2 Pixel shader, no normal map (`lm_r0c0_sm3`, technique 8) — VERIFIED
```
A   = tex2D(lightmapSecondary, lmUV * float2(1, 0.5))                  // top 512x512 half
B   = tex2D(lightmapSecondary, lmUV * float2(1, 0.5) + float2(0, 0.5)) // bottom half
d   = float2(A.a, B.a) * float2(4.08, 4.0645161) + float2(-2.08, -2.0645161)
Lz  = saturate(0.6 * exp2(-dot(d, d)) + 0.4)
light = A.rgb + B.rgb * Lz
col = tex2D(colorMap, uv).rgb * vc.rgb * light
out.rgb = lerp(fogColor.rgb, col, fogFactor);  out.a = 1
```
Key instructions: `mad r2.xy, r2, c1, c1.zw` (c1 = 4.08, 4.0645, -2.08, -2.0645); `dp2add r0.w, r2, r2, 0`; `exp r0.w, -r0.w` (D3D `exp` is **2^x**); `mad_sat r0.w, r0.w, 0.6, 0.4`; `mad r1.xyz, B, r0.w, A`.
* The pair `(4.08, -2.08)` maps an 8-bit 130/255 to 0. The pair `(4.0645, -2.0645)` = `(126/31, -64/31)` maps a 6-bit DXT green of 32/63 to 0. The same decode is used for normal maps (x in alpha, y in green) **and** for the lightmap alphas. Both encode a *slope* `v.xy / v.z` in [-2.08, 2.0].
* `0.6*2^(-s²)+0.4` is a cheap fit of `1/sqrt(1+s²)` (error < 0.015), i.e. the z of the unit vector `(s, 1)`. So `B*Lz = B * dot(L, flatNormal)`.
* Row 0 of the **top** half (texel rows 0) is not lightmap. It holds the light-falloff lookup rows (§2.5). Surfaces never map there.

### 2.3 With normal + specular map (`lm_r0c0n0s0_sm3_sco`, technique 8) — VERIFIED
```
d  = slope(A.a, B.a)                       ; n = slope(normalMap.a, normalMap.g)
Lz = 0.6*exp2(-|d|²)+0.4 ;  Nz = 0.6*exp2(-|n|²)+0.4
light = A.rgb * Nz + B.rgb * saturate(Lz*Nz*(1 + dot(d, n)))        // = A*N.z + B*sat(dot(L,N))
Nw = normalize(N + n.x*T + n.y*Bn)                                   // bumped world normal
I  = normalize(Pw)  (eye -> point) ; R = I - 2*dot(I,Nw)*Nw
probe = texCUBElod(reflectionProbe, float4(R, 6 - 8*specMap.a))
fres  = envMapParms.x + envMapParms.y * exp2(envMapParms.z * |dot(I,Nw)|)
refl  = probe.rgb * probe.a * specMap.rgb * fres
albedo = lerp(colorMap*vc, scorch, scorchAmt)      // scorchAmt = 0 normally (terrain scorch decals)
out = lerp(fogColor, albedo*light + refl, fogFactor)
```
The typical material `envMapParms` is `(0.8, 3.2, -4.25, 0.625)` (205 materials; values from the OAT dump of material constants).

### 2.4 Primary light on top (techniques 10/14/18, plus shadow variants 12/16/20) — VERIFIED
`P = tex2D(lightmapPrimary, lmUV).x` (full UV, 1024² L8) multiplies the dynamic light:
* **sun** (`lm_sun_*`): `light += P * sunDiffuse.rgb * sat(dot(sunPosition.xyz, Nw))`.

  Sun specular:
  ```
  spec = exp2_sat((dot(R, sunPos) - 0.99925) * (1.442695 * exp2(9.3775 * specMap.a) + 10.0989))
  refl = (P * envMapParms.w * sunSpecular.rgb * spec + probe.rgb*probe.a) * specMap.rgb * fres
  ```
* **omni** (`lm_omni_*`):
  ```
  Ld = lightPosition.xyz - Pw
  t  = sat(|Ld| * lightPosition.w)
  falloff = tex2D(lightmapSecondary, t*lightFalloffPlacement.xy + lightFalloffPlacement.zw).rgb
  light += P * falloff * lightDiffuse.rgb * sat(dot(normalize(Ld), Nw))
  ```
* **spot** (`lm_spot_*`): the same as omni, times `s = sat(dot(normalize(Ld), lightSpotDir)*f.x + f.y)` (with `f = lightSpotFactors`), then `spot = s > 0 ? pow(s, f.z) : 0`.
* Shadow-mapped variants (`lm_sm_*`):
  * spot: a 4-tap PCF result `S`, then `shadow = lerp(P, S, lightSpotFactors.w)`. This is a `lrp r2.w, c8.w, S, P` instruction.
  * sun: uses `shadowmapSamplerSun` the same way.
* `col = albedo * light + refl` → fog. **There is no ×2 for world surfaces.**

### 2.5 Falloff curves stored in lightmap row 0 — VERIFIED
`GfxLightDef.lmapLookupStart` = column in row 0 of `lightmap0_secondary`: `light_point_linear` → 1 (16 texels, plus 1 padding texel on each side) and `tungsten_lamp` → 19 (32 texels). These are 8-bit R values (alpha 255):
```
linear   R=G=B: 255 255 244 223 202 181 159 138 117 96 74 53 32 11 0 0          (~ saturate(1 - t), ~1/12 flat start)
tungsten R: 255 235 226 218 210 204 195 187 181 175 169 156 144 133 120 106 94 85 77 68 60 53 47 39 31 25 16 9 9 0 0 0
tungsten G: 255 226 212 200 189 180 167 155 146 137 128 119 110 101 92 81 72 65 59 52 46 40 36 30 24 19 13 7 7 0 0 0
tungsten B: 255 216 198 181 166 154 137 120 108 97 85 79 73 67 61 54 48 43 39 35 31 27 24 20 16 13 9 5 5 0 0 0
```
Placement (ASSUMED): `u = (start + 0.5 + t*(width-1)) / 512`, `v = 0.5/1024`.
Models read the same curve from the light def's own attenuation image (`attenuationSampler`, `falloff_linear` / `falloff_tungsten`).

### 2.6 Evidence that the primary light is not baked into the secondary — VERIFIED
Per primary-light index, mean of the lightmap at the vertices of the surfaces (`local/` script output):

| primaryLight | surfaces' vertices | mean A | mean B | mean P (L8) | light colour |
|---|---|---|---|---|---|
| 0 (none) | 37,656 | .070 .072 .089 | .097 .104 .115 | **0.000** | – |
| 1 (sun) | 24,223 | .057 .063 .076 | .112 .157 .200 | 0.481 | .44 .57 .69 |
| 17 (orange omni) | 6,960 | .109 .099 .110 | .162 .170 .187 | 0.470 | 1 .5 0 |
| 18 (orange omni) | 6,166 | .111 .103 .121 | .137 .152 .176 | 0.416 | 1 .5 0 |
| 20 (tungsten omni) | 3,178 | .081 .074 .082 | .121 .113 .105 | 0.553 | 1 .98 .91 |

Surfaces lit by the orange lanterns have a neutral or bluish secondary. Their orange comes entirely from the dynamic term. Surfaces with no primary light have P = 0 everywhere.

## 3. GfxWorld / ComWorld light data for Nacht — VERIFIED (values)

**sunParse** (SunLightParseParams):

| Field | Value |
|---|---|
| ambientScale | 0.1 |
| ambientColor | (0.5, 0.5, 0.7) |
| diffuseFraction | 0.15 |
| sunLight | 0.75 |
| sunColor | (0.64, 0.85, 1.0) |
| diffuseColor | (0.3125, 0.6875, 1.0) (set) |
| angles | (-150, 17, 0) |
| treeScatter | 1 / 1 |

These are compile-time parameters. Ambient/diffuse are baked into the lightmaps and grid (ASSUMED).

**sunLight** (GfxLight, type 1):
* colour (0.3536, 0.4696, 0.5525) = `sunColor × 0.5525`
* dir (-0.828, -0.253, 0.500) points **towards** the moon/sun: 30° elevation. `AngleVectors(-150, 17)` gives exactly this.
* `sunColorFromBsp` = 0. `sunPrimaryLightIndex` = 1.

**Primary lights** (ComWorld, 21 entries, 72 B each). Type 1 = sun, 2 = spot, 3 = omni. For spots, `dir` = −(cone axis), i.e. it points back towards the light: downward ceiling lamps store (0,0,1). So `lightSpotDir = dir` works with `dot(normalize(lightPos − P), dir)` (ASSUMED convention; it is the same as the sun's "towards the light" dir).

| idx | type | colour | origin | radius | cos outer/inner | def |
|---|---|---|---|---|---|---|
| 0 | – | – | – | – | – | (null) |
| 1 | sun | .435 .570 .693 | – | – | – | – |
| 2 | spot | 1 .502 0 | -288 -88 64 | 500 | .866/.940 | light_point_linear |
| 3 | spot | .6 .588 .529 | -141 680 90 | 400 | .643/.766 | linear |
| 4 | spot | 1 .988 .918 | -111.5 858 160 | 500 | .707/.866 | linear |
| 5,6 | spot | 1 .988 .918 | (-1.5,1056,236), (41,739,236) | 500/400 | .643/.766, .707/.866 | linear |
| 7 | spot | 1 .984 .910 | 177.5 717.3 90 | 400 | .643/.766 | linear |
| 8,9 | spot | 1 .99 .94 | (302.5,203,232.5), (310,547,236) | 400 | .707/.866 | linear |
| 10,11 | spot | 1 .985 .91 | (341,922,95), (416,739,236) | 400 | .643/.766, .707/.866 | linear |
| 12 | spot | 1 1 1 | 674 481 133 | 600 | .707/.866 | linear |
| 13 | spot | 1 .502 0 | 712 1408 64 | 500 | .707/.866 | linear |
| 14 | spot | 1 .992 .941 | 766.8 870.8 15 | 300 | .707/.866 | linear |
| 15 | spot | 1 .502 0 | 1136 832 72 | 500 | .843/.954 | linear |
| 16 | omni | 1 .502 0 | 110 -959.8 76 | 300 | – | linear |
| 17 | omni | 1 .502 0 | 139.5 -94.3 77.3 | 450 | – | linear |
| 18 | omni | 1 .502 0 | 728 936 200 | 500 | – | linear |
| 19 | omni | 1 .980 .910 | -192 -677.2 18 | 350 | – | tungsten_lamp |
| 20 | omni | 1 .980 .910 | 208 429.8 18 | 350 | – | tungsten_lamp |

(Full precision, dir vectors, cullDist 1000, `canUseShadowMap` 1 and exponent are in `local/world_light.json`.)
* The ComWorld sun colour (.435,.570,.693) differs from `GfxWorld.sunLight` (.354,.470,.553). Use the GfxWorld one for `sunDiffuse` (ASSUMED).
* `sunSpecular` is not stored (ASSUMED ≈ sunDiffuse).

**Per-surface primary light** (3,741 surfaces): 1,337 none · 1,066 sun · 452 spot (2–15) · 886 omni (16–20) → `world_light.json`.
**Static models** (1,506): 101 none · 1,156 sun · 249 spot/omni (`GfxStaticModelDrawInst.primaryLightIndex`). `groundLighting` = 0 and `cachedLightSettingIndex` = 0 for all of them, so smodel lighting is computed at runtime from the grid (ASSUMED).
**Reflection probes**: 44 (origins in json); surfaces pick one by `reflectionProbeIndex`.

## 4. Fog (SetVolFog 165, 835, 200, 75, 0.5 grey) — shader VERIFIED

This appears in every lit world/model VS (`fogConsts` = c21). The PS uses `fogColor` (c0, rgb = 0.5).
```
Pw   = camera-relative world position ; dist = |Pw| ; dz = Pw.z
D    = dist * c.z
if (c.x > 0 && |dz| > 0.01) { a = c.x*dz + 1e-6;  D *= (1 - exp2(-a)) / a; }
f    = min(1, exp2(c.w - c.y * D))          // per vertex, interpolated
out  = lerp(fogColor.rgb, litColor, f)        // per pixel (mad (col-fog)*f + fog)
```
* The sky cubemap shader (`sky.hlsl`, used by Nacht's `wc_sky`) has **no fog**.
* Film is a later full-screen pass that samples the resolved scene (`postfx_color` reads `RESOLVED_SCENE`), so **fog → then film**. VERIFIED.
* **Constant mapping (ASSUMED)**, chosen so that "no fog before start, 50 % at halfway":
  ```
  c.x = 1/halfwayHeight
  c.y = 1/(halfwayDist - startDist)
  c.z = 2^-((eyeZ - baseHeight)/halfwayHeight)
  c.w = startDist * c.y
  ```
  For Nacht with the eye at z=60 this gives (0.005, 0.0014925, 1.053, 0.2463). An alternative reading is `c.y = 1/halfwayDist` (halfway measured from start).
* Note: `(1-2^-a)/a → ln2` as dz → 0, while the `|dz| ≤ 0.01` branch uses 1, so the raw formula jumps by ln2 there. The recommended port is `fog_factor_port` in `waw_lighting_ref.py`: it normalises the integral by `ln 2` so that H is 1 for horizontal rays. With that port, fog at eye height is 0 % at ≤165 u, 14 % at 300, 31 % at 500, 52 % at 835 and 68 % at 1200.

## 5. Film grade (`postfx_color.hlsl`, code_post_gfx) — shader VERIFIED
```
c = sceneColor.rgb (8-bit resolved scene, already fogged)
L = dot(c, float3(0.299, 0.587, 0.114))
o = (c * colorBias.w + L) * (colorTintBase.rgb + colorTintDelta.rgb * L) + colorBias.rgb ;  alpha = 1
```
The same block also appears inside `postfx_dof_color`, `glow_setup` and `vertcol_film`. `postfx` is a plain copy. Registers: colorTintBase c5, colorTintDelta c6, colorBias c7.

**dvar → constant (ASSUMED).** This is the only mapping under which the shader equals the natural model. It also makes `neutral.vision` the identity and fits `default_night.vision` (green dark tint, white light tint, desat 1) and `cheat_invert.vision`:
```
d = max(r_filmDesaturation, ε) ; k = colorBias.w = (1-d)/d
colorTintBase  = contrast * d * darkTint
colorTintDelta = contrast * d * (lightTint - darkTint)
colorBias.rgb  = brightness              (contrast pivot unknown; irrelevant for contrast = 1)
invert: negate base/delta, bias = 1 - bias
=> o = contrast * lerp(c, L, d) * lerp(darkTint, lightTint, L) + brightness
```
Nacht (`vision/zombie.vision`, read from common.ff): film on, contrast 1, brightness 0.0055, desat 0.4, lightTint (2,2,2), darkTint (0.84,0.92,1.10), invert 0, glow 0. This gives the following.

The constants are base = (.336, .368, .44), delta = (.464, .432, .36) and k = 1.5.

The resulting curve (grey in → out):

| In | Out |
|---|---|
| .05 | .050 .054 .063 |
| .10 | .101 .108 .125 |
| .20 | .220 .233 .262 |
| .30 | .362 .379 .417 |
| .50 | .716 .736 .781 |
| ≈ .60–.64 | clips to 1 (blue first) |

Dark areas get a blue lift and mid-tones are brightened strongly. The output is written to an 8-bit backbuffer, so it saturates.

## 6. Light grid and model lighting

### 6.1 GfxLightGrid encoding — VERIFIED (`lightgrid.py`)
* Header: `mins (3933,3955,2047)`, `maxs (4247,4219,2068)`, `rowAxis=1` (Y), `colAxis=0` (X), `rawRowDataSize 8484`, `entryCount 18105`, `colorCount 3526`, `hasLightRegions 1`, `sunPrimaryLightIndex 1`.
* Grid coordinates (ASSUMED spacing; it fits the world bbox and the floor/ceiling test below):
  ```
  gx = floor(x/32) + 4096 ;  gy = floor(y/32) + 4096 ;  gz = floor(z/64) + 2048
  ```
  So the cells are **32 × 32 × 64** units and the grid covers x -5216..4864, y -4512..3968, z -64..1344. For example, the start-room column (-37,202) has points at z = 64, 128 and 192. The floor is at 1 and the ceiling at 129, so no point sits inside the floor slab.
* Lookup:
  ```
  row = g[rowAxis] - mins[rowAxis];  s = rowDataStart[row]  (0xFFFF = empty row)
  p = rawRowData + 4*s:  u16 colStart, u16 colCount, u16 zStart, u16 zCount, u32 firstEntry
  then runs until colCount columns are covered:  u8 nCols, u8 nZ, (u8 zOff if nZ>0)
     each column of the run holds nZ entries for z = zStart+zOff .. +nZ-1, column-major;
     nZ = 0 -> nCols empty columns.  entry = firstEntry + Σprev(nCols*nZ) + col*nZ + (gz - zStart - zOff)
  ```
  Checks: on all 189 non-empty rows the column sums equal colCount and the entry sums equal the firstEntry deltas, and the padding is < 4 bytes. Every one of the 18,105 entries is reached exactly once and round-trips.
* `GfxLightGridEntry {u16 colorsIndex; u8 primaryLightIndex; u8 needsTrace}`. primaryLightIndex values are 0…20 or 255. It is the primary light a model standing there should use.
* `GfxLightGridColors.rgb[56][3]` = the **56 boundary cells of a 4×4×4 cube** (4³−2³ = 56). They are in order x fastest, then y, then z, skipping the 8 interior cells. This was VERIFIED statistically by ray-casting the world from 350 grid points: the "openness" in ±x/±y/±z correlates best with cube axes 0/1/2 (r = .27/.17/.49; every off-diagonal value is lower). The top layer is brighter on average.

### 6.2 Model shaders (`mc_l_sm_*` → `lp_*`) — VERIFIED
```
Nw  = normalize(worldNormal)       (bumped: normalize(N + n.x*T + n.y*Bn) as in §2.3)
uvw = Nw / max(|Nw.x|,|Nw.y|,|Nw.z|) * lightingLookupScale.xyz + baseLightingCoords.xyz
G   = tex3D(modelLighting, uvw)                  // per-model 4x4x4 block of a volume atlas
light = 2*G.rgb                                       // lp_*      (no primary light)
light = 2*G.rgb + G.a * sunDiffuse * sat(dot(sunPosition, Nw))                          // lp_sun_*
light = 2*G.rgb + G.a * atten(t).rgb * lightDiffuse * sat(dot(Ldir,Nw)) [* spot]          // lp_omni_*/lp_spot_*
col = colorMap*vc * light (+ refl as §2.3, with probe) ; fog as §4
```
* Scale: models use **×2** (`mad r0, 2, light*albedo, -fog`). World surfaces don't.
* `G.a` = visibility of the model's primary light (ASSUMED filled by the CPU). `lp_sm_sun` replaces it with a cascaded shadow-map result near the camera.
* `lightingLookupScale` (ASSUMED) = 1.5 texels / atlas size, so the lookup lands on the cube's boundary texel centres. In practice the result is a bilinear interpolation over the face that `N` points at.
* The CPU (ASSUMED) builds each model's block by blending the 56-colour sets of the 8 grid points around the model's lighting origin (trilinear; points that are missing or occluded via `needsTrace` are dropped). It applies to zombies, script models, static models (groundLighting is unused, all 0) and the viewmodel (sampled at the player's view origin).

Samples of `2*cube(N)` (`lightgrid.py`, 8-corner blend):

| Location (z = 40) | Grid primary | up | down | +x | −x |
|---|---|---|---|---|---|
| start room (-37,202) | 17 | .37 .31 .24 | .29 .25 .20 | .17 .13 .12 | .73 .61 .46 |
| help room (300,700) | 0 | .07 .06 .08 | .14 .13 .13 | .08 .07 .09 | .19 .17 .16 |
| next to orange omni 17 (139,-94) | 17 | .26 .18 .17 | .39 .21 .12 | .39 .22 .15 | .23 .14 .12 |
| next to tungsten 20 (208,430) | 20 | .28 .22 .16 | .15 .13 .12 | .25 .19 .15 | .17 .14 .13 |
| courtyard (600,1300) | 13 | .17 .09 .09 | .23 .12 .11 | .17 .09 .09 | .19 .10 .09 |

Single grid points (no blend) show the outdoor/indoor split. One point near the outdoor omni 16 (110,-960,76) is blue-ish: avg (.12,.15,.18), whose own orange primary is not in the grid. One point at the orange spot 2 is warm on +x, the side facing the lanterns, and blue on −x, the side facing outside.

The grid holds the indirect/bounce light plus the non-primary lights. The direct light of the primary is added dynamically, as on the world.

## 7. Porting recipe for the remake (Bevy)

Do all of this in **gamma space**: sample textures as `Rgba8Unorm` (not `*Srgb`), keep values 0..1. Either render to a non-sRGB target and present as-is, or convert to linear only at the very end (do the film in gamma, then `srgb_to_linear` before writing an sRGB swapchain).

1. **Bake (CPU, per lightmap texel)** — replaces `bake_lightmap`:
   `light = A.rgb + B.rgb * (0.6*exp2(-|d|²)+0.4)`, with `d = (A.a*4.08-2.08, B.a*4.0645161-2.0645161)` (A = top, B = bottom half).
   Keep `P = primary L8` as a separate channel. Do not add the sun everywhere.
2. **World fragment:**
   ```
   col = albedo * vc * (light + P * primaryTerm(surface.primaryLightIndex)) [+ refl]
   ```
   * `primaryTerm` uses the ComWorld light (§3) with the falloff (§2.5) and spot cone. For the sun it is `sunLight.color * sat(dot(sunLight.dir, N))`.
   * For index 0 it is zero.
   * Vertex colour is BGRA bytes. Layered materials use `vc.g` as the blend weight and don't tint.
3. **Props / zombies / viewmodel:**
   ```
   col = albedo * vc * (2*cube(N) + vis * primaryTerm(gridEntry.primaryLightIndex))
   ```
   * `cube` comes from the 8-corner blended grid sample at the model's origin (+ ~half height).
   * `vis` = 1, or a shadow ray.
4. **Fog** (per vertex or pixel) with the §4 constants and `fogColor 0.5`. Skip it on the sky.
5. **Film** as a full-screen pass after everything (and after fog): the §5 formula with the Nacht constants, then saturate.

## 8. Still uncertain
* The CPU mapping of the film dvars, and of the fog dvars → `fogConsts`. Both are ASSUMED; the exe is encrypted. For Nacht, contrast = 1 removes the biggest film unknown (the pivot). Fog: whether `halfway` is measured from the eye or from `start`, how `baseHeight` is clamped, and how the ln2 normalisation is handled.
* `sunDiffuse` / `sunSpecular` values. Candidates are `GfxWorld.sunLight.color` vs `ComWorld` light 1, possibly scaled by `r_lightTweak*` dvars. `lightDiffuse` is assumed = `ComPrimaryLight.color` (no intensity scale).
* `lightPosition.w = 1/radius`, the spot-factor formula, and the exact falloff texel placement.
* sRGB: there are no conversions in any shader, but the D3D sampler/render-state sRGB flags are CPU-side. Gamma space is the strong assumption. The `r_gamma` hardware ramp (a user setting) is not modelled.
* Light grid: the 32/32/64 spacing and offsets are inferred (consistent). For models, the CPU's corner weighting/`needsTrace` occlusion and how the model block's alpha (primary visibility) is computed are unknown.
* Which technique is selected at runtime (shadow-map variants vs plain): it depends on `sm_*` dvars and `canUseShadowMap`. The Nacht sun has `canUseShadowMap 0`, so sun surfaces use `lit_sun` with the baked P.
