"""Decode every xanim in every zone of the install and check exact stream consumption."""
import glob, os, sys, collections
import zonewalk as zw
import decode_xanim as dx
ROOT = r'D:\SteamLibrary\steamapps\common\Call of Duty World at War\zone\english'
tot = collections.Counter(); seen = set()
out = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'verify_all_xanims.txt'), 'w')
def P(*a):
    s = ' '.join(str(x) for x in a); print(s); out.write(s + '\n'); out.flush()
for p in sorted(glob.glob(os.path.join(ROOT, '*.ff'))):
    W = zw.Walker(zw.load_zone(p)); W.walk()
    anims = dx.all_xanims(W)
    z = collections.Counter()
    for name, r in anims.items():
        z['anims'] += 1
        try:
            A = dx.decode(W, r)
        except Exception as e:
            z['fail'] += 1; P('  FAIL', name, repr(e)); continue
        if any(u != n for u, n in A['consumed'].values()):
            z['mismatch'] += 1; P('  MISMATCH', name, A['consumed']); continue
        nf = A['numframes']
        if nf >= 256: z['nf>=256'] += 1
        if any(len(b['rot']) >= 65 or len(b['trans']) >= 65 for b in A['bones']) and nf >= 256: z['pooled16'] += 1
        if A['consumed']['randomDataInt'][1]: z['randomDataInt'] += 1
        for b in A['bones']:
            for k in (b['rot'], b['trans']):
                if k and k[-1][0] > nf: z['key>nf'] += 1
        if name not in seen:
            seen.add(name); z['unique'] += 1
    tot.update(z)
    P('%-40s %s' % (os.path.basename(p), dict(z)))
P('TOTAL', dict(tot), 'checkpoints_ok', dx.decode.checkpoints_ok)
