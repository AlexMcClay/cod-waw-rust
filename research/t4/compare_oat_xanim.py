"""Cross-check decode_xanim against OAT's dumped raw xanims (oat/dump/xanim/*, binary version 17).
OAT stores N-1 quat components (w rebuilt with sqrt) -> quats are compared up to sign."""
import struct, os, math, sys
from t4assets import walk
import decode_xanim as dx
HERE = os.path.dirname(os.path.abspath(__file__))
DUMP = os.path.join(HERE, 'oat', 'dump', 'xanim')

class R:
    def __init__(s, b): s.b, s.p = b, 0
    def u(s, f):
        v = struct.unpack_from('<' + f, s.b, s.p); s.p += struct.calcsize('<' + f); return v if len(v) > 1 else v[0]
    def cstr(s):
        e = s.b.index(0, s.p); v = s.b[s.p:e].decode('latin-1'); s.p = e + 1; return v

def parse_raw(path):
    r = R(open(path, 'rb').read())
    ver, nfr, nb, flags, atype, fps = r.u('H'), r.u('H'), r.u('H'), r.u('B'), r.u('B'), r.u('H')
    looped = bool(flags & 1); nf = nfr if looped else nfr - 1
    nlf = nf + 1; byte_idx = nf < 256
    def idx(n):
        if n >= nlf: return list(range(n))
        return list(r.u('%d%s' % (n, 'B' if byte_idx else 'H'))) if n > 1 else [r.u('B' if byte_idx else 'H')]
    out = {'nf': nf, 'looped': looped, 'atype': atype, 'fps': fps, 'bones': []}
    if flags & 2:
        n = r.u('H')
        if n == 1: r.u('h')
        elif n > 1: idx(n); r.u('%dh' % n)
        n = r.u('H')
        if n == 1: r.u('3f')
        elif n > 1:
            idx(n); small = r.u('B'); r.u('6f'); r.u('%d%s' % (3 * n, 'B' if small else 'H'))
    if nb:
        bm = (nb + 7) // 8
        flip = r.b[r.p:r.p + bm]; r.p += bm
        half = r.b[r.p:r.p + bm]; r.p += bm
        names = [r.cstr() for _ in range(nb)]
        for i in range(nb):
            ishalf = half[i // 8] >> (i % 8) & 1
            k = 1 if ishalf else 3
            n = r.u('H')
            rot = []
            if n == 1: rot = [(0, r.u('%dh' % k) if k > 1 else (r.u('h'),))]
            elif n > 1:
                ix = idx(n); vals = r.u('%dh' % (n * k))
                vals = vals if isinstance(vals, tuple) else (vals,)
                rot = [(ix[j], vals[j * k:(j + 1) * k]) for j in range(n)]
            n = r.u('H'); tr = []
            if n == 1: tr = [(0, r.u('3f'))]
            elif n > 1:
                ix = idx(n); small = r.u('B'); mins = r.u('3f'); size = r.u('3f')
                sc = 0.003921568859368563 if small else 0.00001525902189314365
                v = r.u('%d%s' % (3 * n, 'B' if small else 'H'))
                tr = [(ix[j], tuple(mins[c] + size[c] * sc * v[3 * j + c] for c in range(3))) for j in range(n)]
            out['bones'].append({'name': names[i], 'half': ishalf, 'rot': rot, 'trans': tr})
    cnt = r.u('B'); out['notes'] = [(r.cstr(), r.u('H')) for _ in range(cnt)]
    assert r.p == len(r.b), (path, r.p, len(r.b))
    return out

W = walk(); X = dx.all_xanims(W)
bad = 0; nkeys = 0; maxq = 0; maxt = 0
for name, rec in X.items():
    A = dx.decode(W, rec); O = parse_raw(os.path.join(DUMP, name))
    prob = []
    if O['nf'] != A['numframes'] or O['looped'] != A['loop'] or O['atype'] != A['assetType']: prob.append('header')
    if [b['name'] for b in O['bones']] != [b['name'] for b in A['bones']]: prob.append('names')
    for ob, ab in zip(O['bones'], A['bones']):
        if [k[0] for k in ob['rot']] != [k[0] for k in ab['rot_raw']]: prob.append('rotframes ' + ab['name']); continue
        for (f, ov), (_, av) in zip(ob['rot'], ab['rot_raw']):
            nkeys += 1
            # compare stored components up to a global sign: OAT stores first N-1 comps
            s = 1 if sum(a * o for a, o in zip(av, ov)) >= 0 else -1
            maxq = max(maxq, max(abs(a * s - o) for a, o in zip(av, ov)))
        if [k[0] for k in ob['trans']] != [k[0] for k in ab['trans']]: prob.append('transframes ' + ab['name']); continue
        for (f, ov), (_, av) in zip(ob['trans'], ab['trans']):
            nkeys += 1; maxt = max(maxt, max(abs(a - o) for a, o in zip(av, ov)))
    on = [n for n, f in O['notes']]; an = [n for n, t in A['notifies'] if not (n == 'end' and abs(t - 1) < 1e-4)]
    if on != an: prob.append('notes %s %s' % (on, an))
    else:
        for (n, f), (_, t) in zip(O['notes'], [x for x in A['notifies'] if not (x[0] == 'end' and abs(x[1] - 1) < 1e-4)]):
            if abs(f - t * A['numframes']) > 0.51: prob.append('notetime %s %s %s' % (n, f, t))
    if prob: bad += 1; print(name, prob[:4])
print('anims compared %d, with differences %d, keys compared %d, max |quat component diff| %d (int16 units), max trans diff %.2e' % (len(X), bad, nkeys, maxq, maxt))
