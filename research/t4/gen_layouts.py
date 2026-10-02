# Builds t4_layouts.json: 32-bit struct layouts (offsets/sizes from OAT ZoneCodeGenerator
# struct tests) + member C types (parsed from OAT T4_Assets.h) + enum constant values.
import re, json, glob, os
HERE = os.path.dirname(os.path.abspath(__file__))
hdr = open(os.path.join(HERE, 'ref', 'T4_Assets.h'), encoding='utf-8').read()
hdr = re.sub(r'//[^\n]*', '', hdr)
hdr = re.sub(r'/\*.*?\*/', '', hdr, flags=re.S)

# ---------- struct tests: offsets / sizes / align
layouts = {}
for f in glob.glob(os.path.join(HERE, 'gen', 'out2', 'T4', 'XAssets', '*', '*struct_test.cpp')):
    cur = None
    for line in open(f, encoding='utf-8'):
        m = re.search(r'Tests for (\w+)"', line)
        if m:
            cur = layouts.setdefault(m.group(1), {'offsets': {}, 'size': None, 'align': None}); continue
        m = re.search(r'offsetof\(\w+, (\w+)\) == (\d+)', line)
        if m and cur is not None:
            cur['offsets'][m.group(1)] = int(m.group(2)); continue
        m = re.search(r'(\d+)u == sizeof\((\w+)\)', line)
        if m: cur['size'] = int(m.group(1))
        m = re.search(r'(\d+)u == alignof\((\w+)\)', line)
        if m: cur['align'] = int(m.group(1))

# ---------- header: members + enums + typedefs
members = {}
enums = {}
enum_base = {}
typedefs = {}
tok = re.finditer(r'(typedef[^;]*;)|((?:struct|union)\s+(?:\w+\(\d+\)\s+)?(\w+)\s*\{)|(enum\s+(\w+)\s*(?::\s*([\w ]+))?\s*\{([^}]*)\})', hdr)
for m in tok:
    if m.group(1):
        t = m.group(1)
        mm = re.match(r'typedef\s+(?:\w+\(\d+\)\s+)?(.+?)\s+(\w+)\s*((?:\[\d+\])*)\s*;', t.replace('\n', ' '))
        if mm: typedefs[mm.group(2)] = (mm.group(1).strip(), [int(x) for x in re.findall(r'\[(\d+)\]', mm.group(3))])
    elif m.group(2):
        name = m.group(3)
        # find matching brace
        i = m.end(); depth = 1; body_start = i
        while depth:
            c = hdr[i]
            if c == '{': depth += 1
            elif c == '}': depth -= 1
            i += 1
        body = hdr[body_start:i - 1]
        # remove nested blocks
        flat = ''; d = 0
        for c in body:
            if c == '{': d += 1; flat += ' ANON ' if d == 1 else ''; continue
            if c == '}': d -= 1; continue
            if d == 0: flat += c
        mem = {}
        for decl in flat.split(';'):
            decl = ' '.join(decl.split())
            if not decl: continue
            decl = re.sub(r'\b(type_align32|type_align|gcc_align32|gcc_align|tdef_align32)\(\d+\)\s*', '', decl)
            decl = decl.replace('const ', '')
            decl = re.sub(r'\(\*(\w+)\)\s*\[\w+\]', r'* \1', decl)
            mm = re.match(r'(?:struct |union )?(.*?)\s*(\**)\s*(\w+)\s*((?:\[[^\]]+\])*)\s*(?::\s*(\d+))?$', decl)
            if not mm: continue
            base, ptr, nm, dims, bits = mm.groups()
            if base.endswith('ANON'): base = 'ANON'
            dl = []
            for x in re.findall(r'\[([^\]]+)\]', dims):
                try: dl.append(int(x, 0))
                except ValueError: dl.append(x)
            mem[nm] = {'type': base.strip(), 'ptr': len(ptr), 'dims': dl}
        members[name] = mem
    else:
        name = m.group(5); base = (m.group(6) or 'int').strip()
        enum_base[name] = base
        val = -1
        for e in m.group(7).split(','):
            e = e.strip()
            if not e: continue
            if '=' in e:
                k, v = [x.strip() for x in e.split('=', 1)]
                try: val = int(eval(v.replace('u', ''), {}, enums))
                except Exception: val = enums.get(v, 0)
            else:
                k = e; val += 1
            enums[k] = val

json.dump({'layouts': layouts, 'members': members, 'enums': enums, 'enum_base': enum_base, 'typedefs': typedefs},
          open(os.path.join(HERE, 't4_layouts.json'), 'w'), indent=1)
print(len(layouts), 'layouts', len(members), 'header structs', len(enums), 'enum consts', len(typedefs), 'typedefs')
for k in ('GfxWorld', 'Material', 'XModel', 'XSurface', 'GfxSurface'):
    print(k, layouts.get(k, {}).get('size'), list(members.get(k, {}).items())[:3])

# ---------- Fill offsets from generated loaders (structs with pointers; 32-bit offsets)
d = json.load(open(os.path.join(HERE, 't4_layouts.json')))
L = d['layouts']
for f in glob.glob(os.path.join(HERE, 'gen', 'out', 'T4', 'XAssets', '*', '*_load_db.cpp')):
    src = open(f, encoding='utf-8').read()
    for m in re.finditer(r'void Loader_\w+::FillStruct_(\w+)\(const ZoneStreamFillReadAccessor& fillAccessor\)\n\{(.*?)\n\}', src, re.S):
        sname, body = m.group(1), m.group(2)
        lay = L.setdefault(sname, {'offsets': {}, 'size': None, 'align': None})
        for mm in re.finditer(r'fillAccessor\.Fill(?:Ptr|Array)?\(var\w+->(\w+)(?:\[(\w+)\])*,\s*(\d+)', body):
            fld, idx, off = mm.group(1), mm.group(2), int(mm.group(3))
            if idx is None or fld not in lay['offsets']:
                if fld not in lay['offsets'] or lay['offsets'][fld] > off:
                    lay['offsets'][fld] = off
        for mm in re.finditer(r'var\w+ = &?var\w+->(\w+)(?:\[\w+\])*;\s*\n\s*FillStruct_\w+\(fillAccessor\.AtOffset\((\d+)', body):
            fld, off = mm.group(1), int(mm.group(2))
            if fld not in lay['offsets'] or lay['offsets'][fld] > off:
                lay['offsets'][fld] = off
    for m in re.finditer(r'FillStruct_(\w+)\(m_stream\.LoadWithFill\((\d+)\)\)', src):
        L.setdefault(m.group(1), {'offsets': {}, 'size': None, 'align': None})['size'] = int(m.group(2))
    for m in re.finditer(r'LoadWithFill\((\d+) \* count\);\s*\n(?:.*\n){1,6}?\s*FillStruct_(\w+)\(arrayFill', src):
        L.setdefault(m.group(2), {'offsets': {}, 'size': None, 'align': None})['size'] = int(m.group(1))
json.dump(d, open(os.path.join(HERE, 't4_layouts.json'), 'w'), indent=1)
print('after fill-parse:', len(L), 'layouts')
for k in ('GfxWorld', 'Material', 'XModel', 'XSurface', 'GfxSurface', 'GfxWorldDpvsStatic', 'GfxWorldDpvsPlanes', 'GfxWaterBuffer'):
    print(k, L.get(k, {}).get('size'), len(L.get(k, {}).get('offsets', {})), len(d['members'].get(k, {})))
