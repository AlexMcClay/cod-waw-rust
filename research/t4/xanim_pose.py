"""
xanim_pose.py - apply decoded T4 xanims (decode_xanim.py) to XModel skeletons; skinning + OBJ/PNG export.

Posing rule used (see T4_XANIM_NOTES.md for the evidence):
  local rot  (bone in anim)      = anim quat (absolute local rotation, replaces bind local rotation);
                                    NO_QUAT -> identity
  local rot  (bone not in anim)  = bind local rotation (XModel quats)
  local trans                    = bind local trans (XModel trans) + anim trans (anim trans is an OFFSET; NO_TRANS -> 0)
  global(child) = global(parent) o local(child);  root bones of the first model = identity (entity space)
  attached model (e.g. gun):      its root bone is parented to an attach bone of the previous model
                                  (viewmodel gun: j_gun -> tag_weapon) with local = anim value (identity here)
  merged model (e.g. head):       bones whose names already exist are shared; new bones hang off their parent by name
  delta part (root motion):       entity transform at frame f = (yaw from delta quat, translation from delta trans)
"""
import math, struct, sys, os
import numpy as np
from zonewalk import G, unwrap
from t4assets import walk, data_bytes
import decode_xanim as dx
import parse_xmodel as px

HERE = os.path.dirname(os.path.abspath(__file__))


def qmul(a, b):
    ax, ay, az, aw = a; bx, by, bz, bw = b
    return (aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz)


def qrot(q, v):
    x, y, z, w = q
    r = qmul(qmul(q, (v[0], v[1], v[2], 0.0)), (-x, -y, -z, w))
    return r[:3]


def qnorm(q):
    l = math.sqrt(sum(c * c for c in q)) or 1.0
    return tuple(c / l for c in q)


def qinv(q): return (-q[0], -q[1], -q[2], q[3])


def qmat(q):
    x, y, z, w = q
    return np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                     [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                     [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])


class Skeleton:
    def __init__(s, W, models):
        """models: list of (xmodel name, attach bone name or None).  attach None + not first -> merge by bone names"""
        s.W = W
        xm = {a[1]: a[2] for a in W.assets if a[0] == 'xmodel'}
        s.names, s.parent, s.lq, s.lt, s.is_root = [], [], [], [], []
        s.models = []   # (name, info, surfaces, bone index map model->skeleton)
        idx = {}
        for mi, (mname, attach) in enumerate(models):
            info = px.xmodel_info(W, xm[mname])
            surfs = px.surfaces(W, xm[mname])
            bmap = []
            for bi, b in enumerate(info['bones']):
                n = b['name']
                if n in idx and attach is None:
                    bmap.append(idx[n]); continue
                k = len(s.names)
                idx[n] = k; bmap.append(k); s.names.append(n)
                if b['parent'] < 0:
                    if mi == 0:
                        s.parent.append(-1); s.lq.append((0, 0, 0, 1)); s.lt.append((0, 0, 0)); s.is_root.append(True)
                    else:
                        # attached root: parent = attach bone, local = identity (anim may override)
                        s.parent.append(idx[attach]); s.lq.append((0, 0, 0, 1)); s.lt.append((0, 0, 0)); s.is_root.append(True)
                else:
                    s.parent.append(bmap[b['parent']])
                    s.lq.append(qnorm(b['localQuat'])); s.lt.append(tuple(b['localTrans'])); s.is_root.append(False)
            s.models.append((mname, info, surfs, bmap))
        s.index = idx
        s.bind = s.compose(s.lq, s.lt)
        # inverse bind per model bone from that model's baseMat (vertices are in each model's own bind space)
        s.inv_bind = []
        for mname, info, surfs, bmap in s.models:
            inv = []
            for b in info['bones']:
                q = qnorm(b['baseQuat']); t = b['baseTrans']
                inv.append((q, t))
            s.inv_bind.append(inv)

    def compose(s, lq, lt):
        gq, gt = [None] * len(s.names), [None] * len(s.names)
        for i in range(len(s.names)):
            p = s.parent[i]
            if p < 0:
                gq[i], gt[i] = lq[i], lt[i]
            else:
                gq[i] = qnorm(qmul(gq[p], lq[i]))
                r = qrot(gq[p], lt[i]); gt[i] = (gt[p][0] + r[0], gt[p][1] + r[1], gt[p][2] + r[2])
        return gq, gt

    def pose(s, A, frame, additive=None, add_frame=0.0, layers=()):
        """layers: extra (anim, frame) pairs applied after A; bones they contain override (e.g. viewmodel ads_up on tag_torso)"""
        lq, lt = list(s.lq), list(s.lt)
        for LA, lf in [(A, frame)] + list(layers):
            s._apply(LA, lf, lq, lt)
        if additive is not None:   # assetType 6: local = local o additive rot, trans += additive trans
            for bi, b in enumerate(additive['bones']):
                k = s.index.get(b['name'])
                if k is None: continue
                q, t = dx.sample_bone(additive, bi, add_frame)
                if q is not None: lq[k] = qnorm(qmul(lq[k], q))
                if t is not None: lt[k] = tuple(lt[k][c] + t[c] for c in range(3))
        return s.compose(lq, lt)

    def _apply(s, A, frame, lq, lt):
        for bi, b in enumerate(A['bones']):
            k = s.index.get(b['name'])
            if k is None: continue
            q, t = dx.sample_bone(A, bi, frame)
            lq[k] = q if q is not None else (0.0, 0.0, 0.0, 1.0)
            lt[k] = (s.lt[k][0] + t[0], s.lt[k][1] + t[1], s.lt[k][2] + t[2]) if t is not None else s.lt[k]

    def skin(s, gq, gt, model_index, lod=0):
        """returns list of (vertex array Nx3, tris) per surface of LOD"""
        mname, info, surfs, bmap = s.models[model_index]
        inv = s.inv_bind[model_index]
        # per model bone: matrix M = G_anim * inv(G_bind)
        mats = []
        for mb in range(len(info['bones'])):
            k = bmap[mb]
            bq, bt = inv[mb]
            R = qmat(gq[k]) @ qmat(qinv(bq))
            T = np.array(gt[k]) - R @ np.array(bt)
            mats.append((R, T))
        out = []
        li = info['lods'][lod]
        for si in range(li['surfIndex'], li['surfIndex'] + li['numsurfs']):
            sf = surfs[si]
            V = np.array([v['xyz'] for v in sf['verts']])
            P = np.zeros_like(V)
            for vi, bones in enumerate(vertex_weights(s.W, s.models[model_index][0], si, sf)):
                acc = np.zeros(3)
                for bone, w in bones:
                    R, T = mats[bone]
                    acc += w * (R @ V[vi] + T)
                P[vi] = acc
            out.append((P, sf['tris']))
        return out


_wcache = {}


def vertex_weights(W, mname, si, sf):
    key = (mname, si)
    if key in _wcache: return _wcache[key]
    vc = sf['verts']; n = len(vc)
    res = [None] * n
    if sf['rigidLists']:
        v = 0
        for r in sf['rigidLists']:
            for _ in range(r['vertCount']):
                res[v] = [(r['boneIndex'], 1.0)]; v += 1
        for i in range(v, n): res[i] = [(0, 1.0)]
    else:
        xm = {a[1]: a[2] for a in W.assets if a[0] == 'xmodel'}[mname]
        s = unwrap(G(xm, 'surfs'))[si]
        vi = G(s, 'vertInfo')
        cnt = [G(vi, 'vertCount', k) for k in range(4)]
        raw = data_bytes(W, G(vi, 'vertsBlend'), 2 * (cnt[0] + 3 * cnt[1] + 5 * cnt[2] + 7 * cnt[3]))
        u = struct.unpack('<%dH' % (len(raw) // 2), raw)
        p = 0; v = 0
        for nb in range(4):
            for _ in range(cnt[nb]):
                bones = [u[p] // 64]; ws = []
                p += 1
                for _k in range(nb):
                    bones.append(u[p] // 64); ws.append(u[p + 1] / 65535.0); p += 2
                w0 = 1.0 - sum(ws)
                res[v] = list(zip(bones, [w0] + ws)); v += 1
        assert v == n, (v, n)
    _wcache[key] = res
    return res


def delta_at(A, frame):
    """root motion at frame: (yaw quat (0,0,z,w), translation) relative to the anim start (entity space)"""
    d = A['delta']
    if not d: return (0, 0, 0, 1), (0, 0, 0)
    q = dx.sample_keys(d['quat'], frame, True) or (0, 0, 0, 1)
    t = dx.sample_keys(d['trans'], frame, False) or (0, 0, 0)
    return q, t


def write_obj(path, parts, comment=''):
    with open(path, 'w') as f:
        f.write('# %s\n' % comment)
        base = 1
        for name, surfs in parts:
            for k, (P, tris) in enumerate(surfs):
                f.write('o %s_%d\n' % (name, k))
                for v in P: f.write('v %.4f %.4f %.4f\n' % tuple(v))
                for t in tris: f.write('f %d %d %d\n' % (base + t[0], base + t[1], base + t[2]))
                base += len(P)
