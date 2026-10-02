"""
parse_sounds.py - SndAliasList (snd_alias_list_t) -> aliases -> SoundFile -> LoadedSound (inline RIFF) / streamed file name.

Outputs: sounds.json (every alias list with its aliases), prints a summary, and verifies the inline RIFF
bytes against OAT's dumped .wav files for all loaded sounds.
"""
import struct, json, os, sys
import zonewalk as zw
from zonewalk import G, unwrap, Rec, Data, BackRef
from t4assets import walk, data_bytes

HERE = os.path.dirname(os.path.abspath(__file__))
SAT = {0: 'unknown', 1: 'loaded', 2: 'streamed', 3: 'primed'}


def riff_info(b):
    if not b or b[:4] != b'RIFF': return {'riff': False}
    form = b[8:12].decode('latin-1')
    o = 12; info = {'riff': True, 'form': form}
    while o + 8 <= len(b):
        cid = b[o:o + 4]; sz = struct.unpack_from('<I', b, o + 4)[0]
        if cid == b'fmt ':
            fmt, ch, rate, bps, align, bits = struct.unpack_from('<HHIIHH', b, o + 8)
            info.update({'codec': fmt, 'channels': ch, 'rate': rate, 'bits': bits})
        elif cid == b'data':
            info['data_bytes'] = sz
        o += 8 + sz + (sz & 1)
    return info


def loaded_sound(W, ls):
    ls = unwrap(ls)
    if not isinstance(ls, Rec): return {'unresolved': repr(ls)}
    snd = G(ls, 'sound')
    size = G(snd, 'data_size')
    b = data_bytes(W, G(snd, 'data'), size)
    d = unwrap(G(snd, 'data'))
    return {'name': W.cstr(G(ls, 'name')), 'data_size': size, 'data_fpos': d.fpos if isinstance(d, Data) else None,
            **riff_info(b)}


def alias_info(W, a):
    sf = unwrap(G(a, 'soundFile'))
    out = {'aliasName': W.cstr(G(a, 'aliasName')), 'subtitle': W.cstr(G(a, 'subtitle')),
           'secondaryAliasName': W.cstr(G(a, 'secondaryAliasName')), 'chainAliasName': W.cstr(G(a, 'chainAliasName')),
           'volMin': G(a, 'volMin'), 'volMax': G(a, 'volMax'), 'pitchMin': G(a, 'pitchMin'), 'pitchMax': G(a, 'pitchMax'),
           'distMin': G(a, 'distMin'), 'distMax': G(a, 'distMax'), 'probability': G(a, 'probability'),
           'flags': G(a, 'flags'), 'startDelay': G(a, 'startDelay'), 'sequence': G(a, 'sequence')}
    if isinstance(sf, Rec):
        t = G(sf, 'type')
        out['type'] = SAT.get(t, t); out['exists'] = G(sf, 'exists')
        u = G(sf, 'u')
        if t == 1:
            out['loaded'] = loaded_sound(W, G(u, 'loadSnd'))
        else:
            fn = G(u, 'streamSnd', 'filename')
            out['stream'] = {'hash': G(fn, 'hash'), 'dir': W.cstr(G(fn, 'dir')), 'name': W.cstr(G(fn, 'name'))}
    else:
        out['soundFile'] = repr(sf)
    return out


def main():
    W = walk()
    lists = []
    for a in W.assets:
        if a[0] != 'sound': continue
        r = a[2]
        head = unwrap(G(r, 'head'))
        n = G(r, 'count')
        al = [alias_info(W, x) for x in head[:n]] if isinstance(head, list) else []
        lists.append({'name': a[1], 'count': n, 'header_fpos': r.fpos, 'aliases': al})
    json.dump(lists, open(os.path.join(HERE, 'sounds.json'), 'w'), indent=1)
    from collections import Counter
    c = Counter(x.get('type') for l in lists for x in l['aliases'])
    print('alias lists %d, aliases %d, by type %s' % (len(lists), sum(len(l['aliases']) for l in lists), dict(c)))
    loaded = {}
    for l in lists:
        for x in l['aliases']:
            if 'loaded' in x and x['loaded'].get('name'): loaded[x['loaded']['name']] = x['loaded']
    print('distinct LoadedSounds referenced: %d; forms %s; codecs %s' % (len(loaded), Counter(v.get('form') for v in loaded.values()),
          Counter(v.get('codec') for v in loaded.values())))
    ex = [l for l in lists if l['aliases'] and l['aliases'][0].get('type') == 'loaded'][:2] + \
         [l for l in lists if l['aliases'] and l['aliases'][0].get('type') == 'streamed'][:2]
    for l in ex:
        x = l['aliases'][0]
        print('  %-30s -> %s' % (l['name'], x.get('loaded') or x.get('stream')))
    # verify against OAT dump
    ok = bad = miss = 0
    for name, v in loaded.items():
        p = os.path.join(HERE, 'oat', 'dump', 'sound', os.path.splitext(name)[0] + ('.wav' if v.get('form') == 'WAVE' else '.xwma'))
        if not os.path.exists(p): miss += 1; continue
        mine = W.z[v['data_fpos']:v['data_fpos'] + v['data_size']] if v['data_fpos'] is not None else None
        if mine == open(p, 'rb').read(): ok += 1
        else: bad += 1
    print('LoadedSound bytes vs OAT dumped files: identical %d, different %d, no OAT file %d' % (ok, bad, miss))


if __name__ == '__main__':
    main()
