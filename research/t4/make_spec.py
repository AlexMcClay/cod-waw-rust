# Renders t4_loaders_gen.py into a terse, language-neutral load-order spec (T4_LOAD_SPEC.txt).
import re, json
LAY = json.load(open('t4_layouts.json'))
src = open('t4_loaders_gen.py').read()
out = ['T4 (WaW PC) zone load-order spec. Mechanically rendered from t4_loaders_gen.py (itself translated from',
       'OpenAssetTools ZoneCodeGenerator output). Notation:',
       '  read STRUCT(N)            : read N bytes into the block on top of the block stack (struct header)',
       '  X.f = alloc(A)            : round current block offset up to A (NO file bytes skipped)',
       '  data(X.f, ESZ, COUNT)     : read ESZ*COUNT bytes (0 bytes from file if current block is RUNTIME)',
       '  xstring(X.f)              : if ptr==-1: alloc(1) + read NUL-terminated string; else back-ref',
       '  asset(TYPE, X.f)          : generic asset pointer load (push TEMP; -1/-2 => inline (+4-byte VIRTUAL slot for -2); else alias)',
       '  ptr_native / ptr_lookup   : back-reference to data / struct already loaded;  alias_lookup: ref to a pointer slot',
       '  array(T, ESZ, n)          : read n*ESZ bytes then run Load_T(False) for each element', '']
for cls in re.finditer(r'class L_(\w+)\(LoaderBase\):\n(.*?)(?=\nclass |\nLOADERS)', src, re.S):
    name, body = cls.group(1), cls.group(2)
    out.append('=' * 100)
    out.append('ASSET %s' % name)
    for fn in re.finditer(r'    def (\w+)\(self, a(?:, count)?\):\n(.*?)(?=\n    def |\Z)', body, re.S):
        fname, fb = fn.group(1), fn.group(2)
        lines = []
        for l in fb.split('\n'):
            if not l.strip() or l.strip() == 'a = a': continue
            l = l.replace('self.', '').replace("v['", '').replace("']", '')
            l = re.sub(r"G\((\w+), '(\w+)'", r'G(\1.\2', l)
            l = re.sub(r"LV\((\w+), '(\w+)'\)\.set\(S\.alloc\((\d+)\)\)", r'\1.\2 = alloc(\3)', l)
            l = re.sub(r"LV\((\w+), '(\w+)'\)\.ref\(\)", r'\1.\2', l)
            l = re.sub(r"LV\((\w+), '(\w+)'(, )?", r'\1.\2(', l)
            l = l.replace("fill(", 'read STRUCT(').replace('load_data(', 'data(').replace('W.load_asset(', 'asset(')
            l = l.replace('S.push(XFILE_BLOCK_', 'push(').replace('S.pop()', 'pop()').replace('xs = ', 'xstring ').replace('load_xstring()', '')
            if l.strip() == '': continue
            lines.append(l[4:])
        out.append('  def %s:' % fname)
        out.extend(lines)
    out.append('')
open('T4_LOAD_SPEC.txt', 'w').write('\n'.join(out))
print(len(out), 'lines')
