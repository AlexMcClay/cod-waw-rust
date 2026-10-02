"""
extract_shaders.py - dump every MaterialTechniqueSet of a T4 (WaW PC) zone:
techniques, passes, vertex/pixel shader bytecode and shader arguments.

Usage:  py extract_shaders.py [zone.bin | path/to/x.ff] [outdir]
  default zone = ../t4/zone.bin (Nacht), default outdir = local/shaders/<zonename>

Writes (game-derived, git-ignored):
  <outdir>/vs/<name>.vso, <outdir>/ps/<name>.pso   raw D3D9 bytecode
  <outdir>/techsets.json                           techset -> techniques -> passes -> shaders + args
Relies on the zone walker in ../t4 (zonewalk.py / t4_loaders_gen.py).
"""
import os, sys, json, struct
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 't4')); sys.path.insert(0, HERE)
import zonewalk as zw
from zonewalk import G, unwrap, Rec, Data, XStr, BackRef, PtrView, Arr

# T4 enums (OAT T4_Assets.h)
TECHNIQUE_NAMES = ['depth_prepass', 'build_float_z', 'build_shadowmap_depth', 'build_shadowmap_color', 'unlit',
    'emissive', 'emissive_shadow', 'emissive_reflected', 'lit', 'lit_fade', 'lit_sun', 'lit_sun_fade',
    'lit_sun_shadow', 'lit_sun_shadow_fade', 'lit_spot', 'lit_spot_fade', 'lit_spot_shadow', 'lit_spot_shadow_fade',
    'lit_omni', 'lit_omni_fade', 'lit_omni_shadow', 'lit_omni_shadow_fade', 'lit_charred', 'lit_fade_charred',
    'lit_sun_charred', 'lit_sun_fade_charred', 'lit_sun_shadow_charred', 'lit_sun_shadow_fade_charred',
    'lit_spot_charred', 'lit_spot_fade_charred', 'lit_spot_shadow_charred', 'lit_spot_shadow_fade_charred',
    'lit_omni_charred', 'lit_omni_fade_charred', 'lit_omni_shadow_charred', 'lit_omni_shadow_fade_charred',
    'lit_instanced', 'lit_instanced_sun', 'lit_instanced_sun_shadow', 'lit_instanced_spot',
    'lit_instanced_spot_shadow', 'lit_instanced_omni', 'lit_instanced_omni_shadow', 'light_spot', 'light_omni',
    'light_spot_shadow', 'light_spot_charred', 'light_omni_charred', 'light_spot_shadow_charred',
    'fakelight_normal', 'fakelight_view', 'sunlight_preview', 'case_texture', 'wireframe_solid',
    'wireframe_shaded', 'shadowcookie_caster', 'shadowcookie_receiver', 'debug_bumpmap', 'debug_bumpmap_instanced']

CONST_SRC = ['LIGHT_POSITION', 'LIGHT_DIFFUSE', 'LIGHT_SPECULAR', 'LIGHT_SPOTDIR', 'LIGHT_SPOTFACTORS',
    'NEARPLANE_ORG', 'NEARPLANE_DX', 'NEARPLANE_DY', 'SHADOW_PARMS', 'SHADOWMAP_POLYGON_OFFSET',
    'RENDER_TARGET_SIZE', 'LIGHT_FALLOFF_PLACEMENT', 'DOF_EQUATION_VIEWMODEL_AND_FAR_BLUR', 'DOF_EQUATION_SCENE',
    'DOF_LERP_SCALE', 'DOF_LERP_BIAS', 'DOF_ROW_DELTA', 'PARTICLE_CLOUD_COLOR', 'GAMETIME', 'PIXEL_COST_FRACS',
    'PIXEL_COST_DECODE', 'FILTER_TAP_0', 'FILTER_TAP_1', 'FILTER_TAP_2', 'FILTER_TAP_3', 'FILTER_TAP_4',
    'FILTER_TAP_5', 'FILTER_TAP_6', 'FILTER_TAP_7', 'COLOR_MATRIX_R', 'COLOR_MATRIX_G', 'COLOR_MATRIX_B',
    'SHADOWMAP_SWITCH_PARTITION', 'SHADOWMAP_SCALE', 'ZNEAR', 'SUN_POSITION', 'SUN_DIFFUSE', 'SUN_SPECULAR',
    'LIGHTING_LOOKUP_SCALE', 'DEBUG_BUMPMAP', 'MATERIAL_COLOR', 'FOG', 'FOG_COLOR', 'GLOW_SETUP', 'GLOW_APPLY',
    'COLOR_BIAS', 'COLOR_TINT_BASE', 'COLOR_TINT_DELTA', 'OUTDOOR_FEATHER_PARMS', 'ENVMAP_PARMS',
    'SPOT_SHADOWMAP_PIXEL_ADJUST', 'CLIP_SPACE_LOOKUP_SCALE', 'CLIP_SPACE_LOOKUP_OFFSET', 'PARTICLE_CLOUD_MATRIX',
    'DEPTH_FROM_CLIP', 'CODE_MESH_ARG_0', 'CODE_MESH_ARG_1', 'BASE_LIGHTING_COORDS',
    'WORLD_MATRIX', 'INVERSE_WORLD_MATRIX', 'TRANSPOSE_WORLD_MATRIX', 'INVERSE_TRANSPOSE_WORLD_MATRIX',
    'VIEW_MATRIX', 'INVERSE_VIEW_MATRIX', 'TRANSPOSE_VIEW_MATRIX', 'INVERSE_TRANSPOSE_VIEW_MATRIX',
    'PROJECTION_MATRIX', 'INVERSE_PROJECTION_MATRIX', 'TRANSPOSE_PROJECTION_MATRIX',
    'INVERSE_TRANSPOSE_PROJECTION_MATRIX', 'WORLD_VIEW_MATRIX', 'INVERSE_WORLD_VIEW_MATRIX',
    'TRANSPOSE_WORLD_VIEW_MATRIX', 'INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX', 'VIEW_PROJECTION_MATRIX',
    'INVERSE_VIEW_PROJECTION_MATRIX', 'TRANSPOSE_VIEW_PROJECTION_MATRIX', 'INVERSE_TRANSPOSE_VIEW_PROJECTION_MATRIX',
    'WORLD_VIEW_PROJECTION_MATRIX', 'INVERSE_WORLD_VIEW_PROJECTION_MATRIX', 'TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX',
    'INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX', 'SHADOW_LOOKUP_MATRIX', 'INVERSE_SHADOW_LOOKUP_MATRIX',
    'TRANSPOSE_SHADOW_LOOKUP_MATRIX', 'INVERSE_TRANSPOSE_SHADOW_LOOKUP_MATRIX', 'WORLD_OUTDOOR_LOOKUP_MATRIX',
    'INVERSE_WORLD_OUTDOOR_LOOKUP_MATRIX', 'TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX',
    'INVERSE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX']

TEXTURE_SRC = ['BLACK', 'WHITE', 'IDENTITY_NORMAL_MAP', 'MODEL_LIGHTING', 'LIGHTMAP_PRIMARY', 'LIGHTMAP_SECONDARY',
    'SHADOWCOOKIE', 'SHADOWMAP_SUN', 'SHADOWMAP_SPOT', 'FEEDBACK', 'RESOLVED_POST_SUN', 'RESOLVED_SCENE',
    'POST_EFFECT_0', 'POST_EFFECT_1', 'SKY', 'LIGHT_ATTENUATION', 'DYNAMIC_SHADOWS', 'OUTDOOR', 'FLOATZ',
    'PROCESSED_FLOATZ', 'RAW_FLOATZ', 'CASE_TEXTURE', 'CINEMATIC_Y', 'CINEMATIC_CR', 'CINEMATIC_CB',
    'CINEMATIC_A', 'REFLECTION_PROBE']

ARG_TYPES = ['MATERIAL_VERTEX_CONST', 'LITERAL_VERTEX_CONST', 'MATERIAL_PIXEL_SAMPLER', 'CODE_VERTEX_CONST',
             'CODE_PIXEL_SAMPLER', 'CODE_PIXEL_CONST', 'MATERIAL_PIXEL_CONST', 'LITERAL_PIXEL_CONST']


def bytes_of(W, d, n):
    d = unwrap(d)
    if isinstance(d, Data) and d.fpos is not None:
        return W.z[d.fpos:d.fpos + n]
    if isinstance(d, BackRef):
        fp = W.S.file_pos(d.blk, d.off)
        if fp is not None: return W.z[fp:fp + n]
    return None


def shader_info(W, sh, kind):
    sh = unwrap(sh)
    if not isinstance(sh, Rec): return None
    name = W.cstr(G(sh, 'name'))
    ld = G(sh, 'prog', 'loadDef')
    size = G(ld, 'programSize')
    code = bytes_of(W, G(ld, 'program'), 4 * size)
    return {'name': name, 'size_dwords': size, 'loadForRenderer': G(ld, 'loadForRenderer'), 'code': code}


def arg_info(W, a):
    t = G(a, 'type'); dest = G(a, 'dest')
    u = G(a, 'u', 'codeSampler')
    out = {'type': ARG_TYPES[t] if t < len(ARG_TYPES) else t, 'dest': dest}
    if t in (1, 7):
        lit = bytes_of(W, G(a, 'u', 'literalConst'), 16)
        out['literal'] = list(struct.unpack('<4f', lit)) if lit else repr(unwrap(G(a, 'u', 'literalConst')))
    elif t in (3, 5):
        idx, first, rows = u & 0xFFFF, (u >> 16) & 0xFF, (u >> 24) & 0xFF
        # NB: OAT's T4 enum (CONST_SRC) is IW3's and does NOT match T4 indices; the authoritative meaning
        # is the shader's CTAB register name, added below as 'reg_name'.
        out['code_index'] = idx
        out['firstRow'] = first; out['rowCount'] = rows
    elif t == 4:
        out['code_index'] = u
    else:
        out['nameHash'] = '%08x' % u
    return out


def ctab_of(code):
    """[(name, registerSet, index, count)] from the CTAB comment block of D3D9 bytecode"""
    import d3d9_disasm
    w = struct.unpack('<%dI' % (len(code) // 4), code)
    i = 1
    while i < len(w) and (w[i] & 0xFFFF) == 0xFFFE:
        n = (w[i] >> 16) & 0x7FFF
        if w[i + 1] == 0x42415443: return d3d9_disasm.parse_ctab(list(w[i + 1:i + 1 + n]))[0]
        i += 1 + n
    return []


def dump_zone(path, outdir):
    W = zw.Walker(zw.load_zone(path)); W.walk()
    os.makedirs(os.path.join(outdir, 'vs'), exist_ok=True)
    os.makedirs(os.path.join(outdir, 'ps'), exist_ok=True)
    result = {}; seen = {}; collisions = set()
    for typ, name, rec, s0, s1 in W.assets:
        if typ != 'techniqueset' or not isinstance(rec, Rec): continue
        ts = {'worldVertFormat': G(rec, 'worldVertFormat'), 'techniques': {}}
        techs = G(rec, 'techniques')
        for ti in range(59):
            tech = unwrap(techs.get(ti))
            if not isinstance(tech, Rec): continue
            tn = W.cstr(G(tech, 'name'))
            npass = G(tech, 'passCount')
            passes = []
            parr = G(tech, 'passArray')
            for pi in range(npass):
                p = parr[pi] if isinstance(parr, Arr) else parr
                vs = shader_info(W, G(p, 'vertexShader'), 'vs')
                ps = shader_info(W, G(p, 'pixelShader'), 'ps')
                for sh, sub, ext in ((vs, 'vs', '.vso'), (ps, 'ps', '.pso')):
                    if sh and sh['code']:
                        fn = os.path.join(outdir, sub, sh['name'].replace('/', '__').replace('\\', '__') + ext)
                        prev = seen.setdefault((sub, sh['name']), sh['code'])
                        if prev != sh['code']: collisions.add((sub, sh['name']))
                        if not os.path.exists(fn):
                            open(fn, 'wb').write(sh['code'])
                n_args = G(p, 'perPrimArgCount') + G(p, 'perObjArgCount') + G(p, 'stableArgCount')
                args = []
                aa = unwrap(G(p, 'args'))
                if n_args and isinstance(aa, (Arr, list)):
                    args = [arg_info(W, aa[i]) for i in range(n_args)]
                    for a in args:   # name the destination register from the shader's constant table
                        sh = vs if a['type'] in ('MATERIAL_VERTEX_CONST', 'LITERAL_VERTEX_CONST', 'CODE_VERTEX_CONST') else ps
                        rs = 3 if 'SAMPLER' in a['type'] else 2
                        if sh and sh['code']:
                            for nm, s_, ri, rc in ctab_of(sh['code']):
                                if s_ == rs and ri <= a['dest'] < ri + rc: a['reg_name'] = nm
                passes.append({'vs': vs and vs['name'], 'ps': ps and ps['name'],
                               'perPrim': G(p, 'perPrimArgCount'), 'perObj': G(p, 'perObjArgCount'),
                               'stable': G(p, 'stableArgCount'), 'customSamplerFlags': G(p, 'customSamplerFlags'),
                               'args': args})
            ts['techniques'][ti] = {'type': TECHNIQUE_NAMES[ti], 'name': tn, 'flags': G(tech, 'flags'), 'passes': passes}
        result[name] = ts
    json.dump(result, open(os.path.join(outdir, 'techsets.json'), 'w'), indent=1)
    print('same-named shaders with different bytecode:', sorted(collisions) or 'none')
    return W, result


if __name__ == '__main__':
    zp = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, '..', 't4', 'zone.bin')
    zn = os.path.splitext(os.path.basename(zp))[0]
    if zn == 'zone': zn = 'nazi_zombie_prototype'
    od = sys.argv[2] if len(sys.argv) > 2 else os.path.join(HERE, 'local', 'shaders', zn)
    W, r = dump_zone(zp, od)
    nvs = len(os.listdir(os.path.join(od, 'vs'))); nps = len(os.listdir(os.path.join(od, 'ps')))
    print('%s: %d techsets, %d vertex shaders, %d pixel shaders -> %s' % (zn, len(r), nvs, nps, od))
