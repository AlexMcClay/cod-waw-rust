# Walks every .ff in the install's zone/english and checks: EOF reached exactly, VIRTUAL/LARGE/PHYSICAL block totals
# equal the XFile header, and all alias references resolved.
import glob, os, sys, zlib, traceback
import zonewalk as zw
root = r"D:\SteamLibrary\steamapps\common\Call of Duty World at War\zone\english"
ok = 0; bad = []
for p in sorted(glob.glob(os.path.join(root, '*.ff'))):
    n = os.path.basename(p)
    try:
        z = zw.load_zone(p)
        W = zw.Walker(z); W.walk()
        S = W.S
        good = S.pos == len(z) and all(S.off[b] == W.block_sizes[b] for b in (4, 5, 6)) and S.unresolved == 0
        print('%-40s %s eof=%s virt=%s large=%s phys=%s unres=%d assets=%d' % (n, 'OK ' if good else 'BAD', S.pos == len(z),
              S.off[4] == W.block_sizes[4], S.off[5] == W.block_sizes[5], S.off[6] == W.block_sizes[6], S.unresolved, len(W.assets)), flush=True)
        ok += good
        if not good: bad.append(n)
    except Exception as e:
        print('%-40s EXC %s' % (n, repr(e)[:150]), flush=True); bad.append(n)
print('zones OK %d, bad %d: %s' % (ok, len(bad), bad))
