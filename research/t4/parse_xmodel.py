"""
parse_xmodel.py - dump an XModel (bones, LODs, surfaces, materials, verts, tris, weights) and export LOD0 as OBJ.

usage: py parse_xmodel.py <model name substring> [lod] [zone .ff path]   e.g.  py parse_xmodel.py char_ger_honorgd_zomb_behead
       (the Colt viewmodel lives in common.ff: py parse_xmodel.py viewmodel_usa_colt45_pistol 0 <...>/zone/english/common.ff)
"""
import struct, json, os, sys
import zonewalk as zw
from zonewalk import G, unwrap, Rec, Data, Arr
from t4assets import walk, material_info, color_map, data_bytes, unpack_unit_vec, unpack_texcoords_vu

HERE = os.path.dirname(os.path.abspath(__file__))


def xmodel_info(W, xm):
    nb, nrb, ns = G(xm, 'numBones'), G(xm, 'numRootBones'), G(xm, 'numsurfs')
    info = {'name': W.cstr(G(xm, 'name')), 'numBones': nb, 'numRootBones': nrb, 'numsurfs': ns, 'numLods': G(xm, 'numLods'),
            'radius': G(xm, 'radius'), 'mins': G(xm, 'mins'), 'maxs': G(xm, 'maxs'), 'flags': G(xm, 'flags')}
    # bones
    bn = data_bytes(W, G(xm, 'boneNames'))
    names = [W.script_strings[i] for i in struct.unpack('<%dH' % nb, bn)] if bn else []
    pl = data_bytes(W, G(xm, 'parentList'))
    parents = list(pl) if pl else []
    qb = data_bytes(W, G(xm, 'quats')); tb = data_bytes(W, G(xm, 'trans'))
    bm = data_bytes(W, G(xm, 'baseMat'))
    bones = []
    for i in range(nb):
        b = {'name': names[i] if names else None}
        if i >= nrb:
            k = i - nrb
            # parentList[k] = offset back to parent: parent = i - parentList[k]  (IW convention)
            b['parent'] = i - parents[k] if parents else None
            if qb: b['localQuat'] = tuple(c / 32767.0 for c in struct.unpack_from('<4h', qb, 8 * k))  # x,y,z,w
            if tb: b['localTrans'] = struct.unpack_from('<3f', tb, 12 * k)   # VERIFIED stride = 3 floats (buffer is allocated as 4*(n) floats)
        else:
            b['parent'] = -1
        if bm:
            q = struct.unpack_from('<4f', bm, 32 * i); t = struct.unpack_from('<3f', bm, 32 * i + 16)
            b['baseQuat'] = q; b['baseTrans'] = t
        bones.append(b)
    info['bones'] = bones
    # materials
    mh = unwrap(G(xm, 'materialHandles'))
    mats = [material_info(W, mh.get(i)) for i in range(ns)] if mh else []
    info['materials'] = [{'name': m['name'], 'colorMap': color_map(m), 'techset': m.get('techniqueSet')} for m in mats]
    info['lods'] = [{'dist': G(xm, 'lodInfo', i, 'dist') if False else G(unwrap(G(xm, 'lodInfo'))[i], 'dist'),
                     'numsurfs': G(unwrap(G(xm, 'lodInfo'))[i], 'numsurfs'),
                     'surfIndex': G(unwrap(G(xm, 'lodInfo'))[i], 'surfIndex')} for i in range(4)]
    return info


def surfaces(W, xm):
    out = []
    for s in unwrap(G(xm, 'surfs')):
        vc, tc = G(s, 'vertCount'), G(s, 'triCount')
        vb = data_bytes(W, G(s, 'verts0'), 32 * vc)
        tb = data_bytes(W, G(s, 'triIndices'), 6 * tc)
        verts = []
        for i in range(vc):
            o = 32 * i
            x, y, z, bs = struct.unpack_from('<4f', vb, o)
            col, tex, nrm, tan = struct.unpack_from('<4I', vb, o + 16)
            verts.append({'xyz': (x, y, z), 'uv': unpack_texcoords_vu(tex), 'n': unpack_unit_vec(nrm), 'color': col})
        tris = [struct.unpack_from('<3H', tb, 6 * i) for i in range(tc)]
        vi = G(s, 'vertInfo')
        vcnt = [G(vi, 'vertCount', k) for k in range(4)]
        blend = data_bytes(W, G(vi, 'vertsBlend'), 2 * (vcnt[0] + 3 * vcnt[1] + 5 * vcnt[2] + 7 * vcnt[3]))
        vl = unwrap(G(s, 'vertList'))
        rigid = [{'boneIndex': G(r, 'boneOffset') // 64, 'vertCount': G(r, 'vertCount'), 'triOffset': G(r, 'triOffset'),
                  'triCount': G(r, 'triCount')} for r in vl] if isinstance(vl, list) else []
        out.append({'vertCount': vc, 'triCount': tc, 'baseVertIndex': G(s, 'baseVertIndex'), 'baseTriIndex': G(s, 'baseTriIndex'),
                    'deformed': G(s, 'deformed'), 'tileMode': G(s, 'tileMode'), 'blendVertCounts': vcnt,
                    'vertsBlend_u16': len(blend) // 2 if blend else 0, 'rigidLists': rigid, 'verts': verts, 'tris': tris})
    return out


def export_obj(info, surfs, lod, path):
    li = info['lods'][lod]
    with open(path, 'w') as f:
        f.write('# %s lod%d (raw game units, Z up)\n' % (info['name'], lod))
        base = 1
        for k in range(li['numsurfs']):
            si = li['surfIndex'] + k
            s = surfs[si]
            f.write('o surf%d\nusemtl %s\n' % (k, info['materials'][si]['name'] if info['materials'] else 'none'))
            for v in s['verts']: f.write('v %.6g %.6g %.6g\n' % v['xyz'])
            for v in s['verts']: f.write('vt %.6g %.6g\n' % (v['uv'][0], 1 - v['uv'][1]))
            for v in s['verts']: f.write('vn %.4f %.4f %.4f\n' % v['n'])
            for t in s['tris']:
                a, b, c = (base + t[0], base + t[1], base + t[2])
                f.write('f %d/%d/%d %d/%d/%d %d/%d/%d\n' % (a, a, a, b, b, b, c, c, c))
            base += len(s['verts'])


def main():
    want = sys.argv[1] if len(sys.argv) > 1 else 'char_ger_honorgd_zomb_behead'
    lod = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    W = walk(sys.argv[3] if len(sys.argv) > 3 else None)
    cands = [a for a in W.assets if a[0] == 'xmodel' and a[1] == want] or \
            [a for a in W.assets if a[0] == 'xmodel' and want in a[1] and not a[1].startswith(',')]
    ent = cands[0]; xm = ent[2]
    info = xmodel_info(W, xm)
    surfs = surfaces(W, xm)
    print('XModel %s header @ zone offset %d (asset bytes %d..%d)' % (info['name'], xm.fpos, ent[3], ent[4]))
    print(' bones %d (root %d), surfs %d, lods %d' % (info['numBones'], info['numRootBones'], info['numsurfs'], info['numLods']))
    for i, l in enumerate(info['lods'][:info['numLods']]):
        n = l['numsurfs']; ss = surfs[l['surfIndex']:l['surfIndex'] + n]
        print('  lod%d dist %.0f surfs %d..%d verts %d tris %d' % (i, l['dist'], l['surfIndex'], l['surfIndex'] + n,
              sum(s['vertCount'] for s in ss), sum(s['triCount'] for s in ss)))
    for i, s in enumerate(surfs):
        bad = sum(1 for t in s['tris'] if max(t) >= s['vertCount'])
        print('  surf%d verts %d tris %d baseVert %d baseTri %d blendCounts %s rigidLists %d  bad-idx %d  material %s (colorMap %s)' % (
            i, s['vertCount'], s['triCount'], s['baseVertIndex'], s['baseTriIndex'], s['blendVertCounts'], len(s['rigidLists']), bad,
            info['materials'][i]['name'] if info['materials'] else None, info['materials'][i]['colorMap'] if info['materials'] else None))
    print(' first bones:', [(b['name'], b['parent']) for b in info['bones'][:8]])
    out = os.path.join(HERE, '%s_lod%d.obj' % (info['name'].lstrip(','), lod))
    export_obj(info, surfs, lod, out)
    json.dump({k: v for k, v in info.items()}, open(out[:-4] + '_info.json', 'w'), indent=1, default=str)
    print('wrote', out)


if __name__ == '__main__':
    main()
