"""Extract rawfiles (gsc/csc/...) from a fastfile using the research/t4 walker.
usage: py extract_rawfiles.py <zone.ff> <out_dir> [substr ...]   (only names containing one of the substrings)"""
import sys, os
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 't4'))
from zonewalk import G
from t4assets import walk, data_bytes

W = walk(sys.argv[1]); out = sys.argv[2]; subs = [s.lower() for s in sys.argv[3:]]
n = 0
for a in W.assets:
    if a[0] not in ('rawfile', 'RawFile') or a[2] is None: continue
    name = a[1]
    if subs and not any(s in name.lower() for s in subs): continue
    b = data_bytes(W, G(a[2], 'buffer'), G(a[2], 'len'))
    if b is None: print('no data', name); continue
    p = os.path.join(out, name.replace('/', os.sep))
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, 'wb').write(b); n += 1
print('wrote', n, 'rawfiles to', out)
