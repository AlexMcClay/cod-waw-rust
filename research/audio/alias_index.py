"""Alias lookup across Nacht (research/t4/sounds.json) and common.ff (sounds_common.json, generated on first run).

usage: py alias_index.py name1 name2 ...      -> one line per alias: zone, variant count, loaded/streamed, files, IWD hit
       py alias_index.py --grep substr         -> list alias-list names containing substr (both zones)
Streamed files are checked against sound/<dir>/<name> in the install's main/*.iwd."""
import sys, os, json, glob, zipfile
HERE = os.path.dirname(os.path.abspath(__file__))
T4 = os.path.join(HERE, '..', 't4')
sys.path.insert(0, T4)
INSTALL = r"D:\SteamLibrary\steamapps\common\Call of Duty World at War"
COMMON_FF = os.path.join(INSTALL, 'zone', 'english', 'common.ff')


def build_common():
    p = os.path.join(HERE, 'sounds_common.json')
    if os.path.exists(p): return json.load(open(p))
    from zonewalk import G, unwrap
    from t4assets import walk
    import parse_sounds as ps
    W = walk(COMMON_FF)
    lists = []
    for a in W.assets:
        if a[0] != 'sound' or a[2] is None: continue
        r = a[2]; head = unwrap(G(r, 'head')); n = G(r, 'count')
        al = [ps.alias_info(W, x) for x in head[:n]] if isinstance(head, list) else []
        lists.append({'name': a[1], 'count': n, 'aliases': al})
    json.dump(lists, open(p, 'w'), indent=0)
    return lists


_iwd = None
def iwd_names():
    global _iwd
    if _iwd is None:
        _iwd = {}
        for p in sorted(glob.glob(os.path.join(INSTALL, 'main', '*.iwd'))):
            for n in zipfile.ZipFile(p).namelist():
                if n.lower().startswith('sound/'): _iwd[n.lower()] = os.path.basename(p)
    return _iwd


def load():
    nacht = {l['name'].lower(): l for l in json.load(open(os.path.join(T4, 'sounds.json')))}
    common = {l['name'].lower(): l for l in build_common()}
    return nacht, common


def describe(l):
    kinds = set(); files = []; iwd = iwd_names()
    for a in l['aliases']:
        t = a.get('type'); kinds.add(t)
        if 'stream' in a:
            k = ('sound/%s/%s' % (a['stream']['dir'], a['stream']['name'])).replace(chr(92), '/').replace('//', '/').lower()
            files.append('%s [%s]' % (k, iwd.get(k, 'NOT IN IWD')))
        elif 'loaded' in a:
            n = a['loaded'].get('name') or '?'
            files.append(n + ('' if a['loaded'].get('data_size') else ' (no data: cross-zone ref)'))
    return '/'.join(sorted(str(k) for k in kinds)), files


def main():
    nacht, common = load()
    args = sys.argv[1:]
    if args and args[0] == '--grep':
        s = args[1].lower()
        for z, d in (('nacht', nacht), ('common', common)):
            for k in sorted(d):
                if s in k: print('%-7s %s' % (z, d[k]['name']))
        return
    for n in args:
        hit = False
        for z, d in (('nacht', nacht), ('common', common)):
            l = d.get(n.lower())
            if l:
                hit = True
                kind, files = describe(l)
                uniq = sorted(set(files))
                print('%-34s %-6s x%-2d %-8s %s%s' % (n, z, l['count'], kind, '; '.join(uniq[:4]), ' ...(+%d)' % (len(uniq) - 4) if len(uniq) > 4 else ''))
        if not hit: print('%-34s MISSING (not in nacht or common)' % n)


if __name__ == '__main__':
    main()
