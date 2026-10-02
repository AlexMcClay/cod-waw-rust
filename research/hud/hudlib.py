"""Shared helpers for the HUD research scripts: IWD image index, IWI decoder (Python), zone walking shortcuts.
Nothing here writes into the repo except under research/hud/local/ (git-ignored)."""
import os, sys, struct, zipfile, glob
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 't4'))
INSTALL = r"D:\SteamLibrary\steamapps\common\Call of Duty World at War"
ZONES = os.path.join(INSTALL, 'zone', 'english')
LOCAL = os.path.join(HERE, 'local')

_index = None


def iwd_index():
    """images/<name>.iwi -> (iwd path, member name). Later IWDs (sorted by name, localized after iw_*) win,
    matching the engine's 'highest-numbered pak overrides' rule (ASSUMED order; no conflicts matter here)."""
    global _index
    if _index is None:
        _index = {}
        paks = sorted(glob.glob(os.path.join(INSTALL, 'main', 'iw_*.iwd'))) + \
            sorted(glob.glob(os.path.join(INSTALL, 'main', 'localized_*.iwd')))
        for p in paks:
            with zipfile.ZipFile(p) as z:
                for n in z.namelist():
                    nl = n.lower()
                    if nl.startswith('images/') and nl.endswith('.iwi'):
                        _index[nl[7:-4]] = (p, n)
    return _index


def read_iwi(name):
    e = iwd_index().get(name.lower())
    if not e:
        return None
    with zipfile.ZipFile(e[0]) as z:
        return z.read(e[1])


FMT = {1: 'ARGB32', 2: 'RGB24', 3: 'LA16', 4: 'L8', 5: 'A8', 0x0B: 'DXT1', 0x0C: 'DXT3', 0x0D: 'DXT5'}


def _lvl_size(fmt, w, h):
    w, h = max(1, w), max(1, h)
    if fmt in (0x0B, 0x0C, 0x0D):
        return ((w + 3) // 4) * ((h + 3) // 4) * (8 if fmt == 0x0B else 16)
    return w * h * {1: 4, 2: 3, 3: 2, 4: 1, 5: 1}[fmt]


def _rgb565(c):
    r, g, b = (c >> 11) & 31, (c >> 5) & 63, c & 31
    return ((r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2))


def _color_block(b, punch):
    c0, c1, bits = struct.unpack_from('<HHI', b, 0)
    p0, p1 = _rgb565(c0), _rgb565(c1)
    if c0 > c1 or not punch:
        p2 = tuple((2 * a + c) // 3 for a, c in zip(p0, p1)); p3 = tuple((a + 2 * c) // 3 for a, c in zip(p0, p1))
        pal = [p0 + (255,), p1 + (255,), p2 + (255,), p3 + (255,)]
    else:
        p2 = tuple((a + c) // 2 for a, c in zip(p0, p1))
        pal = [p0 + (255,), p1 + (255,), p2 + (255,), (0, 0, 0, 0)]
    return [list(pal[(bits >> (2 * i)) & 3]) for i in range(16)]


def decode_iwi(data):
    """-> (format name, width, height, RGBA bytes of the full-size level). IWI v6: 28-byte header, mips smallest first."""
    assert data[:3] == b'IWi' and data[3] == 6, 'not IWI v6'
    fmt, flags = data[4], data[5]
    w, h = struct.unpack_from('<HH', data, 6)
    if fmt not in FMT:
        raise ValueError('unsupported IWI format %#x' % fmt)
    size = _lvl_size(fmt, w, h)
    lvl = data[len(data) - size:]          # full-size level is last in the file
    out = bytearray(w * h * 4)
    if fmt in (0x0B, 0x0C, 0x0D):
        bs = 8 if fmt == 0x0B else 16
        bw = (w + 3) // 4
        for bi in range(len(lvl) // bs):
            bx, by = bi % bw, bi // bw
            if by * 4 >= h:
                break
            blk = lvl[bi * bs:(bi + 1) * bs]
            if fmt == 0x0B:
                px = _color_block(blk, True)
            else:
                px = _color_block(blk[8:], False)
                if fmt == 0x0C:
                    a = int.from_bytes(blk[:8], 'little')
                    for i in range(16):
                        px[i][3] = ((a >> (4 * i)) & 15) * 17
                else:
                    a0, a1 = blk[0], blk[1]
                    ab = int.from_bytes(blk[2:8], 'little')
                    if a0 > a1:
                        al = [a0, a1] + [((7 - k) * a0 + k * a1) // 7 for k in range(1, 7)]
                    else:
                        al = [a0, a1] + [((5 - k) * a0 + k * a1) // 5 for k in range(1, 5)] + [0, 255]
                    for i in range(16):
                        px[i][3] = al[(ab >> (3 * i)) & 7]
            for y in range(4):
                for x in range(4):
                    ix, iy = bx * 4 + x, by * 4 + y
                    if ix < w and iy < h:
                        o = (iy * w + ix) * 4
                        out[o:o + 4] = bytes(px[y * 4 + x])
    else:
        n = w * h
        for i in range(n):
            if fmt == 1:
                b, g, r, a = lvl[i * 4:i * 4 + 4]
            elif fmt == 2:
                b, g, r = lvl[i * 3:i * 3 + 3]; a = 255
            elif fmt == 3:
                r = g = b = lvl[i * 2]; a = lvl[i * 2 + 1]
            elif fmt == 4:
                r = g = b = lvl[i]; a = 255
            else:
                r = g = b = 255; a = lvl[i]
            out[i * 4:i * 4 + 4] = bytes((r, g, b, a))
    return FMT[fmt], w, h, bytes(out)


def save_png(path, w, h, rgba):
    from PIL import Image
    os.makedirs(os.path.dirname(path), exist_ok=True)
    Image.frombytes('RGBA', (w, h), rgba).save(path)


def walk_zone(name):
    import zonewalk as zw
    W = zw.Walker(zw.load_zone(os.path.join(ZONES, name + '.ff')))
    W.walk()
    return W
