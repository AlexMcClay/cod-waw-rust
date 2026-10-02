"""Dump, for every WeaponDef in a zone: all SndAliasCustom sound fields, notetrackSoundMapKeys/Values,
and the notetracks (name, normalized time, frame) of every viewmodel anim slot.

usage: py dump_weapon_sounds.py [zone.ff | zone.bin] [--anims-from other.ff ...]
Default zone = research/t4/zone.bin (Nacht). Anims that live in another zone (e.g. common.ff colt45/knife)
are looked up in the zones passed with --anims-from.
Writes weapon_sounds_<tag>.json next to this script and prints a text report."""
import sys, os, json
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 't4'))
from zonewalk import G, unwrap, Rec, XStr
from t4assets import walk
import decode_xanim as dx

WEAP_ANIM = ['root', 'idle', 'emptyIdle', 'fire', 'holdFire', 'lastShot', 'rechamber', 'melee', 'meleeCharge', 'reload',
             'reloadEmpty', 'reloadStart', 'reloadEnd', 'raise', 'firstRaise', 'drop', 'altRaise', 'altDrop', 'quickRaise',
             'quickDrop', 'emptyRaise', 'emptyDrop', 'sprintIn', 'sprintLoop', 'sprintOut', 'deploy', 'breakdown', 'detonate',
             'nightVisionWear', 'nightVisionRemove', 'adsFire', 'adsLastShot', 'adsRechamber', 'adsUp', 'adsDown']
SOUNDS = ["pickupSound", "pickupSoundPlayer", "ammoPickupSound", "ammoPickupSoundPlayer", "projectileSound",
          "pullbackSound", "pullbackSoundPlayer", "fireSound", "fireSoundPlayer", "fireLoopSound",
          "fireLoopSoundPlayer", "fireStopSound", "fireStopSoundPlayer", "fireLastSound", "fireLastSoundPlayer",
          "emptyFireSound", "emptyFireSoundPlayer", "crackSound", "whizbySound", "meleeSwipeSound",
          "meleeSwipeSoundPlayer", "meleeHitSound", "meleeMissSound", "rechamberSound", "rechamberSoundPlayer",
          "reloadSound", "reloadSoundPlayer", "reloadEmptySound", "reloadEmptySoundPlayer", "reloadStartSound",
          "reloadStartSoundPlayer", "reloadEndSound", "reloadEndSoundPlayer", "rotateLoopSound",
          "rotateLoopSoundPlayer", "deploySound", "deploySoundPlayer", "finishDeploySound",
          "finishDeploySoundPlayer", "breakdownSound", "breakdownSoundPlayer", "finishBreakdownSound",
          "finishBreakdownSoundPlayer", "detonateSound", "detonateSoundPlayer", "nightVisionWearSound",
          "nightVisionWearSoundPlayer", "nightVisionRemoveSound", "nightVisionRemoveSoundPlayer", "altSwitchSound",
          "altSwitchSoundPlayer", "raiseSound", "raiseSoundPlayer", "firstRaiseSound", "firstRaiseSoundPlayer",
          "putawaySound", "putawaySoundPlayer", "overheatSound", "overheatSoundPlayer"]


TIMES = ['iFireTime', 'iRechamberTime', 'iRechamberBoltTime', 'iMeleeTime', 'meleeChargeTime', 'iReloadTime', 'iReloadEmptyTime',
         'iReloadAddTime', 'reloadEmptyAddTime', 'iReloadStartTime', 'iReloadStartAddTime', 'iReloadEndTime', 'iDropTime',
         'iRaiseTime', 'quickDropTime', 'quickRaiseTime', 'iFirstRaiseTime', 'iEmptyRaiseTime', 'iEmptyDropTime', 'deployTime',
         'breakdownTime', 'iAltRaiseTime', 'iAltDropTime']


def snd_name(W, sac):
    n = unwrap(G(sac, 'name'))
    if isinstance(n, Rec):
        return W.cstr(G(n, 'soundName'))
    return None if not n else repr(n)


def anim_table(W):
    out = {}
    for name, r in dx.all_xanims(W).items():
        nf = G(r, 'numframes'); fps = G(r, 'framerate')
        notes = []
        nt = unwrap(G(r, 'notify'))
        if nt:
            for e in nt:
                t = G(e, 'time')
                notes.append({'note': W.script_strings[G(e, 'name')], 'time': round(t, 4),
                              'frame': round(t * nf, 2), 'sec': round(t * nf / fps, 3) if fps else None})
        out[name.lower()] = {'numframes': nf, 'fps': fps, 'len_s': round(nf / fps, 3) if fps else None, 'notes': notes}
    return out


def main():
    args = sys.argv[1:]
    extra = []
    if '--anims-from' in args:
        i = args.index('--anims-from'); extra = args[i + 1:]; args = args[:i]
    path = args[0] if args else None
    W = walk(path)
    anims = anim_table(W)
    anim_src = {k: 'zone' for k in anims}
    for p in extra:
        W2 = walk(p)
        for k, v in anim_table(W2).items():
            if k not in anims:
                anims[k] = v; anim_src[k] = os.path.basename(p)
    res = {}
    lines = []
    for a in W.assets:
        if a[0] != 'weapon' or a[2] is None: continue
        w = a[2]
        snds = {}
        for f in SOUNDS:
            s = snd_name(W, G(w, f))
            if s: snds[f] = s
        keys = [G(w, 'notetrackSoundMapKeys', i) for i in range(20)]
        vals = [G(w, 'notetrackSoundMapValues', i) for i in range(20)]
        ntmap = [(W.script_strings[k], W.script_strings[v]) for k, v in zip(keys, vals) if k]
        slots = {}
        for i in range(35):
            s = W.cstr(G(w, 'szXAnims', i))
            if not s: continue
            info = anims.get(s.lower())
            slots[WEAP_ANIM[i]] = {'anim': s, 'src': anim_src.get(s.lower()), **(info or {'missing': True})}
        times = {f: G(w, f) for f in TIMES}
        res[a[1]] = {'displayName': W.cstr(G(w, 'szDisplayName')), 'sounds': snds, 'notetrackSoundMap': ntmap, 'times_ms': times, 'anims': slots}
        lines.append('=== %s' % a[1])
        lines.append('   times_ms ' + ' '.join('%s=%s' % (k, v) for k, v in times.items() if v))
        for f, s in snds.items(): lines.append('   snd  %-26s %s' % (f, s))
        if ntmap: lines.append('   notetrackSoundMap %s' % ntmap)
        for slot, inf in slots.items():
            nts = [n for n in inf.get('notes', []) if n['note'] != 'end']
            if nts or inf.get('missing'):
                lines.append('   anim %-12s %-36s %s nf=%s %s' % (slot, inf['anim'], inf.get('src') or 'MISSING', inf.get('numframes'),
                             ' '.join('%s@f%g(%.3fs)' % (n['note'], n['frame'], n['sec'] or 0) for n in nts)))
    tag = 'nacht' if path is None else os.path.basename(path).replace('.ff', '')
    json.dump(res, open(os.path.join(HERE, 'weapon_sounds_%s.json' % tag), 'w'), indent=1)
    open(os.path.join(HERE, 'weapon_sounds_%s.txt' % tag), 'w').write('\n'.join(lines) + '\n')
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
