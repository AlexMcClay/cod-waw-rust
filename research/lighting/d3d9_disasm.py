"""
d3d9_disasm.py - minimal Direct3D 9 shader-model 2/3 bytecode disassembler (pure Python, no dependencies).

Written from the public token format (D3D9 driver docs: version token, instruction token = opcode in bits 0-15,
controls in 16-23, length in 24-27; parameter tokens = register number 0-10, type bits 28-30 + 11-12,
write mask 16-19 / swizzle 16-23, modifiers; comment blocks incl. the CTAB constant table).
Used as a fallback by disasm_all.py when the Windows SDK's fxc.exe (/dumpbin) is unavailable, and as a
cross-check: `py d3d9_disasm.py --check <file.pso> <fxc listing>` compares the instruction streams.

Usage: py d3d9_disasm.py <shader.vso|.pso>
"""
import struct, sys, re

OPS = {0: 'nop', 1: 'mov', 2: 'add', 3: 'sub', 4: 'mad', 5: 'mul', 6: 'rcp', 7: 'rsq', 8: 'dp3', 9: 'dp4', 10: 'min',
       11: 'max', 12: 'slt', 13: 'sge', 14: 'exp', 15: 'log', 16: 'lit', 17: 'dst', 18: 'lrp', 19: 'frc', 20: 'm4x4',
       21: 'm4x3', 22: 'm3x4', 23: 'm3x3', 24: 'm3x2', 25: 'call', 26: 'callnz', 27: 'loop', 28: 'ret', 29: 'endloop',
       30: 'label', 31: 'dcl', 32: 'pow', 33: 'crs', 34: 'sgn', 35: 'abs', 36: 'nrm', 37: 'sincos', 38: 'rep',
       39: 'endrep', 40: 'if', 41: 'if', 42: 'else', 43: 'endif', 44: 'break', 45: 'break', 46: 'mova', 47: 'defb',
       48: 'defi', 64: 'texcoord', 65: 'texkill', 66: 'texld', 67: 'texbem', 68: 'texbeml', 69: 'texreg2ar',
       70: 'texreg2gb', 71: 'texm3x2pad', 72: 'texm3x2tex', 73: 'texm3x3pad', 74: 'texm3x3tex', 76: 'texm3x3spec',
       77: 'texm3x3vspec', 78: 'expp', 79: 'logp', 80: 'cnd', 81: 'def', 82: 'texreg2rgb', 83: 'texdp3tex',
       84: 'texm3x2depth', 85: 'texdp3', 86: 'texm3x3', 87: 'texdepth', 88: 'cmp', 89: 'bem', 90: 'dp2add', 91: 'dsx',
       92: 'dsy', 93: 'texldd', 94: 'setp', 95: 'texldl', 96: 'break_pred'}
NO_DST = {'if', 'else', 'endif', 'rep', 'endrep', 'loop', 'endloop', 'break', 'call', 'callnz', 'ret', 'label', 'texkill', 'nop'}
CMP = {1: '_gt', 2: '_eq', 3: '_ge', 4: '_lt', 5: '_ne', 6: '_le'}
USAGE = ['position', 'blendweight', 'blendindices', 'normal', 'psize', 'texcoord', 'tangent', 'binormal',
         'tessfactor', 'positiont', 'color', 'fog', 'depth', 'sample']
TEXTYPE = {2: '2d', 3: 'cube', 4: 'volume'}
SRCMOD = {0: '%s', 1: '-%s', 2: '%s_bias', 3: '-%s_bias', 4: '%s_bx2', 5: '-%s_bx2', 6: '1-%s', 7: '%s_x2',
          8: '-%s_x2', 9: '%s_dz', 10: '%s_dw', 11: '%s_abs', 12: '-%s_abs', 13: '!%s'}


def regname(tok, ps, ver_major):
    t = ((tok >> 28) & 7) | ((tok >> 8) & 0x18)
    n = tok & 0x7FF
    if t == 0: return 'r%d' % n
    if t == 1: return 'v%d' % n
    if t == 2: return 'c%d' % n
    if t == 3: return ('t%d' if ps else 'a%d') % n
    if t == 4: return ['oPos', 'oFog', 'oPts'][n] if n < 3 else 'rast%d' % n
    if t == 5: return 'oD%d' % n
    if t == 6: return ('o%d' if ver_major >= 3 else 'oT%d') % n
    if t == 7: return 'i%d' % n
    if t == 8: return 'oC%d' % n
    if t == 9: return 'oDepth'
    if t == 10: return 's%d' % n
    if t in (11, 12, 13): return 'c%d' % (n + 2048 * (t - 10))
    if t == 14: return 'b%d' % n
    if t == 15: return 'aL'
    if t == 17: return ['vPos', 'vFace'][n] if n < 2 else 'misc%d' % n
    if t == 18: return 'l%d' % n
    if t == 19: return 'p%d' % n
    return 'reg%d_%d' % (t, n)


def dst(tok, ps, vm):
    s = regname(tok, ps, vm)
    m = (tok >> 16) & 0xF
    if m != 0xF: s += '.' + ''.join(c for i, c in enumerate('xyzw') if m & (1 << i))
    mod = (tok >> 20) & 0xF
    sfx = ('_sat' if mod & 1 else '') + ('_pp' if mod & 2 else '') + ('_centroid' if mod & 4 else '')
    return s, sfx


def src(tok, ps, vm, rel=None):
    s = regname(tok, ps, vm)
    if rel is not None: s += '[%s]' % rel          # fxc style: c40[a0.w]
    sw = (tok >> 16) & 0xFF
    comps = ''.join('xyzw'[(sw >> (2 * i)) & 3] for i in range(4))
    mod = (tok >> 24) & 0xF
    if mod in (11, 12): s += '_abs'          # fxc style: r0_abs.x / -r0_abs.x
    if comps != 'xyzw':
        if len(set(comps)) == 1: comps = comps[0]
        s += '.' + comps
    if mod == 11: return s
    if mod == 12: return '-' + s
    return SRCMOD.get(mod, '%s?mod').replace('%s', s)


def parse_ctab(words):
    """words: DWORDs of the comment payload starting with 'CTAB' -> list of (name, regset, index, count)"""
    b = struct.pack('<%dI' % len(words), *words)[4:]
    size, creator, version, nconst, cinfo, flags, target = struct.unpack_from('<7I', b, 0)
    out = []
    def cstr(o): return b[o:b.index(b'\0', o)].decode('latin-1')
    for i in range(nconst):
        name, rs, ri, rc, _, ti, dv = struct.unpack_from('<IHHHHII', b, cinfo + 20 * i)
        out.append((cstr(name), rs, ri, rc))
    return out, (cstr(creator) if creator else ''), (cstr(target) if target else '')


def disassemble(code):
    w = struct.unpack('<%dI' % (len(code) // 4), code[:len(code) // 4 * 4])
    ver = w[0]
    ps = (ver >> 16) == 0xFFFF
    vmaj, vmin = (ver >> 8) & 0xFF, ver & 0xFF
    lines = []
    i = 1
    body = ['    %s_%d_%d' % ('ps' if ps else 'vs', vmaj, vmin)]
    while i < len(w):
        t = w[i]
        op = t & 0xFFFF
        if op == 0xFFFE:  # comment
            n = (t >> 16) & 0x7FFF
            payload = w[i + 1:i + 1 + n]
            if payload and payload[0] == 0x42415443:  # 'CTAB'
                consts, creator, target = parse_ctab(list(payload))
                lines.append('// %s  (target %s)' % (creator, target))
                lines.append('// Registers:')
                for name, rs, ri, rc in sorted(consts, key=lambda c: (c[1] != 2, c[2])):
                    lines.append('//   %-32s %s%-4d %d' % (name, 'bics'[rs] if rs < 4 else '?', ri, rc))
            i += 1 + n
            continue
        if t == 0x0000FFFF: break
        ln = (t >> 24) & 0xF
        toks = w[i + 1:i + 1 + ln]
        i += 1 + ln
        name = OPS.get(op, 'op%d' % op)
        ctrl = (t >> 16) & 0xFF
        if op in (41, 45) and ctrl in CMP: name += CMP[ctrl]
        if op == 66 and ctrl == 1: name = 'texldp'
        if op == 66 and ctrl == 2: name = 'texldb'
        pred = bool(t & 0x10000000)
        if op == 31:  # dcl
            u, d = toks[0], toks[1]
            ds, sfx = dst(d, ps, vmaj)
            rt = ((d >> 28) & 7) | ((d >> 8) & 0x18)
            if rt == 10:
                body.append('    dcl_%s %s' % (TEXTYPE.get((u >> 27) & 0xF, '?'), ds))
            else:
                us = USAGE[u & 0x1F] if (u & 0x1F) < len(USAGE) else 'usage%d' % (u & 0x1F)
                ui = (u >> 16) & 0xF
                body.append('    dcl_%s%s%s %s' % (us, ui if ui else '', sfx, ds))
            continue
        if op == 81:  # def
            ds, _ = dst(toks[0], ps, vmaj)
            f = struct.unpack('<4f', struct.pack('<4I', *toks[1:5]))
            body.append('    def %s, %s' % (ds, ', '.join(('%.9g' % x).replace('e-0', 'e-00').replace('e+0', 'e+00') for x in f)))
            continue
        if op in (47, 48):
            ds, _ = dst(toks[0], ps, vmaj)
            vals = [str(x if x < 0x80000000 else x - (1 << 32)) for x in toks[1:]]
            body.append('    %s %s, %s' % (name, ds, ', '.join(vals)))
            continue
        params = list(toks)
        out = []; sfx = ''
        if op == 65:  # texkill takes a destination-format parameter
            body.append('    texkill %s' % dst(params[0], ps, vmaj)[0]); continue
        k = 0
        if name not in NO_DST and op not in (40, 41, 45, 96) and params:
            ds, sfx = dst(params[0], ps, vmaj); out.append(ds); k = 1
        if pred and k < len(params):  # predicate register comes first among sources
            out.insert(0, '(' + src(params[k], ps, vmaj) + ')'); k += 1
        while k < len(params):
            p = params[k]; k += 1
            rel = None
            if p & 0x2000 and k < len(params):  # relative addressing token follows
                rel = src(params[k], ps, vmaj); k += 1
            out.append(src(p, ps, vmaj, rel))
        body.append('    %s%s %s' % (name, sfx, ', '.join(out)) if out else '    ' + name)
    return '\n'.join(lines + [''] + body) + '\n'


def _norm(line):
    return re.sub(r'\s+', ' ', line.strip())


def check(code_path, fxc_listing):
    mine = [_norm(l) for l in disassemble(open(code_path, 'rb').read()).splitlines() if l.startswith('    ')]
    ref = [_norm(l) for l in open(fxc_listing).read().splitlines() if l.startswith('    ') and not l.strip().startswith('//')]
    ref = [l for l in ref if l]
    diffs = [(a, b) for a, b in zip(mine, ref) if a != b]
    return len(mine), len(ref), diffs


if __name__ == '__main__':
    if sys.argv[1] == '--check':
        n1, n2, d = check(sys.argv[2], sys.argv[3])
        print('lines mine %d fxc %d, differing %d' % (n1, n2, len(d)))
        for a, b in d[:20]: print('  mine: %s\n  fxc:  %s' % (a, b))
    else:
        sys.stdout.write(disassemble(open(sys.argv[1], 'rb').read()))
