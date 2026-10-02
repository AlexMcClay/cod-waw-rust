"""Decode every Font_s in code_post_gfx.ff: glyph tables -> local/fonts/<font>.json, atlas image -> local/fonts/<image>.png,
and a text-rendering test (local/fonts/sample_<font>.png) using the glyph metrics exactly as documented in WAW_ZOMBIE_HUD.md.
Usage: py decode_font.py [zone]   (default code_post_gfx)"""
import sys, os, json, struct
import hudlib
from zonewalk import G, unwrap, Data
import t4assets as ta

GLYPH = struct.Struct('<HbbBBBxffff')   # letter, x0, y0, dx, pixelWidth, pixelHeight, pad, s0, t0, s1, t1 (24 bytes)


def read_font(W, rec):
    f = unwrap(rec)
    n = G(f, 'glyphCount')
    g = unwrap(G(f, 'glyphs'))
    raw = W.z[g.fpos:g.fpos + 24 * n] if isinstance(g, Data) else ta.data_bytes(W, g, 24 * n)
    glyphs = []
    for i in range(n):
        L, x0, y0, dx, pw, ph, s0, t0, s1, t1 = GLYPH.unpack_from(raw, 24 * i)
        glyphs.append(dict(letter=L, char=chr(L), x0=x0, y0=y0, dx=dx, pixelWidth=pw, pixelHeight=ph,
                           s0=s0, t0=t0, s1=s1, t1=t1))
    mi = ta.material_info(W, G(f, 'material'))
    gi = ta.material_info(W, G(f, 'glowMaterial'))
    img = mi['textures'][0]['image'] if mi.get('textures') else None
    return dict(fontName=W.cstr(G(f, 'fontName')), pixelHeight=G(f, 'pixelHeight'), glyphCount=n,
                material=mi['name'], materialTechset=mi['techniqueSet'], glowMaterial=gi.get('name'),
                image=img and img['name'], imageWidth=img and img['width'], imageHeight=img and img['height'],
                imageInlinePixels=img and img['inline_pixels'], glyphs=glyphs)


def render(font, atlas, text, scale=1.0, color=(255, 255, 255)):
    """CoD-style text layout: pen starts at x=0 on the baseline; per glyph quad at
    (pen + x0*s, baseline + y0*s) size (pixelWidth*s, pixelHeight*s), uv (s0,t0)-(s1,t1); pen += dx*s.
    Missing glyphs fall back to '?' (like the engine)."""
    from PIL import Image
    gl = {g['letter']: g for g in font['glyphs']}
    ph = font['pixelHeight']
    width = int(sum(gl.get(ord(c), gl.get(ord('?')))['dx'] for c in text) * scale) + 8
    H = int(ph * scale * 1.6) + 8
    im = Image.new('RGBA', (width, H), (40, 40, 40, 255))
    base = int(ph * scale) + 4                     # baseline y
    pen = 4.0
    AW, AH = atlas.size
    for c in text:
        g = gl.get(ord(c)) or gl.get(ord('?'))
        if g['pixelWidth'] and g['pixelHeight']:
            box = (int(round(g['s0'] * AW)), int(round(g['t0'] * AH)), int(round(g['s1'] * AW)), int(round(g['t1'] * AH)))
            gimg = atlas.crop(box).resize((max(1, int(round(g['pixelWidth'] * scale))), max(1, int(round(g['pixelHeight'] * scale)))), Image.BILINEAR)
            a = gimg.split()[3]
            tint = Image.new('RGBA', gimg.size, color + (255,))
            tint.putalpha(a)
            im.alpha_composite(tint, (int(round(pen + g['x0'] * scale)), int(round(base + g['y0'] * scale))))
        pen += g['dx'] * scale
    return im


def main():
    from PIL import Image
    zone = sys.argv[1] if len(sys.argv) > 1 else 'code_post_gfx'
    W = hudlib.walk_zone(zone)
    out = os.path.join(hudlib.LOCAL, 'fonts')
    os.makedirs(out, exist_ok=True)
    atlases = {}
    summary = []
    for a in W.assets:
        if a[0] != 'font':
            continue
        fd = read_font(W, a[2])
        nm = fd['fontName'].split('/')[-1]
        json.dump(fd, open(os.path.join(out, nm + '.json'), 'w'), indent=1)
        if fd['image'] not in atlases:
            data = hudlib.read_iwi(fd['image'])
            fmt, w, h, rgba = hudlib.decode_iwi(data)
            hudlib.save_png(os.path.join(out, fd['image'] + '.png'), w, h, rgba)
            atlases[fd['image']] = (Image.frombytes('RGBA', (w, h), rgba), fmt, hudlib.iwd_index()[fd['image']])
        atlas, fmt, src = atlases[fd['image']]
        sample = render(fd, atlas, 'Thompson 128 Round 1510 +60 Press & hold F to buy [Cost: 1500]', 2.0)
        sample.save(os.path.join(out, 'sample_%s.png' % nm))
        lo = min((g for g in fd['glyphs'] if g['pixelHeight']), key=lambda g: g['y0'])
        summary.append('%-22s pixelHeight %2d glyphs %3d material %-24s glow %-28s image %s (%dx%d, %s, from %s: %s)  min y0 %d (%r)' % (
            fd['fontName'], fd['pixelHeight'], fd['glyphCount'], fd['material'], fd['glowMaterial'], fd['image'],
            atlas.size[0], atlas.size[1], fmt, os.path.basename(src[0]), src[1], lo['y0'], lo['char']))
    open(os.path.join(out, 'summary.txt'), 'w').write('\n'.join(summary) + '\n')
    print('\n'.join(summary))


if __name__ == '__main__':
    main()
