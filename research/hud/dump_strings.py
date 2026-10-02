"""Dump localized strings and selected rawfiles from zones (game-derived output -> local/).
Usage:
  py dump_strings.py loc <zone> [substr ...]        -> local/localize_<zone>.txt   (NAME = "value")
  py dump_strings.py raw <zone> <rawfile name> ...  -> local/raw/<name>
"""
import sys, os
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 't4'))
import zonewalk as zw
from zonewalk import G, unwrap, Data
import t4assets as ta
ROOT = r"D:\SteamLibrary\steamapps\common\Call of Duty World at War\zone\english"


def main():
    mode, zn = sys.argv[1], sys.argv[2]
    args = sys.argv[3:]
    W = zw.Walker(zw.load_zone(os.path.join(ROOT, zn + '.ff')))
    W.walk()
    if mode == 'loc':
        out = os.path.join(HERE, 'local', 'localize_%s.txt' % zn)
        with open(out, 'w', encoding='utf-8') as f:
            for a in W.assets:
                if a[0] != 'localize':
                    continue
                r = unwrap(a[2])
                name, val = W.cstr(G(r, 'name')), W.cstr(G(r, 'value'))
                if not args or any(x.lower() in (name or '').lower() for x in args):
                    f.write('%s = %r\n' % (name, val))
        print('wrote', out)
    else:
        for a in W.assets:
            if a[0] == 'rawfile' and a[1] in args:
                r = unwrap(a[2])
                n = G(r, 'len')
                b = ta.data_bytes(W, G(r, 'buffer'), n)
                p = os.path.join(HERE, 'local', 'raw', a[1].replace('/', os.sep))
                os.makedirs(os.path.dirname(p), exist_ok=True)
                open(p, 'wb').write(b[:n] if b else b'')
                print('wrote', p, n)


if __name__ == '__main__':
    main()
