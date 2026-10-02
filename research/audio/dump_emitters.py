"""Ambient sound emitters of Nacht from the map entities (research/t4/nacht_mapents.txt) -> nacht_emitters.json.
script_struct with script_label random|looper|line_emitter (handled by clientscripts/_audio.csc in common.ff)."""
import json, os, re
HERE = os.path.dirname(os.path.abspath(__file__))
txt = open(os.path.join(HERE, '..', 't4', 'nacht_mapents.txt'), encoding='latin-1').read()
ents = [dict(re.findall(r'"([^"]+)" "([^"]*)"', b)) for b in re.findall(r'\{([^{}]*)\}', txt)]
by_tn = {e['targetname']: e for e in ents if 'targetname' in e}
out = []
for e in ents:
    if 'script_sound' not in e: continue
    o = [float(v) for v in e['origin'].split()]
    r = {'label': e.get('script_label'), 'alias': e['script_sound'], 'origin': o}
    if e.get('script_label') == 'random':
        r['wait_min'] = float(e.get('script_wait_min', 1)); r['wait_max'] = float(e.get('script_wait_max', 3))
    if e.get('script_label') == 'line_emitter':
        r['looping'] = 'script_looping' in e
        t = by_tn.get(e.get('target'))
        r['end'] = [float(v) for v in t['origin'].split()] if t else None
    out.append(r)
json.dump(out, open(os.path.join(HERE, 'nacht_emitters.json'), 'w'), indent=1)
from collections import Counter
print(Counter((r['label'], r['alias']) for r in out))
print(set((r.get('wait_min'), r.get('wait_max')) for r in out if r['label'] == 'random'))
