"""Existence/type of every alias a Nacht WeaponDef references (sound fields + viewmodel notetracks).
Reads weapon_sounds_nacht.json; prints one line per distinct alias."""
import json, os
from alias_index import load, describe
HERE = os.path.dirname(os.path.abspath(__file__))
nacht, common = load()
W = json.load(open(os.path.join(HERE, 'weapon_sounds_nacht.json')))
use = {}
for wn, w in W.items():
    for f, s in w['sounds'].items(): use.setdefault(s.lower(), set()).add(f)
    for slot, a in w['anims'].items():
        for n in a.get('notes', []):
            if n['note'] != 'end': use.setdefault(n['note'].lower(), set()).add('notetrack')
for a in sorted(use):
    hits = []
    for z, d in (('nacht', nacht), ('common', common)):
        l = d.get(a)
        if l:
            kind, files = describe(l)
            inline = all('no data' not in f for f in files)
            hits.append('%s:x%d %s%s %s' % (z, l['count'], kind, '' if inline or kind != 'loaded' else '(xref)', sorted(set(files))[0]))
    print('%-34s %-48s %s' % (a, ','.join(sorted(use[a]))[:48], ' | '.join(hits) if hits else 'MISSING'))
