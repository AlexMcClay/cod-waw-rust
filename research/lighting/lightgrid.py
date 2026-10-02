"""
lightgrid.py - decode and sample the Nacht GfxLightGrid (T4 / WaW PC).

Encoding (VERIFIED on all 189 non-empty rows of nazi_zombie_prototype: column totals == row colCount,
entry totals == firstEntry deltas, table padding < 4 zero bytes):

  grid coords: g = (floor(x/32) + 4096, floor(y/32) + 4096, floor(z/64) + 2048)      [ASSUMED spacing/offset,
               consistent with mins/maxs vs the world bbox and with the sanity checks printed below]
  row  = g[rowAxis] - mins[rowAxis]       (rowAxis = 1 -> Y here); rowDataStart[row] == 0xFFFF -> empty row
  rowp = rawRowData + 4 * rowDataStart[row]
  row header: u16 colStart, u16 colCount, u16 zStart, u16 zCount, u32 firstEntry      (12 bytes)
  then runs until colCount columns are covered:
      u8 nCols, u8 nZ, [u8 zOffset  only if nZ > 0]
      the run covers nCols consecutive columns; each column has nZ entries for z = zStart+zOffset .. +nZ-1
      (column-major: entry = runFirst + colInRun * nZ + (z - zStart - zOffset)); nZ == 0 -> empty columns (skip)
  entries[i] = {u16 colorsIndex, u8 primaryLightIndex, u8 needsTrace}
  colors[colorsIndex].rgb[56][3] = u8 RGB of the 56 boundary texels of a 4x4x4 cube (4^3 - 2^3 = 56)

Usage: py lightgrid.py            -> prints stats + sample points, writes local/lightgrid_samples.txt
"""
import os, sys, struct, math
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))

# zone offsets of the GfxLightGrid arrays in research/t4/zone.bin (from world_light.py / the walker)
ZONE = os.path.join(HERE, '..', 't4', 'zone.bin')
HDR = dict(mins=(3933, 3955, 2047), maxs=(4247, 4219, 2068), rowAxis=1, colAxis=0,
           rowDataStart=51300850, rawRowData=51301380, rawRowDataSize=8484,
           entries=51309864, entryCount=18105, colors=51382284, colorCount=3526)

# the 56 boundary cells of a 4x4x4 cube, x fastest then y then z (ASSUMED order; see WAW_LIGHTING.md)
CUBE56 = [(x, y, z) for z in range(4) for y in range(4) for x in range(4)
          if not (1 <= x <= 2 and 1 <= y <= 2 and 1 <= z <= 2)]
assert len(CUBE56) == 56


class LightGrid:
    def __init__(s, zone_path=ZONE, hdr=HDR):
        z = open(zone_path, 'rb').read()
        s.h = hdr
        nrows = hdr['maxs'][hdr['rowAxis']] - hdr['mins'][hdr['rowAxis']] + 1
        s.rds = struct.unpack('<%dH' % nrows, z[hdr['rowDataStart']:hdr['rowDataStart'] + 2 * nrows])
        s.raw = z[hdr['rawRowData']:hdr['rawRowData'] + hdr['rawRowDataSize']]
        e = np.frombuffer(z[hdr['entries']:hdr['entries'] + 4 * hdr['entryCount']], np.uint8).reshape(-1, 4)
        s.colorsIndex = e[:, 0].astype(np.int32) | (e[:, 1].astype(np.int32) << 8)
        s.primaryLight = e[:, 2].copy(); s.needsTrace = e[:, 3].copy()
        s.colors = np.frombuffer(z[hdr['colors']:hdr['colors'] + 168 * hdr['colorCount']], np.uint8).reshape(-1, 56, 3)

    @staticmethod
    def grid_coord(p):
        return (math.floor(p[0] / 32.0) + 4096, math.floor(p[1] / 32.0) + 4096, math.floor(p[2] / 64.0) + 2048)

    def entry_index(s, g):
        h = s.h
        ra, ca = h['rowAxis'], h['colAxis']
        row = g[ra] - h['mins'][ra]
        if row < 0 or row >= len(s.rds) or s.rds[row] == 0xFFFF: return None
        o = 4 * s.rds[row]
        cs, cc, zs, zc, fe = struct.unpack_from('<4HI', s.raw, o)
        col = g[ca] - cs; zi = g[2] - zs
        if not (0 <= col < cc and 0 <= zi < zc): return None
        p = o + 12; entry = fe
        while True:
            n, k = s.raw[p], s.raw[p + 1]; p += 2
            zo = 0
            if k: zo = s.raw[p]; p += 1
            if col < n:
                if k == 0 or not (zo <= zi < zo + k): return None
                return entry + col * k + (zi - zo)
            col -= n; entry += n * k

    def all_cells(s):
        """yield (gx, gy, gz, entryIndex) for every stored cell"""
        h = s.h
        for row, st in enumerate(s.rds):
            if st == 0xFFFF: continue
            o = 4 * st
            cs, cc, zs, zc, fe = struct.unpack_from('<4HI', s.raw, o)
            p = o + 12; entry = fe; col = 0
            while col < cc:
                n, k = s.raw[p], s.raw[p + 1]; p += 2
                zo = 0
                if k: zo = s.raw[p]; p += 1
                for c in range(n):
                    for zz in range(k):
                        g = [0, 0, 0]
                        g[h['rowAxis']] = h['mins'][h['rowAxis']] + row; g[h['colAxis']] = cs + col + c; g[2] = zs + zo + zz
                        yield g[0], g[1], g[2], entry + c * k + zz
                col += n; entry += n * k

    def cube(s, ei):
        """4x4x4x3 float array (0..1 = u8/255) of one entry; interior cells = nan"""
        cub = np.full((4, 4, 4, 3), np.nan)
        cols = s.colors[s.colorsIndex[ei]] / 255.0
        for i, (x, y, z) in enumerate(CUBE56): cub[x, y, z] = cols[i]
        return cub

    def sample_dir(s, ei, n):
        """what the model shader reads for world normal n (nearest texel; shader multiplies by 2):
        coord = n / max|n_i| * 1.5 texels around the 4x4x4 block centre (ASSUMED lookup scale)"""
        n = np.asarray(n, float); m = np.abs(n).max()
        t = n / m * 1.5 + 1.5                      # texel-centre space 0..3
        i = np.clip(np.rint(t).astype(int), 0, 3)
        return s.cube(ei)[i[0], i[1], i[2]]


def cube_lookup(cub, n):
    """trilinear read of a 4x4x4 cube at coord n/max|n_i| * 1.5 + 1.5 (what the volume-texture fetch does)"""
    c = np.nan_to_num(cub)
    n = np.asarray(n, float); t = n / np.abs(n).max() * 1.5 + 1.5
    i0 = np.clip(np.floor(t).astype(int), 0, 2); f = t - i0
    out = np.zeros(3)
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (f[0] if dx else 1 - f[0]) * (f[1] if dy else 1 - f[1]) * (f[2] if dz else 1 - f[2])
                if w: out += w * c[i0[0] + dx, i0[1] + dy, i0[2] + dz]
    return out


def model_lighting(G, pos, n):
    """2 * cube(n) of the grid blended at pos: trilinear over the 8 surrounding grid points, missing points
    dropped and weights renormalised (ASSUMED; the engine also traces visibility for 'needsTrace' entries).
    Returns (rgb, primaryLightIndex of the nearest found corner)."""
    sp = np.array([32.0, 32.0, 64.0]); off = np.array([4096, 4096, 2048])
    q = np.asarray(pos, float) / sp
    g0 = np.floor(q).astype(int); f = q - g0
    acc = np.zeros((4, 4, 4, 3)); wsum = 0.0; best = (-1, None)
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (f[0] if dx else 1 - f[0]) * (f[1] if dy else 1 - f[1]) * (f[2] if dz else 1 - f[2])
                ei = G.entry_index(tuple(int(v) for v in g0 + off + (dx, dy, dz)))
                if ei is None or w <= 0: continue
                acc += w * np.nan_to_num(G.cube(ei)); wsum += w
                if w > best[0]: best = (w, int(G.primaryLight[ei]))
    if wsum == 0: return None, None
    return 2.0 * cube_lookup(acc / wsum, n), best[1]


def lum(c): return 0.299 * c[..., 0] + 0.587 * c[..., 1] + 0.114 * c[..., 2]


def main():
    G = LightGrid()
    h = G.h
    print('rows', len(G.rds), 'non-empty', sum(1 for r in G.rds if r != 0xFFFF), 'entries', len(G.colorsIndex),
          'colors', len(G.colors), 'max colorsIndex', G.colorsIndex.max())
    cells = list(G.all_cells())
    assert sorted(c[3] for c in cells) == list(range(h['entryCount'])), 'cells do not cover entries exactly'
    print('all_cells covers every entry exactly once:', len(cells))
    for gx, gy, gz, ei in cells[:3] + cells[-3:]:
        assert G.entry_index((gx, gy, gz)) == ei
    ok = sum(G.entry_index((gx, gy, gz)) == ei for gx, gy, gz, ei in cells)
    print('entry_index round trip ok:', ok, '/', len(cells))
    xs = [(c[0] - 4096) * 32 for c in cells]; ys = [(c[1] - 4096) * 32 for c in cells]; zs = [(c[2] - 2048) * 64 for c in cells]
    print('cell world extents (min corner): x %d..%d y %d..%d z %d..%d' % (min(xs), max(xs), min(ys), max(ys), min(zs), max(zs)))
    import collections
    print('entry primaryLightIndex histogram', sorted(collections.Counter(G.primaryLight.tolist()).items()))
    print('needsTrace histogram', sorted(collections.Counter(G.needsTrace.tolist()).items()))
    # per-slot mean luminance (order sanity): layers of the slowest axis
    L = lum(G.colors / 255.0).mean(axis=0)
    layer = [np.mean([L[i] for i, c in enumerate(CUBE56) if c[2] == zz]) for zz in range(4)]
    print('mean luminance per cube z-layer (bottom..top):', np.round(layer, 4))
    for ax in range(3):
        lo = np.mean([L[i] for i, c in enumerate(CUBE56) if c[ax] == 0]); hi = np.mean([L[i] for i, c in enumerate(CUBE56) if c[ax] == 3])
        print('  axis %d face mean lum: -face %.4f  +face %.4f' % (ax, lo, hi))
    out = []
    pts = {'start room (-37,202,40)': (-37, 202, 40), 'start room high (-37,202,100)': (-37, 202, 100),
           'outside courtyard (600,1300,40)': (600, 1300, 40), 'far outside (2000,0,200)': (2000, 0, 200),
           'near orange spot light 2 (-288,-88,64)': (-288, -88, 64), 'near orange omni 16 (110,-960,76)': (110, -960, 76),
           'help room (300,700,40)': (300, 700, 40), 'upstairs (300,500,180)': (300, 500, 180)}
    for name, p in pts.items():
        g = G.grid_coord(p); ei = G.entry_index(g)
        if ei is None:
            line = '%-42s grid %s -> no entry' % (name, g)
        else:
            up = G.sample_dir(ei, (0, 0, 1)); dn = G.sample_dir(ei, (0, 0, -1)); px = G.sample_dir(ei, (1, 0, 0)); nx = G.sample_dir(ei, (-1, 0, 0))
            avg = G.colors[G.colorsIndex[ei]].mean(axis=0) / 255.0
            line = ('%-42s grid %s entry %5d primaryLight %2d needsTrace %d avg %s up %s down %s +x %s -x %s'
                    % (name, g, ei, G.primaryLight[ei], G.needsTrace[ei], np.round(avg, 3), np.round(up, 3), np.round(dn, 3),
                       np.round(px, 3), np.round(nx, 3)))
        print(line); out.append(line)
    print('model lighting (2*cube, trilinear over 8 grid points) for up / down / +x / -x normals:')
    for name, p in {'start room standing zombie (-37,202,40)': (-37, 202, 40), 'help room (300,700,40)': (300, 700, 40),
                    'courtyard (600,1300,40)': (600, 1300, 40), 'next to orange omni 17 (139,-94,40)': (139, -94, 40),
                    'next to tungsten 20 (208,430,40)': (208, 430, 40)}.items():
        res = [model_lighting(G, p, n) for n in ((0, 0, 1), (0, 0, -1), (1, 0, 0), (-1, 0, 0))]
        line = '%-42s primary %s  up %s down %s +x %s -x %s' % (name, res[0][1], *[np.round(r[0], 3) if r[0] is not None else None for r in res])
        print(line); out.append(line)
    os.makedirs(os.path.join(HERE, 'local'), exist_ok=True)
    open(os.path.join(HERE, 'local', 'lightgrid_samples.txt'), 'w').write('\n'.join(out) + '\n')


if __name__ == '__main__':
    main()
