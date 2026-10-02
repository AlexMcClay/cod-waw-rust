"""
world_light.py - dump the light data of the Nacht GfxWorld / ComWorld (T4, WaW PC).

Usage: py world_light.py [zone.bin]      -> prints a summary, writes local/world_light.json
Contents: sunParse (SunLightParseParams), sunLight (GfxLight), sunColorFromBsp, ComWorld primary lights,
per-surface primaryLightIndex histogram, static-model lighting fields, GfxLightGrid header + raw arrays.
"""
import os, sys, json, struct, collections
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 't4'))
import zonewalk as zw
from zonewalk import G, unwrap, Rec, Data, BackRef, Arr


def raw(W, d, n=None):
    d = unwrap(d)
    if isinstance(d, Data) and d.fpos is not None:
        return W.z[d.fpos:d.fpos + (d.size if n is None else n)]
    if isinstance(d, Rec) and d.fpos is not None and n:
        return W.z[d.fpos:d.fpos + n]
    if isinstance(d, Arr) and d and d[0].fpos is not None and n:
        return W.z[d[0].fpos:d[0].fpos + n]
    if isinstance(d, BackRef) and n:
        fp = W.S.file_pos(d.blk, d.off)
        if fp is not None: return W.z[fp:fp + n]
    return None


def light_dict(W, L):
    L = unwrap(L)
    if not isinstance(L, Rec): return repr(L)
    d = {k: G(L, k) for k in ('type', 'canUseShadowMap', 'cullDist', 'color', 'dir', 'origin', 'radius',
                              'cosHalfFovOuter', 'cosHalfFovInner', 'exponent', 'spotShadowIndex')}
    df = unwrap(G(L, 'def'))
    d['def'] = W.cstr(G(df, 'name')) if isinstance(df, Rec) else repr(df)
    return d


def load(path=None):
    W = zw.Walker(zw.load_zone(path or os.path.join(HERE, '..', 't4', 'zone.bin'))); W.walk()
    gw = [a for a in W.assets if a[0] == 'gfxworld'][0][2]
    cw = [a for a in W.assets if a[0] == 'comworld'][0][2]
    return W, gw, cw


def main():
    W, gw, cw = load(sys.argv[1] if len(sys.argv) > 1 else None)
    out = {}
    sp = {k: G(gw, 'sunParse', k) for k in ('ambientScale', 'ambientColor', 'diffuseFraction', 'sunLight', 'sunColor',
                                            'diffuseColor', 'diffuseColorHasBeenSet', 'angles',
                                            'treeScatterIntensity', 'treeScatterAmount')}
    nm = raw(W, G(gw, 'sunParse'), 64)
    sp['name'] = nm.split(b'\0')[0].decode('latin-1') if nm else None
    out['sunParse'] = sp
    out['sunLight'] = light_dict(W, G(gw, 'sunLight'))
    out['sunColorFromBsp'] = G(gw, 'sunColorFromBsp')
    out['sunPrimaryLightIndex'] = G(gw, 'sunPrimaryLightIndex')
    out['primaryLightCount'] = G(gw, 'primaryLightCount')
    out['reflectionProbeCount'] = G(gw, 'reflectionProbeCount')
    out['lightmapCount'] = G(gw, 'lightmapCount')
    # ComWorld primary lights (72 B each)
    n = G(cw, 'primaryLightCount')
    pl = unwrap(G(cw, 'primaryLights'))
    lights = []
    if isinstance(pl, (Arr, list)):
        for i, L in enumerate(pl[:n]):
            d = {k: G(L, k) for k in ('type', 'canUseShadowMap', 'exponent', 'priority', 'cullDist', 'color', 'dir',
                                      'origin', 'radius', 'cosHalfFovOuter', 'cosHalfFovInner', 'cosHalfFovExpanded',
                                      'rotationLimit', 'translationLimit')}
            d['defName'] = W.cstr(G(L, 'defName'))
            d['index'] = i
            lights.append(d)
    out['primaryLights'] = lights
    # surfaces
    surfs = unwrap(G(gw, 'dpvs', 'surfaces'))
    h = collections.Counter(); hl = collections.Counter(); hp = collections.Counter()
    for s in surfs:
        h[G(s, 'primaryLightIndex')] += 1; hl[G(s, 'lightmapIndex')] += 1; hp[G(s, 'reflectionProbeIndex')] += 1
    out['surface_primaryLightIndex_hist'] = dict(sorted(h.items()))
    out['surface_lightmapIndex_hist'] = dict(sorted(hl.items()))
    out['surface_reflectionProbeIndex_hist'] = dict(sorted(hp.items()))
    # static models
    di = unwrap(G(gw, 'dpvs', 'smodelDrawInsts')); si = unwrap(G(gw, 'dpvs', 'smodelInsts'))
    hs = collections.Counter(G(d, 'primaryLightIndex') for d in di)
    out['smodel_primaryLightIndex_hist'] = dict(sorted(hs.items()))
    out['smodel_groundLighting_sample'] = []
    sib = raw(W, si) if isinstance(si, Data) else None
    for k in range(0, len(di), max(1, len(di) // 12)):
        b = sib[28 * k:28 * k + 28] if sib else raw(W, si[k], 28)
        gl = b[24:28] if b else None
        out['smodel_groundLighting_sample'].append({'index': k, 'model': W.cstr(G(unwrap(G(di[k], 'model')), 'name')),
                                                    'origin': G(di[k], 'placement', 'origin'),
                                                    'groundLighting_bytes': list(gl) if gl else None,
                                                    'cachedLightSettingIndex': G(di[k], 'cachedLightSettingIndex')})
    # light grid header
    lg = {k: G(gw, 'lightGrid', k) for k in ('hasLightRegions', 'sunPrimaryLightIndex', 'mins', 'maxs', 'rowAxis',
                                            'colAxis', 'rawRowDataSize', 'entryCount', 'colorCount')}
    out['lightGrid'] = lg
    # probes
    rp = unwrap(G(gw, 'reflectionProbes'))
    out['reflectionProbes'] = [G(p, 'origin') for p in rp] if isinstance(rp, (Arr, list)) else repr(rp)
    os.makedirs(os.path.join(HERE, 'local'), exist_ok=True)
    json.dump(out, open(os.path.join(HERE, 'local', 'world_light.json'), 'w'), indent=1)
    print(json.dumps({k: v for k, v in out.items() if k not in ('primaryLights',)}, indent=1)[:6000])
    for L in lights:
        print(L)


if __name__ == '__main__':
    main()
