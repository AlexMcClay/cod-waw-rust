"""
trace_asset.py - print every stream read (zone offset, size, block, block offset, loader function) while
loading the N-th top-level asset (or the first top-level asset whose name contains a substring).

usage: py trace_asset.py <table index | name substring> [max lines]
"""
import sys, inspect
import zonewalk as zw

target = sys.argv[1] if len(sys.argv) > 1 else '0'
maxl = int(sys.argv[2]) if len(sys.argv) > 2 else 200
BN = ['TEMP', 'RUNTIME', 'LARGE_RT', 'PHYS_RT', 'VIRTUAL', 'LARGE', 'PHYSICAL']
W = zw.Walker(zw.load_zone())
S = W.S
orig_read = zw.Stream.read
state = {'on': False, 'n': 0}


def read(s, n):
    r = orig_read(s, n)
    if state['on'] and state['n'] < maxl:
        fr = inspect.stack()[1:6]
        fn = [f.function for f in fr if f.function not in ('fill', 'load_data', 'read_nt', 'load_array', 'load_xstring', 'ptr_array_begin', 'dynfill', 'load_xstring_array')]
        what = [f.function for f in fr][:2]
        fp, b, bo, buf = r
        print('  zone@%-9s size %-7d %-8s +%#-9x %-28s %s' % (fp if fp is not None else '-', n, BN[b] if b is not None else 'raw', bo or 0,
                                                            (fn[0] if fn else '?'), '/'.join(what)))
        state['n'] += 1
    return r


zw.Stream.read = read
orig_load = W.load_asset
depth = [0]


def la(t, lv):
    depth[0] += 1
    v = lv.ref()
    if state['on'] and state['n'] < maxl:
        print('%s>> load_asset %s ptr=%s  (zone@%d, VIRTUAL off %#x)' % ('  ' * depth[0], t, hex(v) if isinstance(v, int) else type(v).__name__, S.pos, S.off[4]))
    try:
        orig_load(t, lv)
    finally:
        depth[0] -= 1


W.load_asset = la
orig_walk_load = None
# patch: turn tracing on/off around the selected top-level asset
import struct
z = W.z
types = struct.unpack_from('<6280I', z, 11587)[0::2]
idx = int(target) if target.isdigit() else None
real_la = la


def top_la(t, lv):
    global idx
    i = lv.i if isinstance(lv, zw.ElemLV) and depth[0] == 0 else None
    if i is not None and (idx == i):
        state['on'] = True
        print('=== top-level asset %d type %d (%s) starts at zone@%d' % (i, types[i], t, S.pos))
        real_la(t, lv)
        state['on'] = False
        print('=== ends at zone@%d' % S.pos)
        raise SystemExit
    return real_la(t, lv)


if idx is None:
    # find by name: walk once
    W2 = zw.Walker(z); W2.walk()
    for (i, ty, rec) in W2.top:
        nm = W2.asset_name(zw.TYPE_STRUCT[ty], rec)
        if target in (nm or ''):
            idx = i; break
    zw.CUR = W
W.load_asset = top_la
try:
    W.walk()
except SystemExit:
    pass
