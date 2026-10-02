"""
zonewalk.py - sequential walker for CoD WaW (T4, PC) fastfile zones.

Runtime for t4_loaders_gen.py (machine translation of OAT ZoneCodeGenerator's T4 loaders).
It emulates the game's DB_Load* stream semantics exactly:
  * 7 XFILE blocks: 0 TEMP, 1 RUNTIME, 2 LARGE_RUNTIME, 3 PHYSICAL_RUNTIME, 4 VIRTUAL, 5 LARGE, 6 PHYSICAL
  * data is read from the zone stream into the block on top of the block stack; RUNTIME blocks
    consume NO bytes from the file (they are just zero-filled memory reservations)
  * Alloc(align) only rounds the *block offset* up -- it never skips bytes in the file
  * TEMP block offset is saved on push and restored on pop
  * pointer value 0 = null, 0xFFFFFFFF = data follows inline, 0xFFFFFFFE = follows inline AND a
    4-byte pointer slot is reserved in VIRTUAL (alias for later back-references)
  * any other value v: (v-1) -> block = (v-1) >> 29, offset = (v-1) & 0x1FFFFFFF
    (native pointer = address of data in block; alias pointer = address of a pointer slot)

Usage:  py zonewalk.py [zone.bin]   -> walks all assets, prints summary, writes assets_walk.txt
"""
import struct, sys, os, json, zlib, bisect
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
LAY = json.load(open(os.path.join(HERE, 't4_layouts.json')))
LAYOUTS = LAY['layouts']; MEMBERS = LAY['members']; ENUM_BASE = LAY['enum_base']; TYPEDEFS = LAY['typedefs']

# extra layouts for variable-sized XAnim structs (offsets from OAT LoadDynamicFill_*)
for k, v in {
    'XAnimPartTrans': {'size': 4, 'offsets': {'size': 0, 'smallTrans': 2, 'u': 4}},
    'XAnimPartTransData': {'size': 0, 'offsets': {'frames': 0, 'frame0': 0}},
    'XAnimPartTransFrames': {'size': 28, 'offsets': {'mins': 0, 'size': 12, 'frames': 24, 'indices': 28}},
    'XAnimDeltaPartQuat': {'size': 4, 'offsets': {'size': 0, 'u': 4}},
    'XAnimDeltaPartQuatData': {'size': 0, 'offsets': {'frames': 0, 'frame0': 0}},
    'XAnimDeltaPartQuatDataFrames': {'size': 4, 'offsets': {'frames': 0, 'indices': 4}},
    'MaterialTechnique': {'size': 8, 'offsets': {'name': 0, 'flags': 4, 'passCount': 6, 'passArray': 8}},
}.items():
    l = LAYOUTS.setdefault(k, {'offsets': {}, 'size': None, 'align': None})
    for f, o in v['offsets'].items(): l['offsets'].setdefault(f, o)
    if l['size'] is None: l['size'] = v['size']

XFILE_BLOCK_TEMP, XFILE_BLOCK_RUNTIME, XFILE_BLOCK_LARGE_RUNTIME, XFILE_BLOCK_PHYSICAL_RUNTIME, \
    XFILE_BLOCK_VIRTUAL, XFILE_BLOCK_LARGE, XFILE_BLOCK_PHYSICAL = range(7)
BLOCK_KIND = ['temp', 'runtime', 'runtime', 'runtime', 'normal', 'normal', 'normal']
NUL = bytes(1)
FOLLOWING, INSERT, OFFSET = 'FOLLOWING', 'INSERT', 'OFFSET'

ASSET_TYPES = ['xmodelpieces', 'physpreset', 'physconstraints', 'destructibledef', 'xanim', 'xmodel', 'material',
               'techniqueset', 'image', 'sound', 'loadedsound', 'clipmap', 'clipmap_pvs', 'comworld',
               'gameworldsp', 'gameworldmp', 'mapents', 'gfxworld', 'lightdef', 'uimap', 'font', 'menulist',
               'menu', 'localize', 'weapon', 'snddriverglobals', 'fx', 'impactfx', 'aitype', 'mptype',
               'character', 'xmodelalias', 'rawfile', 'stringtable', 'packindex']
TYPE_STRUCT = {1: 'PhysPreset', 2: 'PhysConstraints', 3: 'DestructibleDef', 4: 'XAnimParts', 5: 'XModel',
               6: 'Material', 7: 'MaterialTechniqueSet', 8: 'GfxImage', 9: 'snd_alias_list_t', 10: 'LoadedSound',
               11: 'clipMap_t', 12: 'clipMap_t', 13: 'ComWorld', 14: 'GameWorldSp', 15: 'GameWorldMp',
               16: 'MapEnts', 17: 'GfxWorld', 18: 'GfxLightDef', 20: 'Font_s', 21: 'MenuList', 22: 'menuDef_t',
               23: 'LocalizeEntry', 24: 'WeaponDef', 25: 'SndDriverGlobals', 26: 'FxEffectDef',
               27: 'FxImpactTable', 32: 'RawFile', 33: 'StringTable', 34: 'PackIndex'}
STRUCT_TYPENAME = {'PhysPreset': 'physpreset', 'PhysConstraints': 'physconstraints', 'DestructibleDef': 'destructibledef',
                   'XAnimParts': 'xanim', 'XModel': 'xmodel', 'Material': 'material', 'MaterialTechniqueSet': 'techniqueset',
                   'GfxImage': 'image', 'snd_alias_list_t': 'sound', 'LoadedSound': 'loadedsound', 'clipMap_t': 'clipmap',
                   'ComWorld': 'comworld', 'GameWorldSp': 'gameworldsp', 'GameWorldMp': 'gameworldmp', 'MapEnts': 'mapents',
                   'GfxWorld': 'gfxworld', 'GfxLightDef': 'lightdef', 'Font_s': 'font', 'MenuList': 'menulist',
                   'menuDef_t': 'menu', 'LocalizeEntry': 'localize', 'WeaponDef': 'weapon',
                   'SndDriverGlobals': 'snddriverglobals', 'FxEffectDef': 'fx', 'FxImpactTable': 'impactfx',
                   'RawFile': 'rawfile', 'StringTable': 'stringtable', 'PackIndex': 'packindex'}
NAME_PATH = {'Material': ('info', 'name'), 'snd_alias_list_t': ('aliasName',), 'WeaponDef': ('szInternalName',),
             'Font_s': ('fontName',), 'menuDef_t': ('window', 'name')}

PRIM = {'char': 'b', 'signed char': 'b', 'int8_t': 'b', 'unsigned char': 'B', 'uint8_t': 'B', 'bool': 'B',
        'short': 'h', 'int16_t': 'h', 'unsigned short': 'H', 'uint16_t': 'H',
        'int': 'i', 'int32_t': 'i', 'unsigned int': 'I', 'uint32_t': 'I', 'unsigned': 'I', 'float': 'f',
        'uint64_t': 'Q', 'int64_t': 'q', 'unsigned __int64': 'Q', '__int64': 'q'}
ENUM_FMT = {'int': 'i', 'unsigned int': 'I', 'unsigned char': 'B', 'char': 'b', 'unsigned short': 'H'}


def ptype(v):
    if isinstance(v, int):
        v &= 0xFFFFFFFF
        if v == 0xFFFFFFFF: return FOLLOWING
        if v == 0xFFFFFFFE: return INSERT
    return OFFSET


def decode_ptr(v):
    v = (v & 0xFFFFFFFF) - 1
    return v >> 29, v & 0x1FFFFFFF


# ----------------------------------------------------------------------------- value objects
class Slot:
    """memory reserved by Alloc() in a block; filled by the following load"""
    __slots__ = ('blk', 'boff', 'val')
    def __init__(s, blk, boff): s.blk, s.boff, s.val = blk, boff, None
    def __repr__(s): return 'Slot(%d,%#x,%r)' % (s.blk, s.boff, s.val)
    def __bool__(s): return True


class Data:
    """raw inline data: file position + size (fpos None for runtime blocks)"""
    __slots__ = ('fpos', 'size', 'blk', 'boff', 'esz')
    def __init__(s, fpos, size, blk, boff, esz): s.fpos, s.size, s.blk, s.boff, s.esz = fpos, size, blk, boff, esz
    def __repr__(s): return 'Data(fpos=%s,size=%d,blk=%d,boff=%#x)' % (s.fpos, s.size, s.blk, s.boff)
    def count(s): return s.size // s.esz if s.esz else 0


class XStr:
    __slots__ = ('s', 'fpos')
    def __init__(s, st, fpos): s.s, s.fpos = st, fpos
    def __repr__(s): return 'XStr(%r)' % s.s
    def __str__(s): return s.s


class BackRef:
    """unresolved back reference into block memory"""
    __slots__ = ('blk', 'off', 'kind')
    def __init__(s, blk, off, kind): s.blk, s.off, s.kind = blk, off, kind
    def __repr__(s): return 'BackRef(%s,%d,%#x)' % (s.kind, s.blk, s.off)


class Rec:
    """a struct instance: raw 32-bit bytes + location; fields decoded on demand"""
    __slots__ = ('t', 'buf', 'fpos', 'blk', 'boff', 'ov', 'subs')
    def __init__(s, t, buf, fpos, blk, boff):
        s.t, s.buf, s.fpos, s.blk, s.boff = t, buf, fpos, blk, boff
        s.ov = {}; s.subs = {}
    def __repr__(s): return 'Rec(%s@fpos=%s blk=%d boff=%#x)' % (s.t, s.fpos, s.blk, s.boff)
    def __bool__(s): return True

    def minfo(s, f):
        m = MEMBERS.get(s.t, {}).get(f)
        off = LAYOUTS.get(s.t, {}).get('offsets', {}).get(f)
        if m is None or off is None:
            raise KeyError('%s.%s (member %r offset %r)' % (s.t, f, m, off))
        return m, off

    def field(s, f, idx=()):
        key = (f, idx)
        if key in s.ov: return s.ov[key]
        if key in s.subs: return s.subs[key]
        m, off = s.minfo(f)
        t, ptr, dims = m['type'], m['ptr'], list(m['dims'])
        while t in TYPEDEFS and not ptr:
            t, extra = TYPEDEFS[t][0], TYPEDEFS[t][1]
            dims = dims + extra
        esz, fmt = elem_info(t, ptr)
        dims_i = [d if isinstance(d, int) else 1 for d in dims]
        flat = 0
        for k, i in enumerate(idx):
            stride = 1
            for d in dims_i[k + 1:]: stride *= d
            flat += i * stride
        o = off + flat * esz
        if len(idx) < len(dims_i) and fmt is None:
            # embedded array of structs (not fully indexed) -> Arr view
            n = dims_i[len(idx)]
            if n == 1 and s.buf is not None:  # dynamic trailing array -> rest of buffer
                n = max(1, (len(s.buf) - o) // esz) if esz else 1
            arr = Arr(t, [s._sub(t, o + j * esz, esz) for j in range(n)])
            s.subs[key] = arr
            return arr
        if len(idx) < len(dims_i) and fmt is not None:
            if ptr:
                pv = PtrView(s, f, idx); s.subs[key] = pv; return pv
            rest = dims_i[len(idx):]
            def nest(off, ds):
                if len(ds) == 1:
                    return [struct.unpack_from('<' + fmt, s.buf, off + j * esz)[0] for j in range(ds[0])]
                stride = esz
                for d in ds[1:]: stride *= d
                return [nest(off + j * stride, ds[1:]) for j in range(ds[0])]
            return nest(o, rest)
        if fmt is None:
            sub = s._sub(t, o, esz)
            s.subs[key] = sub
            return sub
        if s.buf is None or o + esz > len(s.buf):
            return 0
        return struct.unpack_from('<' + fmt, s.buf, o)[0]

    def _sub(s, t, o, esz):
        b = s.buf[o:o + esz] if (esz and s.buf is not None) else (s.buf[o:] if s.buf is not None else None)
        r = Rec(t, b, None if s.fpos is None else s.fpos + o, s.blk, s.boff + o)
        return r

    def ptr_loc(s, f, idx=()):
        m, off = s.minfo(f)
        return (s.blk, s.boff + off + 4 * (idx[0] if idx else 0))


class Arr(list):
    def __init__(s, t, items): list.__init__(s, items); s.t = t
    def __repr__(s): return 'Arr(%s x%d)' % (s.t, len(s))
    def __bool__(s): return True


class PtrArr:
    """array of 32-bit pointers loaded from the stream"""
    def __init__(s, vals, fpos, blk, boff): s.vals, s.fpos, s.blk, s.boff = vals, fpos, blk, boff; s.res = {}
    def get(s, i): return s.res.get(i, s.vals[i])
    def set(s, i, v): s.res[i] = v
    def loc(s, i): return (s.blk, s.boff + 4 * i)
    def __repr__(s): return 'PtrArr(n=%d,blk=%d,boff=%#x)' % (len(s.vals), s.blk, s.boff)
    def __bool__(s): return True


class PtrView:
    """embedded fixed array of pointers inside a Rec (e.g. WeaponDef::worldModel[16])"""
    def __init__(s, rec, f, idx): s.rec, s.f, s.idx = rec, f, idx
    def get(s, i): return s.rec.field(s.f, s.idx + (i,))
    def set(s, i, v): s.rec.ov[(s.f, s.idx + (i,))] = v
    def loc(s, i):
        m, off = s.rec.minfo(s.f)
        return (s.rec.blk, s.rec.boff + off + 4 * i)
    def __bool__(s): return True


def unwrap(c):
    while isinstance(c, Slot): c = c.val
    return c


def elem_info(t, ptr):
    if ptr: return 4, 'I'
    if t in PRIM: f = PRIM[t]; return struct.calcsize('<' + f), f
    if t in ENUM_BASE:
        f = ENUM_FMT.get(ENUM_BASE[t], 'i'); return struct.calcsize('<' + f), f
    if t in ('vec2_t',): return 8, None
    if t in ('vec3_t',): return 12, None
    if t in ('vec4_t',): return 16, None
    l = LAYOUTS.get(t)
    if l and l['size'] is not None: return l['size'], None
    return 0, None


def G(c, *path):
    return LV(c, *path).ref()


class LV:
    """an lvalue: container + path of field names / indices"""
    def __init__(s, c, *path): s.c, s.path = c, path

    def _resolve(s):
        c = unwrap(s.c)
        p = list(s.path)
        while p:
            f = p.pop(0)
            idx = []
            while p and not isinstance(p[0], str): idx.append(p.pop(0))
            if isinstance(c, (PtrArr, PtrView)):
                # c[i]
                assert not isinstance(f, str)
            if not p:
                return c, f, tuple(idx)
            c = unwrap(c.field(f, tuple(idx)))
        raise ValueError

    def ref(s):
        c, f, idx = s._resolve()
        if isinstance(c, Arr): c = c[0]
        return c.field(f, idx)

    def set(s, v):
        c, f, idx = s._resolve()
        if isinstance(c, Arr): c = c[0]
        c.ov[(f, idx)] = v
        if c.blk is not None and BLOCK_KIND[c.blk] == 'normal':
            m, off = c.minfo(f)
            if m['ptr']:
                CUR.S.ptrloc[(c.blk, c.boff + off + 4 * (idx[0] if idx else 0))] = v


class ElemLV:
    def __init__(s, arr, i): s.arr, s.i = arr, i
    def ref(s): return s.arr.get(s.i)
    def set(s, v):
        s.arr.set(s.i, v)
        blk, off = s.arr.loc(s.i)
        if blk is not None and BLOCK_KIND[blk] == 'normal': CUR.S.ptrloc[(blk, off)] = v


# ----------------------------------------------------------------------------- stream
class Stream:
    def __init__(s, z, pos, block_sizes):
        s.z, s.pos = z, pos
        s.sizes = block_sizes
        s.off = [0] * 7
        s.stack = []; s.tempstack = []
        s.segs = defaultdict(list)    # normal blocks: list of (boff, fpos, size)
        s.recs = {}                   # (blk, boff) -> Rec  (struct starts, for pointer lookups)
        s.ptrloc = {}                 # (blk, boff) -> value stored in a pointer slot (alias lookups)
        s.last_fill = None
        s.unresolved = 0

    def push(s, b):
        s.stack.append(b)
        if BLOCK_KIND[b] == 'temp': s.tempstack.append(s.off[b])

    def pop(s):
        b = s.stack.pop()
        if BLOCK_KIND[b] == 'temp': s.off[b] = s.tempstack.pop()

    def top(s): return s.stack[-1]

    def align(s, b, a):
        if a > 0: s.off[b] = (s.off[b] + a - 1) // a * a

    def alloc(s, a, dyn=None):
        if dyn is not None: return dyn
        b = s.top(); s.align(b, a)
        return Slot(b, s.off[b])

    def read(s, n):
        if not s.stack:
            fp = s.pos; s.pos += n
            return fp, None, None, s.z[fp:fp + n]
        b = s.top(); bo = s.off[b]
        s.off[b] += n
        if BLOCK_KIND[b] == 'runtime':
            return None, b, bo, None
        fp = s.pos; s.pos += n
        if BLOCK_KIND[b] == 'normal' and n:
            s.segs[b].append((bo, fp, n))
        if s.off[b] > s.sizes[b] and BLOCK_KIND[b] != 'temp':
            raise RuntimeError('block %d overflow' % b)
        return fp, b, bo, s.z[fp:fp + n]

    def read_nt(s):
        e = s.z.index(b'\0', s.pos)
        fp, b, bo, buf = s.read(e - s.pos + 1)
        return XStr(buf[:-1].decode('latin-1'), fp), b, bo

    def insert_ptr(s):
        b = XFILE_BLOCK_VIRTUAL
        s.align(b, 4)
        loc = (b, s.off[b]); s.off[b] += 4
        return loc

    def set_alias(s, loc, v):
        s.ptrloc[loc] = v

    def ptr_native(s, v):
        blk, off = decode_ptr(v)
        r = s.recs.get((blk, off))
        if r is not None: return r
        return BackRef(blk, off, 'data')

    def ptr_lookup(s, v):
        blk, off = decode_ptr(v)
        r = s.recs.get((blk, off))
        if r is not None: return r
        return BackRef(blk, off, 'struct')

    def alias_lookup(s, v):
        blk, off = decode_ptr(v)
        if (blk, off) in s.ptrloc: return s.ptrloc[(blk, off)]
        s.unresolved += 1
        return BackRef(blk, off, 'alias')

    def file_pos(s, blk, off):
        """map a normal-block offset back to the zone file position (None if not file-backed)"""
        segs = s.segs.get(blk)
        if not segs: return None
        i = bisect.bisect_right(segs, (off, 1 << 62, 0)) - 1
        if i < 0: return None
        bo, fp, n = segs[i]
        if bo <= off < bo + n: return fp + (off - bo)
        return None


# ----------------------------------------------------------------------------- loaders
CUR = None


class LoaderBase:
    ASSET = None; HEADER_SIZE = None; POST = None

    def __init__(s, W):
        s.W, s.S = W, W.S
        s.v = defaultdict(lambda: None)
        s.xs = None; s.dyn = None; s.cur = {}

    # --- struct reads
    def fill(s, t, n):
        fp, b, bo, buf = s.S.read(n)
        rec = Rec(t, buf, fp, b, bo)
        if b is not None and BLOCK_KIND[b] == 'normal': s.S.recs[(b, bo)] = rec
        tgt = s.v[t]
        if isinstance(tgt, Slot): tgt.val = rec
        s.v[t] = rec
        s.S.last_fill = rec
        return rec

    def fill_last(s, t):
        rec = s.S.last_fill
        rec.t = t
        s.v[t] = rec

    def load_array(s, t, esz, a, count, fn):
        tgt = s.v[t]
        if a:
            fp, b, bo, buf = s.S.read(esz * count)
            items = []
            for i in range(count):
                r = Rec(t, None if buf is None else buf[i * esz:(i + 1) * esz], None if fp is None else fp + i * esz, b, bo + i * esz)
                if b is not None and BLOCK_KIND[b] == 'normal': s.S.recs[(b, bo + i * esz)] = r
                items.append(r)
            arr = Arr(t, items)
            if isinstance(tgt, Slot): tgt.val = arr
        else:
            arr = unwrap(tgt)
            if isinstance(arr, Rec): arr = Arr(t, [arr])
        if fn is not None:
            for i in range(count):
                s.v[t] = arr[i]
                fn(False)
        elif count:
            s.v[t] = arr[count - 1]
        if fn is None and not count:
            s.v[t] = tgt

    def load_data(s, lv, esz, cnt):
        cnt = int(cnt)
        fp, b, bo, buf = s.S.read(esz * cnt)
        d = Data(fp, esz * cnt, b, bo, esz)
        tgt = None
        if isinstance(lv, LV):
            c, f, idx = lv._resolve()
            if isinstance(c, Arr): c = c[0]
            if c.minfo(f)[0]['ptr']: tgt = lv.ref()
        else:
            tgt = lv.ref()
        if isinstance(tgt, Slot): tgt.val = d
        else: lv.set(d)

    def load_into_var(s, t, esz, cnt):
        fp, b, bo, buf = s.S.read(esz * int(cnt))
        d = Data(fp, esz * int(cnt), b, bo, esz)
        tgt = s.v[t]
        if isinstance(tgt, Slot): tgt.val = d

    # --- strings
    def load_xstring(s, lv=None):
        lv = lv or s.xs
        v = lv.ref()
        if v:
            if ptype(v) == FOLLOWING:
                s.S.alloc(1)
                x, b, bo = s.S.read_nt()
                lv.set(x)
            elif isinstance(v, int):
                lv.set(s.S.ptr_native(v))

    def load_xstring_array(s, a, count):
        tgt = s.xs if not isinstance(s.xs, LV) else s.xs.ref()
        if a:
            fp, b, bo, buf = s.S.read(4 * count)
            pa = PtrArr(list(struct.unpack('<%dI' % count, buf)) if buf else [0] * count, fp, b, bo)
            if isinstance(tgt, Slot): tgt.val = pa
        else:
            pa = unwrap(tgt)
        for i in range(count):
            s.load_xstring(ElemLV(pa, i))

    # --- pointer arrays
    def ptr_array_begin(s, key, a, count):
        tgt = s.v[key]
        if a:
            fp, b, bo, buf = s.S.read(4 * count)
            pa = PtrArr(list(struct.unpack('<%dI' % count, buf)) if buf else [0] * count, fp, b, bo)
            if isinstance(tgt, Slot): tgt.val = pa
            return pa
        return unwrap(tgt)

    def ptr_array_select(s, t, i): s.cur[t] = i
    def deref(s, t): return s.v[t + 'Ptr'].get(s.cur[t])
    def deref_lv(s, t): return ElemLV(s.v[t + 'Ptr'], s.cur[t])

    # --- dynamic (variable sized) structs, hand-ported from OAT LoadDynamicFill_*
    def dynfill(s, name):
        S = s.S
        if name == 'MaterialTechnique':
            fp, b, bo, h = S.read(8)
            n = struct.unpack_from('<H', h, 6)[0]
            _, _, _, rest = S.read(20 * n)
            buf = h + (rest or b'')
        elif name == 'XAnimPartTrans':
            fp, b, bo, h = S.read(4)
            size = struct.unpack_from('<H', h, 0)[0]
            nf = G(s.v['XAnimParts'], 'numframes')
            if size > 0:
                _, _, _, r1 = S.read(28)
                _, _, _, r2 = S.read((size + 1) * (1 if nf < 256 else 2))
                buf = h + r1 + r2
            else:
                _, _, _, r1 = S.read(12); buf = h + r1
        elif name == 'XAnimDeltaPartQuat':
            fp, b, bo, h = S.read(4)
            size = struct.unpack_from('<H', h, 0)[0]
            nf = G(s.v['XAnimParts'], 'numframes')
            if size > 0:
                _, _, _, r1 = S.read(4)
                _, _, _, r2 = S.read((size + 1) * (1 if nf < 256 else 2))
                buf = h + r1 + r2
            else:
                _, _, _, r1 = S.read(4); buf = h + r1
        else:
            raise NotImplementedError(name)
        rec = Rec(name, buf, fp, b, bo)
        S.last_fill = rec
        if b is not None and BLOCK_KIND[b] == 'normal': S.recs[(b, bo)] = rec
        return rec


class Walker:
    def __init__(s, z):
        global CUR
        CUR = s
        s.z = z
        hdr = struct.unpack_from('<9I', z, 0)
        s.size, s.external = hdr[0], hdr[1]
        s.block_sizes = list(hdr[2:9])
        s.S = Stream(z, 36, s.block_sizes)
        s.assets = []          # (typename, name, rec, start_fpos, end_fpos)  in load order (incl. inline deps)
        s.top = []             # top-level asset table entries: (index, typeid, rec)
        import t4_loaders_gen
        s.LOADERS = t4_loaders_gen.LOADERS

    def asset_name(s, t, rec):
        path = NAME_PATH.get(t, ('name',))
        try:
            v = G(rec, *path)
        except KeyError:
            return '?'
        return s.cstr(v)

    def cstr(s, v):
        """string value of a char* field: inline XStr, or back-reference resolved through block segments"""
        if isinstance(v, XStr): return v.s
        if isinstance(v, BackRef):
            fp = s.S.file_pos(v.blk, v.off)
            if fp is not None:
                return s.z[fp:s.z.index(NUL, fp)].decode('latin-1')
        if v == 0 or v is None: return None
        return repr(v)

    def load_asset(s, t, lv):
        S = s.S
        v = lv.ref()
        if not v: return
        L = s.LOADERS[t](s)
        temp = getattr(L, 'TEMP', True)
        if temp: S.push(XFILE_BLOCK_TEMP)
        pt = ptype(v)
        if pt == FOLLOWING or (temp and pt == INSERT):
            start = S.pos
            slot = S.alloc(4)
            tie = S.insert_ptr() if pt == INSERT else None
            L.v[t] = slot
            getattr(L, 'Load_' + t)(True)
            rec = slot.val   # NB: L.v[t] may have been clobbered by recursion; the slot always holds the header
            s.assets.append((STRUCT_TYPENAME.get(t, t), s.asset_name(t, rec), rec, start, S.pos))
            lv.set(rec)
            if tie is not None: S.set_alias(tie, rec)
        elif temp:
            lv.set(S.alias_lookup(v))
        else:
            lv.set(S.ptr_native(v))   # non-TEMP assets (StringTable): plain pointer back-reference
        if temp: S.pop()

    def walk(s, verbose=False, stop_after=None):
        S = s.S
        fp, _, _, buf = S.read(16)
        sc, sp, ac, ap = struct.unpack('<iIiI', buf)
        S.push(XFILE_BLOCK_VIRTUAL)
        s.script_strings = []
        if sp:
            S.alloc(4)
            fp, b, bo, buf = S.read(4 * sc)
            pa = PtrArr(list(struct.unpack('<%dI' % sc, buf)), fp, b, bo)
            L = LoaderBase(s)
            for i in range(sc):
                L.load_xstring(ElemLV(pa, i))
                x = pa.get(i)
                s.script_strings.append(x.s if isinstance(x, XStr) else None)
        if ap:
            S.alloc(4)
            fp, b, bo, buf = S.read(8 * ac)
            s.table_fpos = fp
            types = struct.unpack('<%dI' % (2 * ac), buf)[0::2]
            pa = PtrArr(list(struct.unpack('<%dI' % (2 * ac), buf)[1::2]), fp + 4, b, bo + 4)
            pa.loc = lambda i, pa=pa: (pa.blk, pa.boff + 8 * i)
            for i in range(ac):
                t = TYPE_STRUCT.get(types[i])
                if t is None: raise NotImplementedError('asset type %d' % types[i])
                n0 = len(s.assets)
                s.load_asset(t, ElemLV(pa, i))
                s.top.append((i, types[i], pa.get(i)))
                if verbose:
                    a = s.assets[-1]
                    print('%5d type %2d %-14s %-50s fpos %d..%d' % (i, types[i], a[0], a[1][:50], a[3], a[4]))
                if stop_after is not None and i >= stop_after: break
        S.pop()
        return s


def load_zone(path=None):
    p = path or os.path.join(HERE, 'zone.bin')
    if p.endswith('.ff'):
        d = open(p, 'rb').read(); z = zlib.decompress(d[12:])
    else:
        z = open(p, 'rb').read()
    return z


def main():
    z = load_zone(sys.argv[1] if len(sys.argv) > 1 else None)
    W = Walker(z)
    W.walk(verbose='-v' in sys.argv)
    S = W.S
    print('walk done: file pos %d of %d; block offsets %s; sizes %s' % (S.pos, len(z), [hex(x) for x in S.off], [hex(x) for x in W.block_sizes]))
    print('assets loaded (incl. inline deps):', len(W.assets), 'unresolved alias refs:', S.unresolved)
    out = os.path.join(HERE, 'assets_walk.txt' if len(sys.argv) < 2 or 'nazi_zombie_prototype' in sys.argv[1] or sys.argv[1].endswith('zone.bin') else 'assets_walk_' + os.path.basename(sys.argv[1]) + '.txt')
    with open(out, 'w', encoding='utf-8') as f:
        for a in W.assets: f.write('%s, %s, %d, %d\n' % (a[0], a[1], a[3], a[4]))


if __name__ == '__main__':
    import zonewalk  # re-import so classes are shared with t4_loaders_gen (avoid __main__ duplication)
    zonewalk.main()
