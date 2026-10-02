"""
parse_world.py - extract GfxWorld render geometry from nazi_zombie_prototype zone.

Outputs (in this folder):
  nacht_world.obj / .mtl     static world surfaces (GfxWorld.models[0]) grouped by material
  nacht_brushmodels.obj      brush submodels (models[1..]) placed at their MapEnts origin/angles
  world_surfaces.json        per surface: index range, vertex range, material name, color map image
  world_materials.txt        material -> colorMap image (+ all texture slots)
  world_brushmodels.json     GfxBrushModel[] (models): bounds + surface range
  world_smodels.json         static model instances: xmodel name + origin + axis + scale
Coordinates are written in raw game units (Z up), as stored.
"""
import struct, json, os, math, sys, re
import zonewalk as zw
from zonewalk import G, unwrap
from t4assets import walk, material_info, color_map, data_bytes, unpack_unit_vec

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    W = walk(sys.argv[1] if len(sys.argv) > 1 else None)
    ent = [a for a in W.assets if a[0] == 'gfxworld'][0]
    gw = ent[2]
    print('GfxWorld %s header @ zone offset %d (asset spans %d..%d)' % (W.cstr(G(gw, 'name')), gw.fpos, ent[3], ent[4]))
    vcount = G(gw, 'vertexCount'); icount = G(gw, 'indexCount'); scount = G(gw, 'surfaceCount')
    vd = unwrap(G(gw, 'vd', 'vertices')); idx = unwrap(G(gw, 'indices'))
    print('vertexCount %d (vertices @ %d, %d bytes = %d x 44)' % (vcount, vd.fpos, vd.size, vd.size // 44))
    print('indexCount %d (indices @ %d, %d bytes)' % (icount, idx.fpos, idx.size))
    vb = W.z[vd.fpos:vd.fpos + vd.size]
    ib = struct.unpack('<%dH' % icount, W.z[idx.fpos:idx.fpos + 2 * icount])
    verts = []
    for i in range(vcount):
        o = i * 44
        x, y, z, bsign = struct.unpack_from('<4f', vb, o)
        col = struct.unpack_from('<I', vb, o + 16)[0]
        u, v, lu, lv = struct.unpack_from('<4f', vb, o + 20)
        nrm, tan = struct.unpack_from('<II', vb, o + 36)
        verts.append((x, y, z, u, v, lu, lv, unpack_unit_vec(nrm), col))
    surfs = unwrap(G(gw, 'dpvs', 'surfaces'))
    assert len(surfs) == scount
    mats = {}
    surf_out = []
    bad = 0
    for si, s in enumerate(surfs):
        fv, vc, tc, bi = G(s, 'tris', 'firstVertex'), G(s, 'tris', 'vertexCount'), G(s, 'tris', 'triCount'), G(s, 'tris', 'baseIndex')
        m = unwrap(G(s, 'material'))
        key = id(m)
        if key not in mats: mats[key] = material_info(W, m)
        mi = mats[key]
        tris = []
        for t in range(tc):
            a, b, c = ib[bi + 3 * t], ib[bi + 3 * t + 1], ib[bi + 3 * t + 2]
            if max(a, b, c) >= vc: bad += 1
            tris.append((fv + a, fv + b, fv + c))
        surf_out.append({'index': si, 'material': mi['name'], 'colorMap': color_map(mi), 'firstVertex': fv, 'vertexCount': vc,
                         'triCount': tc, 'baseIndex': bi, 'lightmapIndex': G(s, 'lightmapIndex'),
                         'reflectionProbeIndex': G(s, 'reflectionProbeIndex'), 'primaryLightIndex': G(s, 'primaryLightIndex'),
                         'flags': G(s, 'flags'), 'bounds': G(s, 'bounds'), 'tris': tris})
    print('surfaces %d, distinct materials %d, triangles %d, out-of-range local indices %d'
          % (scount, len(mats), sum(s['triCount'] for s in surf_out), bad))
    # stats / sanity
    used = set()
    for s in surf_out:
        for t in s['tris']: used.update(t)
    xs = [verts[i][0] for i in used]; ys = [verts[i][1] for i in used]; zs = [verts[i][2] for i in used]
    print('bbox of referenced verts: (%.1f %.1f %.1f) .. (%.1f %.1f %.1f); GfxWorld mins/maxs %s %s'
          % (min(xs), min(ys), min(zs), max(xs), max(ys), max(zs), G(gw, 'mins'), G(gw, 'maxs')))
    nl = [math.sqrt(sum(c * c for c in verts[i][7])) for i in list(used)[:5000]]
    print('normal length mean %.4f min %.4f max %.4f' % (sum(nl) / len(nl), min(nl), max(nl)))
    # geometry near player start (-37,202,57)
    px, py, pz = -37, 202, 57
    below = [t for s in surf_out for t in s['tris']
             if all(abs(verts[i][0] - px) < 400 and abs(verts[i][1] - py) < 400 for i in t)]
    print('triangles within 400u (xy) of player start: %d; their z range %.1f..%.1f' % (
        len(below), min(verts[i][2] for t in below for i in t), max(verts[i][2] for t in below for i in t)))

    # ---- brush models (GfxWorld.models[]): model 0 = static world, 1.. = submodels referenced by MapEnts "model" "*N"
    mb = data_bytes(W, unwrap(G(gw, 'models')))
    models = []
    for i in range(G(gw, 'modelCount')):
        o = i * 56
        mins = struct.unpack_from('<3f', mb, o); maxs = struct.unpack_from('<3f', mb, o + 12)
        b0 = struct.unpack_from('<3f', mb, o + 24); b1 = struct.unpack_from('<3f', mb, o + 36)
        sc, ss = struct.unpack_from('<II', mb, o + 48)
        models.append({'index': i, 'writable_mins': mins, 'writable_maxs': maxs, 'bounds': [b0, b1],
                       'surfaceCount': sc, 'startSurfIndex': ss})
    # MapEnts (inline text, loaded by clipMap_t) -> entity origin/angles for "*N" brush models
    me = [a for a in W.assets if a[0] == 'mapents'][0][2]
    txt = data_bytes(W, G(me, 'entityString'), G(me, 'numEntityChars')).decode('latin-1')
    ents = [dict(re.findall(r'"([^"]*)" "([^"]*)"', b)) for b in re.findall(r'[{]([^{}]*)[}]', txt)]
    placement = {}
    for e in ents:
        if e.get('model', '').startswith('*'):
            placement[int(e['model'][1:])] = (tuple(map(float, e.get('origin', '0 0 0').split())),
                                               tuple(map(float, e.get('angles', '0 0 0').split())),
                                               e.get('classname'), e.get('targetname'))
    for m in models:
        if m['index'] in placement:
            o_, a_, c_, t_ = placement[m['index']]
            m['entity'] = {'origin': o_, 'angles': a_, 'classname': c_, 'targetname': t_}
    json.dump(models, open(os.path.join(HERE, 'world_brushmodels.json'), 'w'), indent=0)
    print('brush models %d; model[0] surfs %d..%d (dpvs.staticSurfaceCount %d); models[1..5] (start,count) %s' % (
        len(models), models[0]['startSurfIndex'], models[0]['startSurfIndex'] + models[0]['surfaceCount'],
        G(gw, 'dpvs', 'staticSurfaceCount'), [(m['startSurfIndex'], m['surfaceCount']) for m in models[1:6]]))

    def angles_to_axis(a):
        """Quake-style angles (pitch yaw roll, degrees) -> rows forward(x), left(y), up(z)"""
        p, y, r = [math.radians(x) for x in a]
        sp, cp, sy, cy, sr, cr = math.sin(p), math.cos(p), math.sin(y), math.cos(y), math.sin(r), math.cos(r)
        fwd = (cp * cy, cp * sy, -sp)
        right = (-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp)
        up = (cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp)
        return fwd, tuple(-x for x in right), up

    NL = chr(10)

    def write_obj(path, surf_list):
        vmap = {}
        out_v = []
        for s, xf in surf_list:
            for t in s['tris']:
                for i in t:
                    k = (i, id(xf))
                    if k in vmap: continue
                    vmap[k] = len(out_v) + 1
                    x, y, z = verts[i][:3]; n = verts[i][7]
                    if xf:
                        org, ax = xf
                        x, y, z = (org[j] + x * ax[0][j] + y * ax[1][j] + z * ax[2][j] for j in range(3))
                        n = tuple(n[0] * ax[0][j] + n[1] * ax[1][j] + n[2] * ax[2][j] for j in range(3))
                    out_v.append(((x, y, z), (verts[i][3], 1.0 - verts[i][4]), n))
        with open(path, 'w') as f:
            f.write('# nazi_zombie_prototype GfxWorld, raw game units (Z up)' + NL + 'mtllib nacht_world.mtl' + NL)
            for v in out_v: f.write('v %.4f %.4f %.4f' % v[0] + NL)
            for v in out_v: f.write('vt %.6f %.6f' % v[1] + NL)
            for v in out_v: f.write('vn %.4f %.4f %.4f' % v[2] + NL)
            cur = None
            for s, xf in surf_list:
                f.write('g surf%d' % s['index'] + NL)
                if s['material'] != cur:
                    f.write('usemtl %s' % s['material'] + NL); cur = s['material']
                for t in s['tris']:
                    a, b, c = (vmap[(i, id(xf))] for i in t)
                    f.write('f %d/%d/%d %d/%d/%d %d/%d/%d' % (a, a, a, b, b, b, c, c, c) + NL)
        return len(out_v)

    with open(os.path.join(HERE, 'nacht_world.mtl'), 'w') as fm:
        for mi in mats.values():
            fm.write('newmtl %s' % mi['name'] + NL)
            cm = color_map(mi)
            if cm: fm.write('map_Kd images/%s.png' % cm.lstrip(',') + NL)
            fm.write(NL)
    m0 = models[0]
    world = [(s, None) for s in surf_out[m0['startSurfIndex']:m0['startSurfIndex'] + m0['surfaceCount']]]
    nv = write_obj(os.path.join(HERE, 'nacht_world.obj'), world)
    print('nacht_world.obj: %d surfaces, %d tris, %d verts' % (len(world), sum(s['triCount'] for s, _ in world), nv))
    sub = []
    for m in models[1:]:
        if m['surfaceCount'] and m['index'] in placement:
            org, ang = placement[m['index']][:2]
            xf = (org, angles_to_axis(ang))
            sub += [(s, xf) for s in surf_out[m['startSurfIndex']:m['startSurfIndex'] + m['surfaceCount']]]
    write_obj(os.path.join(HERE, 'nacht_brushmodels.obj'), sub)
    print('nacht_brushmodels.obj: %d surfaces, %d tris (submodel verts are LOCAL; placed with MapEnts origin/angles)' % (
        len(sub), sum(s['triCount'] for s, _ in sub)))

    with open(os.path.join(HERE, 'world_surfaces.json'), 'w') as f:
        json.dump([{k: v for k, v in s.items() if k != 'tris'} for s in surf_out], f, indent=0)
    with open(os.path.join(HERE, 'world_materials.txt'), 'w') as f:
        for mi in sorted(mats.values(), key=lambda m: m['name'] or ''):
            f.write('%s  techset=%s  colorMap=%s' % (mi['name'], mi['techniqueSet'], color_map(mi)) + NL)
            for t in mi['textures']:
                im = t['image'] or {}
                f.write('    %-11s hash=%08x %s..%s image=%s inline_pixels=%s' % (
                    t['semantic'], t['nameHash'], t['nameStart'], t['nameEnd'], im.get('name'), im.get('inline_pixels')) + NL)
    # ---- static model instances
    out = []
    for r in unwrap(G(gw, 'dpvs', 'smodelDrawInsts')):
        xm = unwrap(G(r, 'model'))
        pl = G(r, 'placement')
        out.append({'model': W.cstr(G(xm, 'name')) if isinstance(xm, zw.Rec) else repr(xm),
                    'origin': G(pl, 'origin'), 'axis': G(pl, 'axis'), 'scale': G(pl, 'scale'),
                    'cullDist': G(r, 'cullDist'), 'flags': G(r, 'flags')})
    json.dump(out, open(os.path.join(HERE, 'world_smodels.json'), 'w'), indent=0)
    from collections import Counter
    print('static model instances %d, distinct xmodels %d; top: %s' % (
        len(out), len(set(o['model'] for o in out)), Counter(o['model'] for o in out).most_common(4)))
    print('example smodel:', out[0])


if __name__ == '__main__':
    main()
