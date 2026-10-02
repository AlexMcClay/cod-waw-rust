"""Cross-check viewmodel notetracks vs WeaponDef.notetrackSoundMap vs alias existence (Nacht + common.ff).
Reads weapon_sounds_nacht.json (from dump_weapon_sounds.py) and the alias index."""
import json, os
from alias_index import load
HERE = os.path.dirname(os.path.abspath(__file__))
nacht, common = load()
W = json.load(open(os.path.join(HERE, 'weapon_sounds_nacht.json')))
NON_SOUND = {'end', 'fire', 'start_ragdoll', 'gravity on', 'blend'}
stats = {'in_map_alias': 0, 'in_map_noalias': 0, 'notmap_alias': 0, 'notmap_noalias': 0}
rows = set()
for wn, w in W.items():
    keys = {k.lower(): v for k, v in w['notetrackSoundMap']}
    ident = all(k == v.lower() for k, v in keys.items())
    if not ident: print('NON-IDENTITY MAP', wn, w['notetrackSoundMap'])
    for slot, a in w['anims'].items():
        for n in a.get('notes', []):
            nm = n['note'].lower()
            if nm in NON_SOUND: continue
            inmap = nm in keys
            alias = nm in nacht or nm in common
            stats[('in_map' if inmap else 'notmap') + ('_alias' if alias else '_noalias')] += 1
            if not (inmap and alias): rows.add((wn, slot, a['anim'], nm, inmap, alias))
    for k in keys:
        if k not in nacht and k not in common: print('map key with no alias:', wn, k)
print(stats)
for r in sorted(rows): print('  weapon %-24s slot %-12s anim %-38s note %-30s inMap %s aliasExists %s' % r)
