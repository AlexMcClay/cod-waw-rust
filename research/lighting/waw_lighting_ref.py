"""
waw_lighting_ref.py - reference (numpy) implementation of WaW / T4 world + model lighting, fog and film grade,
transcribed from the disassembled shaders (see WAW_LIGHTING.md). Meant as a porting/testing oracle.

All colour math is done on the raw 0..1 texel values (gamma-encoded, no sRGB decode), as the game does.

Usage: py waw_lighting_ref.py      -> prints Nacht constants, film curve, fog table, a few model-lighting samples
"""
import math
import numpy as np

# ------------------------------------------------------------------ constants straight from the shader bytecode
DIR_SCALE = np.array([4.07999992, 4.06451607])      # def c1.xy   (x from alpha 8 bit, y from 6-bit-green / alpha)
DIR_BIAS = np.array([-2.07999992, -2.06451607])     # def c1.zw
LUMA = np.array([0.299, 0.587, 0.114])              # postfx_color def c0


def unpack_slope(xa, yb):
    """(A.a, B.a) of the two lightmap halves, or (normalMap.a, normalMap.g): 0..1 -> tangent-space slope (x/z, y/z)"""
    return np.stack([xa * DIR_SCALE[0] + DIR_BIAS[0], yb * DIR_SCALE[1] + DIR_BIAS[1]], -1)


def cos_from_slope(d):
    """0.6 * exp2(-|d|^2) + 0.4  ~= 1/sqrt(1+|d|^2) = the z component of the unit vector (d.x, d.y, 1)"""
    return np.clip(0.6 * np.exp2(-(d * d).sum(-1)) + 0.4, 0.0, 1.0)


def lightmap_flat(A, B):
    """lm_r0c0_sm3 (no normal map). A = RGBA of the TOP half (lmUV * (1, .5)), B = BOTTOM half (+ (0, .5))."""
    d = unpack_slope(A[..., 3], B[..., 3])
    return A[..., :3] + B[..., :3] * cos_from_slope(d)[..., None]


def lightmap_bumped(A, B, nmap_a, nmap_g):
    """lm_r0c0n0s0_sm3_sco: normal map stores the bumped normal as a slope in alpha (x) and green (y)."""
    d = unpack_slope(A[..., 3], B[..., 3])
    n = unpack_slope(nmap_a, nmap_g)
    nz = cos_from_slope(n)[..., None]; lz = cos_from_slope(d)[..., None]
    s = np.clip(lz * nz * (1.0 + (d * n).sum(-1)[..., None]), 0.0, 1.0)   # == saturate(dot(L_unit, N_unit))
    return A[..., :3] * nz + B[..., :3] * s


def bumped_world_normal(N, T, Bn, nmap_a, nmap_g):
    """normalize(N + n.x*T + n.y*Bn) with n = slope from the normal map (exact for slope encoding)"""
    n = unpack_slope(nmap_a, nmap_g)
    v = N + n[..., :1] * T + n[..., 1:2] * Bn
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


# ------------------------------------------------------------------ primary light (one per surface / model)
def primary_light(kind, P, N, light, falloff=None):
    """Dynamic term added on top of the lightmap/lightgrid.
    kind: 'sun' | 'omni' | 'spot'; P = world position (any space consistent with light['origin']), N = unit normal.
    light: dict with color, dir (sun: towards the sun; spot: towards the light = -forward), origin, radius,
           cosHalfFovOuter/Inner, exponent.  falloff(t)->rgb  (t = dist/radius, 0..1).  Returns rgb (no shadow)."""
    col = np.asarray(light['color'], float)
    if kind == 'sun':
        return col * max(0.0, float(np.dot(light['dir'], N)))
    L = np.asarray(light['origin'], float) - P
    dist = float(np.linalg.norm(L)); L = L / max(dist, 1e-6)
    t = min(1.0, dist / light['radius'])                                   # lightPosition.w = 1/radius   [ASSUMED]
    att = falloff(t) if falloff else np.full(3, max(0.0, 1.0 - t))
    ndl = max(0.0, float(np.dot(L, N)))
    spot = 1.0
    if kind == 'spot':                                                       # lightSpotFactors           [ASSUMED]
        co, ci = light['cosHalfFovOuter'], light['cosHalfFovInner']
        x = 1.0 / max(ci - co, 1e-4); y = -co * x
        s = min(1.0, max(0.0, float(np.dot(L, light['dir'])) * x + y))
        spot = s ** max(light.get('exponent', 1), 1e-6) if s > 0 else 0.0
    return col * att * ndl * spot


def world_pixel(albedo_rgb, vcolor_rgb, lm_rgb, primary_rgb=0.0, primary_shadow=0.0, reflection_rgb=0.0):
    """final = albedo*vc*(lightmap + shadow*primary) + reflection   (fog and film applied afterwards)"""
    return albedo_rgb * vcolor_rgb * (lm_rgb + primary_shadow * np.asarray(primary_rgb)) + reflection_rgb


def env_fresnel(envMapParms, cos_view):
    """reflection-probe weight: x + y * exp2(z * |dot(I, N)|)   (typical envMapParms 0.8, 3.2, -4.25, 0.625)"""
    x, y, z, _ = envMapParms
    return x + y * math.exp2(z * abs(cos_view))


# ------------------------------------------------------------------ fog (vertex shader, all lit world/model shaders)
def fog_factor(P_rel, fogConsts):
    """P_rel = world position minus eye position. Returns f (1 = no fog); colour = lerp(fogColor, colour, f)."""
    fx, fy, fz, fw = fogConsts
    dist = float(np.linalg.norm(P_rel))
    D = dist * fz
    dz = float(P_rel[2])
    if fx > 0 and abs(dz) > 0.01:
        a = fx * dz + 1e-6
        D *= (1.0 - math.exp2(-a)) / a
    return min(1.0, math.exp2(fw - fy * D))


def fog_factor_port(P_rel, start, halfway, halfHeight, baseHeight, eyeZ):
    """Recommended port (continuous): same shape as the shader, with the height integral normalised so that a
    horizontal ray gets H = 1 (the shader's (1-2^-a)/a tends to ln2, its |dz|<0.01 branch uses 1).  Gives exactly
    'no fog before start, 50 %% at halfway' at eye height when the eye is at baseHeight.   [ASSUMED mapping]"""
    dist = float(np.linalg.norm(P_rel)); dz = float(P_rel[2])
    k = 1.0 / halfHeight
    eye_density = math.exp2(-(eyeZ - baseHeight) * k)
    a = k * dz
    H = 1.0 if abs(a) < 1e-4 else (1.0 - math.exp2(-a)) / (a * math.log(2.0))
    D = dist * eye_density * H
    return min(1.0, math.exp2((start - D) / (halfway - start)))


def fog_consts_setvolfog(start, halfway, halfHeight, baseHeight, eyeZ, halfway_from_start=False):
    """ASSUMED CPU mapping of SetVolFog(start, halfway, halfHeight, baseHeight, ...) -> fogConsts (c21).
    Chosen so that, for c.x = 0, fog = 0 before `start` and 50 % at `halfway` (or start+halfway)."""
    y = 1.0 / (halfway if halfway_from_start else max(halfway - start, 1e-3))
    x = 1.0 / halfHeight if halfHeight > 0 else 0.0
    z = math.exp2(-(eyeZ - baseHeight) * x) if x else 1.0
    return (x, y, z, start * y)


# ------------------------------------------------------------------ film grade (postfx_color.hlsl)
def film_shader(c, colorTintBase, colorTintDelta, colorBias):
    """exact postfx_color math: L = luma(c); o = (c*bias.w + L) * (base + delta*L) + bias.rgb"""
    c = np.asarray(c, float)
    L = (c * LUMA).sum(-1, keepdims=True)
    return (c * colorBias[3] + L) * (np.asarray(colorTintBase) + np.asarray(colorTintDelta) * L) + np.asarray(colorBias[:3])


def film_consts(contrast, brightness, desat, lightTint, darkTint, invert=False):
    """ASSUMED CPU mapping (only one that makes the shader equal lerp(c,L,d)*lerp(dark,light,L)*contrast + bias;
    desaturation must be clamped > 0 for the factorisation). Contrast pivot unknown (irrelevant for contrast = 1)."""
    d = max(desat, 1.0 / 1024)
    k = (1.0 - d) / d
    base = contrast * d * np.asarray(darkTint, float)
    delta = contrast * d * (np.asarray(lightTint, float) - np.asarray(darkTint, float))
    bias = np.full(3, brightness, float)
    if invert: base, delta, bias = -base, -delta, 1.0 - bias
    return base, delta, (bias[0], bias[1], bias[2], k)


def film_effective(c, contrast, brightness, desat, lightTint, darkTint):
    c = np.asarray(c, float); L = (c * LUMA).sum(-1, keepdims=True)
    return contrast * (c + (L - c) * desat) * (np.asarray(darkTint) + (np.asarray(lightTint) - np.asarray(darkTint)) * L) + brightness


NACHT_VISION = dict(contrast=1.0, brightness=0.0055, desat=0.4, lightTint=(2, 2, 2), darkTint=(0.84, 0.92, 1.10))
NACHT_FOG = dict(start=165, halfway=835, halfHeight=200, baseHeight=75, color=(0.5, 0.5, 0.5))


def main():
    np.set_printoptions(precision=4, suppress=True)
    base, delta, bias = film_consts(**{k: NACHT_VISION[k] for k in ('contrast', 'brightness', 'desat', 'lightTint', 'darkTint')})
    print('Nacht film constants (ASSUMED mapping): colorTintBase', base, 'colorTintDelta', delta, 'colorBias', np.array(bias))
    for g in (0.02, 0.05, 0.1, 0.2, 0.3, 0.5, 0.7, 1.0):
        c = np.array([g, g, g])
        print('  grey %.2f -> %s   (warm 0.5/0.3/0.1 scaled %.2f -> %s)' % (
            g, film_shader(c, base, delta, bias), g, film_shader(np.array([0.5, 0.3, 0.1]) * g * 2, base, delta, bias)))
    assert np.allclose(film_shader(np.array([0.3, 0.2, 0.1]), base, delta, bias),
                       film_effective(np.array([0.3, 0.2, 0.1]), **NACHT_VISION), atol=1e-6)
    fc = fog_consts_setvolfog(NACHT_FOG['start'], NACHT_FOG['halfway'], NACHT_FOG['halfHeight'], NACHT_FOG['baseHeight'], eyeZ=60)
    print('Nacht fogConsts (ASSUMED mapping, eye z = 60):', np.round(fc, 5))
    print('  raw shader formula with those constants (note the ln2 jump between dz=0 and dz!=0):')
    for dist in (100, 165, 300, 500, 835, 1200, 2000):
        print('  dist %5d  dz=0 f=%.3f   dz=-50 f=%.3f   dz=+150 f=%.3f' % (
            dist, fog_factor(np.array([dist, 0, 0.0]), fc), fog_factor(np.array([dist, 0, -50.0]), fc),
            fog_factor(np.array([dist, 0, 150.0]), fc)))
    print('  recommended continuous port (fog_factor_port), eye z = 60:')
    F = NACHT_FOG
    for dist in (100, 165, 300, 500, 835, 1200, 2000):
        print('  dist %5d  dz=0 f=%.3f   dz=-50 f=%.3f   dz=+150 f=%.3f' % tuple([dist] + [
            fog_factor_port(np.array([dist, 0, dz]), F['start'], F['halfway'], F['halfHeight'], F['baseHeight'], 60)
            for dz in (0.0, -50.0, 150.0)]))


if __name__ == '__main__':
    main()
