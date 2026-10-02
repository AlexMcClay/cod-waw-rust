"""Markdown tables per weapon: anim slot -> notetrack sound events (frame, seconds, normalized time) + player sound fields.
Reads weapon_sounds_nacht.json. Output: weapon_tables.md (pasted into NACHT_AUDIO.md)."""
import json, os
from alias_index import load
HERE = os.path.dirname(os.path.abspath(__file__))
nacht, common = load()
W = json.load(open(os.path.join(HERE, 'weapon_sounds_nacht.json')))
ORDER = ['zombie_colt', 'sw_357', 'kar98k', 'springfield', 'm1carbine', 'gewehr43', 'm1garand', 'm1garand_gl', 'thompson', 'mp40',
         'stg44', 'bar', 'fg42_bipod', 'mg42_bipod', '30cal_bipod', 'doublebarrel', 'doublebarrel_sawed_grip', 'shotgun',
         'kar98k_scoped_zombie', 'ptrs41_zombie', 'panzerschrek', 'm2_flamethrower_zombie', 'ray_gun', 'm7_launcher',
         'stielhandgranate', 'molotov']
PF = ['fireSoundPlayer', 'fireLastSoundPlayer', 'emptyFireSoundPlayer', 'reloadSoundPlayer', 'reloadEmptySoundPlayer',
      'reloadStartSoundPlayer', 'reloadEndSoundPlayer', 'rechamberSoundPlayer', 'raiseSoundPlayer', 'firstRaiseSoundPlayer',
      'putawaySoundPlayer', 'meleeSwipeSoundPlayer', 'meleeHitSound', 'pickupSoundPlayer', 'ammoPickupSoundPlayer',
      'pullbackSoundPlayer', 'overheatSoundPlayer', 'projectileSound']
SKIP_SLOTS = {'adsUp', 'adsDown'}
SLOT_TIME = {'reload': 'iReloadTime', 'reloadEmpty': 'iReloadEmptyTime', 'rechamber': 'iRechamberTime', 'adsRechamber': 'iRechamberTime',
             'raise': 'iRaiseTime', 'firstRaise': 'iFirstRaiseTime', 'drop': 'iDropTime', 'reloadStart': 'iReloadStartTime',
             'reloadEnd': 'iReloadEndTime', 'deploy': 'deployTime', 'breakdown': 'breakdownTime', 'altRaise': 'iAltRaiseTime',
             'altDrop': 'iAltDropTime', 'quickRaise': 'quickRaiseTime', 'quickDrop': 'quickDropTime', 'emptyRaise': 'iEmptyRaiseTime'}
out = []
for wn in ORDER:
    w = W[wn]
    keys = {k.lower() for k, v in w['notetrackSoundMap']}
    out.append('#### `%s`\n' % wn)
    ps = ', '.join('%s=`%s`' % (f.replace('SoundPlayer', 'Plr').replace('Sound', ''), w['sounds'][f]) for f in PF if f in w['sounds'])
    out.append('Player sound fields: %s\n' % (ps or '(none)'))
    rows = []
    for slot, a in w['anims'].items():
        if slot in SKIP_SLOTS or slot == 'melee' or a['anim'].lower() == 'viewmodel_knife_stick': continue
        notes = [n for n in a.get('notes', []) if n['note'] != 'end']
        if not notes: continue
        fps = a.get('fps'); nf = a.get('numframes')
        st = w.get('times_ms', {}).get(SLOT_TIME.get(slot, ''), 0) or 0
        ev = []
        for n in notes:
            nm = n['note'].lower()
            tag = '' if nm in keys else (' **(not in map)**' if (nm in nacht or nm in common) else ' **(no alias)**')
            ev.append('f%g / t%.3f / **%s** `%s`%s' % (n['frame'], n['time'], ('%.2fs' % (n['time'] * st / 1000.0)) if st else '%.2fs(raw)' % n['sec'], n['note'], tag))
        rows.append('| %s | `%s` | %s f @ %g fps = %.2fs | %s | %s |' % (slot, a['anim'], nf, fps, a['len_s'], ('%d ms' % st) if st else '-', '<br>'.join(ev)))
    if rows:
        out.append('| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |')
        out.append('|---|---|---|---|---|')
        out += rows
    else:
        out.append('_No sound notetracks (except the shared knife lunge)._')
    out.append('')
open(os.path.join(HERE, 'weapon_tables.md'), 'w').write('\n'.join(out))
print('\n'.join(out))
