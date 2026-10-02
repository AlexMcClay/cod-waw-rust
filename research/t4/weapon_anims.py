"""Dump WeaponDef.szXAnims[35] (+ gun/hand models) for every weapon in a zone -> weapon_anims_<zone>.json/.txt"""
import sys, os, json
from zonewalk import G, unwrap, Rec
from t4assets import walk
WEAP_ANIM = ['root', 'idle', 'emptyIdle', 'fire', 'holdFire', 'lastShot', 'rechamber', 'melee', 'meleeCharge', 'reload',
             'reloadEmpty', 'reloadStart', 'reloadEnd', 'raise', 'firstRaise', 'drop', 'altRaise', 'altDrop', 'quickRaise',
             'quickDrop', 'emptyRaise', 'emptyDrop', 'sprintIn', 'sprintLoop', 'sprintOut', 'deploy', 'breakdown', 'detonate',
             'nightVisionWear', 'nightVisionRemove', 'adsFire', 'adsLastShot', 'adsRechamber', 'adsUp', 'adsDown']
path = sys.argv[1] if len(sys.argv) > 1 else None
W = walk(path)
xanims = {a[1].lower() for a in W.assets if a[0] == 'xanim'}
def mname(m):
    m = unwrap(m)
    return W.cstr(G(m, 'name')) if isinstance(m, Rec) else None
out = {}
lines = []
for a in W.assets:
    if a[0] != 'weapon': continue
    w = a[2]
    anims = {}
    for i in range(35):
        s = W.cstr(G(w, 'szXAnims', i))
        if s: anims['%02d_%s' % (i, WEAP_ANIM[i])] = s
    gun = mname(G(w, 'gunXModel', 0)); hand = mname(G(w, 'handXModel'))
    out[a[1]] = {'gunXModel0': gun, 'handXModel': hand, 'anims': anims}
    lines.append('%s  gun=%s hands=%s' % (a[1], gun, hand))
    for k, v in anims.items(): lines.append('    %-22s %s%s' % (k, v, '' if v.lower() in xanims else '   (NOT IN ZONE)'))
tag = 'nacht' if path is None else os.path.basename(path).replace('.ff', '')
json.dump(out, open('weapon_anims_%s.json' % tag, 'w'), indent=1)
open('weapon_anims_%s.txt' % tag, 'w').write('\n'.join(lines) + '\n')
print('\n'.join(lines))
