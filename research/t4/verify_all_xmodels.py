# For every fully-inline XModel in the zone: export each LOD with parse_xmodel and compare triangles with OAT's OBJ dump.
import os, io, contextlib
from t4assets import walk
from parse_xmodel import xmodel_info, surfaces, export_obj
import compare_obj_lib as C
W = walk()
tot = ok = 0; fails = []
for a in W.assets:
    if a[0] != 'xmodel' or a[1].startswith(','): continue
    info = xmodel_info(W, a[2]); surfs = surfaces(W, a[2])
    for lod in range(info['numLods']):
        oat = 'oat/dump/model_export/%s_lod%d.obj' % (info['name'], lod)
        if not os.path.exists(oat): continue
        tmp = 'tmp_verify.obj'
        export_obj(info, surfs, lod, tmp)
        tot += 1
        m, na, nb = C.match(tmp, oat)
        if m == na == nb: ok += 1
        else: fails.append((info['name'], lod, m, na, nb))
print('xmodel LODs compared with OAT:', tot, 'identical triangle sets:', ok)
print('failures:', fails[:10])
os.remove('tmp_verify.obj')
