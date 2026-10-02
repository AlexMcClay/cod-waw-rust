"""
decode_xanim.py - decode T4 (CoD: World at War PC) XAnimParts keyframes from a zone walked by zonewalk.py.

usage:
  py decode_xanim.py                       -> consumption check over all xanims in Nacht (zone.bin)
  py decode_xanim.py <zone.ff|zone.bin>    -> consumption check over all xanims in that zone
  py decode_xanim.py <zone> <anim name>    -> print a decoded anim summary

API:
  W = t4assets.walk(path)
  anims = all_xanims(W)                      # {name: XAnimParts Rec}
  A = decode(W, anims['ai_zombie_walk_v1'])  # dict, see decode() docstring
  q, t = sample_bone(A, bone_index, frame)   # interpolated local quat (xyzw, normalized) / trans (or None)

Decoding algorithm (format description; cross-checked against OpenAssetTools FlatXAnimReader (GPL-3.0) and
verified by exact consumption of every stream for every xanim of every zone in the install):

  bone order: names[] (ScriptString u16) has boneCount[PART_TYPE_ALL] entries, sorted by quat part type:
    [NO_QUAT][HALF_QUAT][FULL_QUAT][HALF_QUAT_NO_SIZE][FULL_QUAT_NO_SIZE]   (counts boneCount[0..4])
  1) quat tracks, bones in that order:
      NO_QUAT           : nothing
      HALF_QUAT         : n = u16(dataShort)+1; idx = INDICES(n); n * (z,w) i16 pairs from randomDataShort
      FULL_QUAT         : n = u16(dataShort)+1; idx = INDICES(n); n * (x,y,z,w) i16 from randomDataShort
      HALF_QUAT_NO_SIZE : 1 * (z,w) from dataShort
      FULL_QUAT_NO_SIZE : 1 * (x,y,z,w) from dataShort
  2) trans tracks, grouped by trans type SMALL_TRANS, TRANS, TRANS_NO_SIZE, NO_TRANS (counts boneCount[5..8]),
     each entry starts with u8 bone index (dataByte) into names[]:
      SMALL_TRANS   : bone u8; n = u16(dataShort)+1; mins = 3 f32 (dataInt); size = 3 f32 (dataInt);
                      idx = INDICES(n); n * u8[3] from randomDataByte; value = mins + size * u8
      TRANS         : same but n * u16[3] from randomDataShort; value = mins + size * u16
      TRANS_NO_SIZE : bone u8; 3 f32 (dataInt) constant
      NO_TRANS      : bone u8
  INDICES(n):  numframes < 256 -> n u8 from dataByte
               else if n-1 >= 64 -> n u16 from the indices[] pool + ((n-2)//256 + 2) u16 "checkpoint" values in
                                    dataShort (copies of idx[0], idx[256], idx[512], ..., idx[n-1]) used by the
                                    runtime binary search
               else -> n u16 from dataShort
  randomDataInt is unused (count 0 everywhere).
"""
import struct, sys, os, math
import zonewalk as zw
from zonewalk import G, unwrap, Rec, Data
from t4assets import walk, data_bytes

PART_NAMES = ['NO_QUAT', 'HALF_QUAT', 'FULL_QUAT', 'HALF_QUAT_NO_SIZE', 'FULL_QUAT_NO_SIZE',
              'SMALL_TRANS', 'TRANS', 'TRANS_NO_SIZE', 'NO_TRANS', 'ALL']
Q_SCALE = 1.0 / 32767.0


class StreamExhausted(Exception):
    pass


class Cursor:
    def __init__(s, name, raw, fmt):
        s.name, s.fmt = name, fmt
        s.esz = struct.calcsize('<' + fmt)
        s.n = len(raw) // s.esz if raw else 0
        s.vals = struct.unpack('<%d%s' % (s.n, fmt), raw[:s.n * s.esz]) if s.n else ()
        s.p = 0

    def take(s, k):
        if s.p + k > s.n:
            raise StreamExhausted('%s: need %d at %d, have %d' % (s.name, k, s.p, s.n))
        v = s.vals[s.p:s.p + k]
        s.p += k
        return v

    def left(s): return s.n - s.p


def _bytes(W, rec, field, esz, count):
    if count == 0: return b''
    d = unwrap(G(rec, field))
    if not d: return b''
    b = data_bytes(W, d, esz * count)
    assert b is not None and len(b) == esz * count, (field, d)
    return b


def all_xanims(W):
    return {a[1]: a[2] for a in W.assets if a[0] == 'xanim' and a[2] is not None}


def _f32(i): return struct.unpack('<f', struct.pack('<i', i))[0]


def decode(W, r):
    """returns dict:
       name, numframes, framerate, frequency, loop, delta, assetType, isDefault, boneCount[10]
       bones: [ {name, quatType, transType, rot: [(frame, (x,y,z,w))], trans: [(frame, (x,y,z))]} ]   (anim order)
              rot quats are normalized floats (raw i16/32767 also kept in 'rot_raw'); half quats -> (0,0,z,w)
              rot empty for NO_QUAT; trans empty for NO_TRANS; single key (frame 0) for *_NO_SIZE
       delta: None or {trans: [(frame,(x,y,z))], quat: [(frame,(0,0,z,w))]}  (root motion, see notes)
       notifies: [(name, time 0..1)]
       consumed: {stream: (used, count)}  -- must be equal for a correct decode
    """
    nf = G(r, 'numframes')
    bc = [G(r, 'boneCount', i) for i in range(10)]
    nb = bc[9]
    byte_idx = nf < 256
    names_raw = _bytes(W, r, 'names', 2, nb)
    names = [W.script_strings[i] for i in struct.unpack('<%dH' % nb, names_raw)] if nb else []
    C = {
        'dataByte': Cursor('dataByte', _bytes(W, r, 'dataByte', 1, G(r, 'dataByteCount')), 'B'),
        'dataShort': Cursor('dataShort', _bytes(W, r, 'dataShort', 2, G(r, 'dataShortCount')), 'h'),
        'dataInt': Cursor('dataInt', _bytes(W, r, 'dataInt', 4, G(r, 'dataIntCount')), 'i'),
        'randomDataShort': Cursor('randomDataShort', _bytes(W, r, 'randomDataShort', 2, G(r, 'randomDataShortCount')), 'h'),
        'randomDataByte': Cursor('randomDataByte', _bytes(W, r, 'randomDataByte', 1, G(r, 'randomDataByteCount')), 'B'),
        'randomDataInt': Cursor('randomDataInt', _bytes(W, r, 'randomDataInt', 4, G(r, 'randomDataIntCount')), 'i'),
    }
    ic = G(r, 'indexCount')
    if ic:
        d = unwrap(G(r, 'indices', '_1' if byte_idx else '_2'))
        raw = data_bytes(W, d, ic * (1 if byte_idx else 2)) if d else b''
        C['indices'] = Cursor('indices', raw, 'B' if byte_idx else 'H')
    else:
        C['indices'] = Cursor('indices', b'', 'H')

    def indices(n):
        if byte_idx:
            return list(C['dataByte'].take(n))
        if n - 1 >= 64:
            idx = list(C['indices'].take(n))
            chk = [v & 0xFFFF for v in C['dataShort'].take((n - 2) // 256 + 2)]
            exp = [idx[k] for k in range(0, n - 1, 256)] + [idx[n - 1]]
            # checkpoint table: idx[0], idx[256], ..., idx[n-1]  (VERIFIED below via 'checkpoint_ok')
            decode.checkpoints_ok &= (chk == exp)
            return idx
        return [v & 0xFFFF for v in C['dataShort'].take(n)]

    bones = [{'name': names[i], 'quatType': None, 'transType': None, 'rot': [], 'rot_raw': [], 'trans': []} for i in range(nb)]
    bi = 0
    for qt in range(5):
        for _ in range(bc[qt]):
            b = bones[bi]; b['quatType'] = PART_NAMES[qt]
            if qt in (1, 2):
                n = (C['dataShort'].take(1)[0] & 0xFFFF) + 1
                idx = indices(n)
                k = 2 if qt == 1 else 4
                vals = C['randomDataShort'].take(n * k)
                for j in range(n):
                    v = vals[j * k:(j + 1) * k]
                    b['rot_raw'].append((idx[j], v))
            elif qt in (3, 4):
                k = 2 if qt == 3 else 4
                b['rot_raw'].append((0, C['dataShort'].take(k)))
            for f, v in b['rot_raw']:
                q = (0.0, 0.0, v[0] * Q_SCALE, v[1] * Q_SCALE) if len(v) == 2 else tuple(c * Q_SCALE for c in v)
                l = math.sqrt(sum(c * c for c in q)) or 1.0
                b['rot'].append((f, tuple(c / l for c in q)))
            bi += 1
    assert bi == nb
    for tt in range(5, 9):
        for _ in range(bc[tt]):
            bone = C['dataByte'].take(1)[0]
            b = bones[bone]
            assert b['transType'] is None
            b['transType'] = PART_NAMES[tt]
            if tt in (5, 6):
                n = (C['dataShort'].take(1)[0] & 0xFFFF) + 1
                mins = [_f32(x) for x in C['dataInt'].take(3)]
                size = [_f32(x) for x in C['dataInt'].take(3)]
                idx = indices(n)
                if tt == 5:
                    fr = C['randomDataByte'].take(3 * n)
                else:
                    fr = [v & 0xFFFF for v in C['randomDataShort'].take(3 * n)]
                b['trans_mins'], b['trans_size'] = mins, size
                for j in range(n):
                    b['trans'].append((idx[j], tuple(mins[c] + size[c] * fr[3 * j + c] for c in range(3))))
            elif tt == 7:
                b['trans'].append((0, tuple(_f32(x) for x in C['dataInt'].take(3))))
    assert all(b['transType'] for b in bones)
    consumed = {k: (c.p, c.n) for k, c in C.items()}

    # ---- delta part (root motion)
    delta = None
    dp = unwrap(G(r, 'deltaPart'))
    if dp:
        delta = {'trans': [], 'quat': []}
        t = unwrap(G(dp, 'trans'))
        if t:
            size = G(t, 'size')
            if size == 0:
                delta['trans'].append((0, struct.unpack_from('<3f', t.buf, 4)))
                delta['transType'] = 'constant'
            else:
                n = size + 1
                small = G(t, 'smallTrans')
                mins = struct.unpack_from('<3f', t.buf, 4); sz = struct.unpack_from('<3f', t.buf, 16)
                idx = struct.unpack_from('<%d%s' % (n, 'B' if byte_idx else 'H'), t.buf, 32)
                fr = unwrap(G(t, 'u', 'frames', 'frames'))
                fd = unwrap(G(fr, '_1' if small else '_2'))
                raw = data_bytes(W, fd, n * (3 if small else 6))
                vals = struct.unpack('<%d%s' % (3 * n, 'B' if small else 'H'), raw)
                for j in range(n):
                    delta['trans'].append((idx[j], tuple(mins[c] + sz[c] * vals[3 * j + c] for c in range(3))))
                delta['transType'] = 'small' if small else 'full'
        q = unwrap(G(dp, 'quat'))
        if q:
            size = G(q, 'size')
            if size == 0:
                raws = [(0, struct.unpack_from('<2h', q.buf, 4))]
            else:
                n = size + 1
                idx = struct.unpack_from('<%d%s' % (n, 'B' if byte_idx else 'H'), q.buf, 8)
                fd = unwrap(G(q, 'u', 'frames', 'frames'))
                raw = data_bytes(W, fd, 4 * n)
                v = struct.unpack('<%dh' % (2 * n), raw)
                raws = [(idx[j], v[2 * j:2 * j + 2]) for j in range(n)]
            for f, (z, w) in raws:
                l = math.hypot(z, w) or 1.0
                delta['quat'].append((f, (0.0, 0.0, z / l, w / l)))

    # ---- notifies
    notifies = []
    nt = unwrap(G(r, 'notify'))
    if nt:
        for e in nt:
            notifies.append((W.script_strings[G(e, 'name')], G(e, 'time')))

    return {'name': W.cstr(G(r, 'name')), 'numframes': nf, 'framerate': G(r, 'framerate'), 'frequency': G(r, 'frequency'),
            'loop': bool(G(r, 'bLoop')), 'delta_flag': bool(G(r, 'bDelta')), 'assetType': G(r, 'assetType'),
            'isDefault': bool(G(r, 'isDefault')), 'boneCount': bc, 'bones': bones, 'delta': delta, 'notifies': notifies,
            'consumed': consumed}


decode.checkpoints_ok = True


# ----------------------------------------------------------------------------- sampling
def _nlerp(a, b, t):
    if sum(x * y for x, y in zip(a, b)) < 0: b = tuple(-x for x in b)
    q = tuple(x + (y - x) * t for x, y in zip(a, b))
    l = math.sqrt(sum(c * c for c in q)) or 1.0
    return tuple(c / l for c in q)


def _lerp(a, b, t): return tuple(x + (y - x) * t for x, y in zip(a, b))


def sample_keys(keys, frame, quat):
    """keys sorted by frame; clamp outside; linear (n)lerp between surrounding keys"""
    if not keys: return None
    if frame <= keys[0][0] or len(keys) == 1: return keys[0][1]
    if frame >= keys[-1][0]: return keys[-1][1]
    lo, hi = 0, len(keys) - 1
    while hi - lo > 1:
        m = (lo + hi) // 2
        if keys[m][0] <= frame: lo = m
        else: hi = m
    f0, v0 = keys[lo]; f1, v1 = keys[hi]
    t = (frame - f0) / (f1 - f0)
    return _nlerp(v0, v1, t) if quat else _lerp(v0, v1, t)


def sample_bone(A, bi, frame):
    b = A['bones'][bi]
    return sample_keys(b['rot'], frame, True), sample_keys(b['trans'], frame, False)


# ----------------------------------------------------------------------------- checks
def check_zone(W, verbose=False):
    anims = all_xanims(W)
    bad = 0; stats = {'anims': 0, 'bones': 0, 'keys': 0}
    maxidx_issues = 0; unsorted = 0; delta_n = 0; types = {}
    for name, r in anims.items():
        try:
            A = decode(W, r)
        except Exception as e:
            print('FAIL decode', name, e); bad += 1; continue
        stats['anims'] += 1; stats['bones'] += len(A['bones'])
        mism = {k: v for k, v in A['consumed'].items() if v[0] != v[1]}
        if mism:
            bad += 1
            print('MISMATCH', name, mism)
        nf = A['numframes']
        for b in A['bones']:
            for keys in (b['rot'], b['trans']):
                stats['keys'] += len(keys)
                fr = [k[0] for k in keys]
                if fr != sorted(fr) or len(set(fr)) != len(fr): unsorted += 1
                if fr and fr[-1] > nf: maxidx_issues += 1
            types[(b['quatType'], b['transType'])] = types.get((b['quatType'], b['transType']), 0) + 1
        if A['delta']: delta_n += 1
    print('anims %d, bones %d, keys %d, stream mismatches %d, unsorted tracks %d, key frame > numframes %d, with delta %d, checkpoints ok %s' % (
        stats['anims'], stats['bones'], stats['keys'], bad, unsorted, maxidx_issues, delta_n, decode.checkpoints_ok))
    return bad


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else None
    W = walk(path)
    if len(sys.argv) > 2:
        A = decode(W, all_xanims(W)[sys.argv[2]])
        print({k: A[k] for k in ('name', 'numframes', 'framerate', 'frequency', 'loop', 'delta_flag', 'assetType', 'boneCount', 'notifies', 'consumed')})
        for b in A['bones']:
            print('  %-20s %-18s %-14s rotkeys %3d transkeys %3d  rot0 %s trans0 %s' % (
                b['name'], b['quatType'], b['transType'], len(b['rot']), len(b['trans']),
                tuple(round(c, 4) for c in b['rot'][0][1]) if b['rot'] else None,
                tuple(round(c, 3) for c in b['trans'][0][1]) if b['trans'] else None))
        if A['delta']:
            d = A['delta']
            print(' delta trans (%s) %d keys: first %s last %s' % (d.get('transType'), len(d['trans']), d['trans'][:1], d['trans'][-1:]))
            print(' delta quat %d keys: first %s last %s' % (len(d['quat']), d['quat'][:1], d['quat'][-1:]))
    else:
        sys.exit(1 if check_zone(W) else 0)


if __name__ == '__main__':
    main()
