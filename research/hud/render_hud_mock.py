"""Compose a 640x480 (virtual coordinates, scaled x2) mock of the Nacht solo HUD from the real assets, using the
positions documented in WAW_ZOMBIE_HUD.md. Values marked ASSUMED there are marked ASSUMED here too.
Writes local/hud_mock.png (game-derived, git-ignored).   Usage: py render_hud_mock.py"""
import os, json
from PIL import Image
import hudlib

S = 2                    # output pixels per virtual unit (640x480 -> 1280x960)
W, H = 640, 480
RED = (108, 1, 0)        # (0.423, 0.004, 0) * 255
YELLOW = (230, 230, 0)   # (0.9, 0.9, 0.0)


def img(name):
    fmt, w, h, rgba = hudlib.decode_iwi(hudlib.read_iwi(name))
    return Image.frombytes('RGBA', (w, h), rgba)


def tint(im, rgb, alpha=1.0):
    r, g, b, a = im.split()
    out = Image.new('RGBA', im.size, rgb + (255,))
    out.putalpha(a.point(lambda v: int(v * alpha)))
    return out


def blit(canvas, im, x, y, w, h, rgb=(255, 255, 255), alpha=1.0):
    im = im.resize((max(1, int(w * S)), max(1, int(h * S))), Image.BILINEAR)
    canvas.alpha_composite(tint(im, rgb, alpha), (int(x * S), int(y * S)))


FONTS = {}


def font(n):
    if n not in FONTS:
        FONTS[n] = json.load(open(os.path.join(hudlib.LOCAL, 'fonts', n + '.json')))
    return FONTS[n]


ATLAS = None


def text_width(fn, s, height):
    f = font(fn); sc = height / f['pixelHeight']
    gl = {g['letter']: g for g in f['glyphs']}
    return sum((gl.get(ord(c)) or gl[63])['dx'] for c in s) * sc


def draw_text(canvas, fn, s, x, y_bottom, height, rgb, align='left', alpha=1.0, shadow=False):
    """height = virtual height of the font's pixelHeight box; y_bottom = bottom of that box (the text 'y')."""
    global ATLAS
    if ATLAS is None:
        ATLAS = img('gamefonts_pc')
    f = font(fn); sc = height / f['pixelHeight']
    gl = {g['letter']: g for g in f['glyphs']}
    if align == 'right':
        x -= text_width(fn, s, height)
    elif align == 'center':
        x -= text_width(fn, s, height) / 2
    if shadow:   # ASSUMED: textStyle 3 = black drop shadow, 1 virtual unit down-right
        draw_text(canvas, fn, s, x + 1, y_bottom + 1, height, (0, 0, 0), 'left', alpha * 0.75)
    pen = x
    AW, AH = ATLAS.size
    for c in s:
        g = gl.get(ord(c)) or gl[63]
        if g['pixelWidth']:
            box = (round(g['s0'] * AW), round(g['t0'] * AH), round(g['s1'] * AW), round(g['t1'] * AH))
            blit(canvas, ATLAS.crop(box), pen + g['x0'] * sc, y_bottom + g['y0'] * sc, g['pixelWidth'] * sc, g['pixelHeight'] * sc, rgb, alpha)
        pen += g['dx'] * sc


def main():
    bg = Image.new('RGBA', (W * S, H * S), (38, 40, 44, 255))
    # 1. chalk tally, round 5: hud_chalk_5 64x64, left/bottom aligned, x=-5 (left after the intro move), y=0
    blit(bg, img('chalkmarks_5'), -5, H - 64, 64, 64, RED)
    # 2. score (engine ownerdraw 288 at -103,-71 right/bottom). Bar size/placement ASSUMED (8:1 image, 18-unit rows)
    bx, by = W - 103, H - 71
    blit(bg, img('scorebar_zom_1'), bx - 4, by - 10, 112, 20, RED, 0.8)
    draw_text(bg, 'normalFont', '1510', bx + 8, by + 8, 48 * 0.3095, (255, 255, 255))      # ASSUMED font/size
    # 3. point popups: right-aligned at x=-103-[20..59], y=-71+[-14..15], fontScale 8 -> ASSUMED 16 units high
    draw_text(bg, 'normalFont', '+10', bx - 22, by + 8 - 6, 16, YELLOW, 'right')
    draw_text(bg, 'normalFont', '+60', bx - 50, by + 8 + 7, 16, YELLOW, 'right', 0.6)
    # 4. weapon name: ownerdraw 81, rect -305 -40 290 40 right/bottom, objectiveFont, textscale 0.3095, white a=.75
    draw_text(bg, 'objectiveFont', 'Thompson', W - 15, H - 40, 48 * 0.3095, (255, 255, 255), 'right', 0.75, True)
    # 5. offhand frag icon (ownerdraw 103) rect -104 -38 24 24, colour (1,1,1,.65); count (105) at -84 -8
    blit(bg, img('hud_us_grenade'), W - 104, H - 38, 24, 24, (255, 255, 255), 0.65)
    draw_text(bg, 'objectiveFont', '3', W - 84, H - 8, 48 * 0.3095, (255, 255, 255), 'left', 0.75, True)
    # 6. clip graphic (ownerdraw 117) anchored at -79 -4; ASSUMED: one ammo_counter_bullet (4x8) per round, leftwards
    bullet = img('ammo_counter_bullet')
    for i in range(20):
        blit(bg, bullet, W - 79 - 4 * (i + 1) - 1 * i, H - 4 - 8, 4, 8, (255, 255, 255), 0.65)
    # 7. ammo stock (ownerdraw 119) rect -75 4 25 25 -> text 'y' (bottom of the pixelHeight box) ASSUMED = rect y;
    #    digits end ~3.2 units above that, i.e. ~0.8 units below the screen edge
    draw_text(bg, 'objectiveFont', '128', W - 75, H + 4, 48 * 0.3095, (255, 255, 255), 'left', 0.75, True)
    # 8. low-ammo "Reload" (ownerdraw 120) rect -10 15 100 30 centre/centre, MIDDLE_CENTER text, smallFont auto-pick
    draw_text(bg, 'normalFont', 'Reload', W / 2, H / 2 + 15 + 15 + 7, 48 * 0.3095, (255, 255, 255), 'center', 1.0, True)
    # 9. cursor hint (ownerdraw 72) rect 0 70 40 40 centre/centre
    draw_text(bg, 'smallFont', 'Press & hold F to buy Thompson [Cost: 1500]', W / 2, H / 2 + 70 + 20, 48 * 0.3095, (255, 255, 255), 'center', 1.0, True)
    # 10. powerup text: objectiveFont fontScale 2 (24 units), TOP centred at y=350 / 380, Max Ammo at 290
    draw_text(bg, 'objectiveFont', 'Double Points: 27', W / 2, 350 + 24, 24, (255, 255, 255), 'center')
    # crosshair (reticle_side_small 8x8 x4 around the centre, ASSUMED spread 10)
    side = img('side_small')
    for dx, dy, rot in ((-14, 0, 90), (14, 0, 90), (0, -14, 0), (0, 14, 0)):
        im = side.rotate(rot, expand=True)
        blit(bg, im, W / 2 + dx - 4, H / 2 + dy - 4, 8, 8, (255, 255, 255), 0.8)
    out = os.path.join(hudlib.LOCAL, 'hud_mock.png')
    bg.save(out)
    print('wrote', out)


if __name__ == '__main__':
    main()
