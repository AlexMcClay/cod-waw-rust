"""Dump menuDef_t assets of a zone as readable .menu-like text (game-derived -> local/).
Usage: py dump_menus.py <zone> [menuName ...]       e.g. py dump_menus.py common weaponinfo compass
Operator names come from OAT's T4_Assets.h operationEnum (ASSUMED to match the T4 PC binary)."""
import sys, os, re, struct
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 't4'))
import zonewalk as zw
from zonewalk import G, unwrap, Rec, PtrArr
ROOT = r"D:\SteamLibrary\steamapps\common\Call of Duty World at War\zone\english"

# operator names from OAT T4_Assets.h
_src = open(os.path.join(HERE, '..', 't4', 'ref', 'T4_Assets.h'), encoding='utf-8', errors='replace').read()
_body = _src[_src.index('enum operationEnum'):]
_body = _body[_body.index('{') + 1:_body.index('};')]
OPS = []
for _line in _body.splitlines():
    _m = re.match(r'\s*(OP_\w+|NUM_OPERATORS)', _line)
    if _m and _m.group(1) != 'OP_FIRSTFUNCTIONCALL':
        OPS.append(_m.group(1))
SYM = {'OP_RIGHTPAREN': ')', 'OP_LEFTPAREN': '(', 'OP_MULTIPLY': '*', 'OP_DIVIDE': '/', 'OP_MODULUS': '%', 'OP_ADD': '+',
       'OP_SUBTRACT': '-', 'OP_NOT': '!', 'OP_LESSTHAN': '<', 'OP_LESSTHANEQUALTO': '<=', 'OP_GREATERTHAN': '>',
       'OP_GREATERTHANEQUALTO': '>=', 'OP_EQUALS': '==', 'OP_NOTEQUAL': '!=', 'OP_AND': '&&', 'OP_OR': '||', 'OP_COMMA': ',',
       'OP_BITWISEAND': '&', 'OP_BITWISEOR': '|', 'OP_BITWISENOT': '~', 'OP_BITSHIFTLEFT': '<<', 'OP_BITSHIFTRIGHT': '>>'}
ITEM_TYPES = ['text', 'button', 'radiobutton', 'checkbox', 'editfield', 'combo', 'listbox', 'model', 'ownerdraw',
              'numericfield', 'slider', 'yesno', 'multi', 'dvarenum', 'bind', 'menumodel', 'validfilefield',
              'decimalfield', 'upreditfield', 'game_message_window']
# horzAlign / vertAlign values (HORIZONTAL_ALIGN_* / VERTICAL_ALIGN_* of menudefinition.h)
ALIGN = {0: 'SUBLEFT|SUBTOP', 1: 'LEFT|TOP', 2: 'CENTER', 3: 'RIGHT|BOTTOM', 4: 'FULLSCREEN', 5: 'NOSCALE',
         6: 'TO640|TO480', 7: 'CENTER_SAFEAREA'}


def s(W, v):
    return W.cstr(unwrap(v))


def expr(W, st):
    st = unwrap(st)
    if not isinstance(st, Rec):
        return None
    n = G(st, 'numEntries')
    if not n:
        return None
    ents = unwrap(G(st, 'entries'))
    out = []
    for i in range(n):
        e = unwrap(ents.get(i)) if isinstance(ents, PtrArr) else None
        if not isinstance(e, Rec):
            out.append('?')
            continue
        typ = struct.unpack_from('<i', e.buf, 0)[0]
        if typ == 0:
            op = struct.unpack_from('<i', e.buf, 4)[0]
            nm = OPS[op] if 0 <= op < len(OPS) else 'OP_%d' % op
            out.append(SYM.get(nm, nm[3:].lower()))
        else:
            dt = struct.unpack_from('<i', e.buf, 4)[0]
            if dt == 0:
                out.append(str(struct.unpack_from('<i', e.buf, 8)[0]))
            elif dt == 1:
                out.append('%g' % struct.unpack_from('<f', e.buf, 8)[0])
            else:
                sv = G(e, 'data', 'operand', 'internals', 'stringVal')
                out.append('"%s"' % s(W, sv))
    return ' '.join(out)


def rect(r):
    return '%g %g %g %g  align %s %s' % (G(r, 'x'), G(r, 'y'), G(r, 'w'), G(r, 'h'),
                                         ALIGN.get(G(r, 'horzAlign'), G(r, 'horzAlign')),
                                         ALIGN.get(G(r, 'vertAlign'), G(r, 'vertAlign')))


def col(c):
    return '(%g %g %g %g)' % tuple(round(x, 4) for x in c)


def mat(W, m):
    m = unwrap(m)
    if isinstance(m, Rec):
        nm = s(W, G(m, 'info', 'name'))
        try:
            import t4assets as ta
            im = ta.color_map(ta.material_info(W, m))
        except Exception:
            im = None
        return '%s [image %s]' % (nm, im)
    return None if not m else repr(m)


def window(W, w, ind, f):
    f.write('%sname "%s"  rect %s   (rectClient %s)\n' % (ind, s(W, G(w, 'name')), rect(G(w, 'rect')), rect(G(w, 'rectClient'))))
    g = s(W, G(w, 'group'))
    if g:
        f.write('%sgroup %s\n' % (ind, g))
    f.write('%sstyle %d border %d ownerdraw %d ownerdrawFlags %#x borderSize %g staticFlags %#x\n' % (
        ind, G(w, 'style'), G(w, 'border'), G(w, 'ownerDraw'), G(w, 'ownerDrawFlags'), G(w, 'borderSize'), G(w, 'staticFlags')))
    f.write('%sforecolor %s backcolor %s bordercolor %s outlinecolor %s\n' % (
        ind, col(G(w, 'foreColor')), col(G(w, 'backColor')), col(G(w, 'borderColor')), col(G(w, 'outlineColor'))))
    b = mat(W, G(w, 'background'))
    if b:
        f.write('%sbackground %s\n' % (ind, b))


def dump_menu(W, m, f):
    f.write('menuDef {\n')
    window(W, G(m, 'window'), '  ', f)
    f.write('  font "%s" fullScreen %d itemCount %d fontIndex %d fadeCycle %d fadeClamp %g fadeAmount %g fadeInAmount %g blur %g\n' % (
        s(W, G(m, 'font')), G(m, 'fullScreen'), G(m, 'itemCount'), G(m, 'fontIndex'), G(m, 'fadeCycle'), G(m, 'fadeClamp'),
        G(m, 'fadeAmount'), G(m, 'fadeInAmount'), G(m, 'blurRadius')))
    for k in ('onOpen', 'onClose', 'onESC', 'allowedBinding', 'soundName'):
        v = s(W, G(m, k))
        if v:
            f.write('  %s %s\n' % (k, v.strip()))
    for k in ('visibleExp', 'rectXExp', 'rectYExp'):
        e = expr(W, G(m, k))
        if e:
            f.write('  %s: %s\n' % (k, e))
    items = unwrap(G(m, 'items'))
    for i in range(G(m, 'itemCount')):
        it = unwrap(items.get(i))
        f.write('  itemDef {   // #%d\n' % i)
        window(W, G(it, 'window'), '    ', f)
        tr = G(it, 'textRect', 0)
        typ = G(it, 'type')
        f.write('    type %s dataType %d alignment %d fontEnum %d textAlignMode %#x textalignx %g textaligny %g textscale %g textStyle %d\n' % (
            ITEM_TYPES[typ] if 0 <= typ < len(ITEM_TYPES) else typ, G(it, 'dataType'), G(it, 'alignment'), G(it, 'fontEnum'),
            G(it, 'textAlignMode'), G(it, 'textalignx'), G(it, 'textaligny'), G(it, 'textscale'), G(it, 'textStyle')))
        f.write('    textRect %s  itemFlags %#x dvarFlags %#x special %g imageTrack %d\n' % (
            rect(tr), G(it, 'itemFlags'), G(it, 'dvarFlags'), G(it, 'special'), G(it, 'imageTrack')))
        for k in ('text', 'dvar', 'dvarTest', 'enableDvar', 'action', 'onFocus', 'leaveFocus', 'mouseEnter', 'mouseExit'):
            v = s(W, G(it, k))
            if v:
                f.write('    %s "%s"\n' % (k, v.strip()))
        for k in ('visibleExp', 'textExp', 'materialExp', 'rectXExp', 'rectYExp', 'rectWExp', 'rectHExp', 'forecolorAExp'):
            e = expr(W, G(it, k))
            if e:
                f.write('    %s: %s\n' % (k, e))
        f.write('  }\n')
    f.write('}\n\n')


def main():
    zn = sys.argv[1]
    want = set(sys.argv[2:])
    W = zw.Walker(zw.load_zone(os.path.join(ROOT, zn + '.ff')))
    W.walk()
    out = os.path.join(HERE, 'local', 'menus_%s.txt' % zn)
    with open(out, 'w', encoding='utf-8') as f:
        for a in W.assets:
            if a[0] == 'menu' and (not want or a[1] in want):
                f.write('// ===== %s (zone %s, fpos %d)\n' % (a[1], zn, a[3]))
                try:
                    dump_menu(W, unwrap(a[2]), f)
                except Exception as e:
                    import traceback
                    f.write('// ERROR %r %s\n' % (e, traceback.format_exc().replace('\n', ' | ')))
    print('wrote', out)


if __name__ == '__main__':
    main()
