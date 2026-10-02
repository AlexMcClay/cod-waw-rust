"""HUD-related WeaponDef fields for every weapon in a zone (display name, hud/ammo-counter icons, clip type,
reticle materials, low-ammo threshold). Output -> local/weapon_hud_<zone>.txt
Usage: py dump_weapon_hud.py [zone ...]   (default nazi_zombie_prototype common)"""
import sys, os
import hudlib
from zonewalk import G, unwrap, Rec

FIELDS_MAT = ['hudIcon', 'ammoCounterIcon', 'reticleCenter', 'reticleSide', 'overlayMaterial', 'killIcon', 'dpadIcon']
FIELDS_VAL = ['weapClass', 'inventoryType', 'offhandClass', 'hudIconRatio', 'ammoCounterIconRatio', 'ammoCounterClip',
              'iClipSize', 'iMaxAmmo', 'iStartAmmo', 'iReticleCenterSize', 'iReticleSideSize', 'iReticleMinOfs',
              'activeReticleType', 'lowAmmoWarningThreshold', 'fHipReticleSidePos', 'crosshairColorChange',
              'overlayReticle', 'overlayInterface', 'overlayWidth', 'overlayHeight']


def matname(W, m):
    import t4assets as ta
    m = unwrap(m)
    if isinstance(m, Rec):
        mi = ta.material_info(W, m)
        return '%s(%s)' % (mi['name'], ta.color_map(mi) or ','.join(str((t['image'] or {}).get('name')) for t in mi['textures']))
    return None if not m else repr(m)


def main():
    zones = sys.argv[1:] or ['nazi_zombie_prototype', 'common']
    for zn in zones:
        W = hudlib.walk_zone(zn)
        p = os.path.join(hudlib.LOCAL, 'weapon_hud_%s.txt' % zn)
        with open(p, 'w', encoding='utf-8') as f:
            for a in W.assets:
                if a[0] != 'weapon':
                    continue
                w = unwrap(a[2])
                f.write('%s  display=%r\n' % (W.cstr(G(w, 'szInternalName')), W.cstr(G(w, 'szDisplayName'))))
                f.write('   ' + '  '.join('%s=%s' % (k, matname(W, G(w, k))) for k in FIELDS_MAT if G(w, k)) + '\n')
                f.write('   ' + '  '.join('%s=%s' % (k, round(G(w, k), 4) if isinstance(G(w, k), float) else G(w, k)) for k in FIELDS_VAL) + '\n')
        print('wrote', p)


if __name__ == '__main__':
    main()
