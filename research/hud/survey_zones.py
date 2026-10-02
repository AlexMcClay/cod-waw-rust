"""Walk HUD-relevant zones and list fonts / menulists / menus / materials (with their colorMap image).
Usage: py survey_zones.py [zone names...]   -> writes local/survey_<zone>.txt
"""
import sys, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 't4'))
import zonewalk as zw, t4assets as ta
ROOT = r"D:\SteamLibrary\steamapps\common\Call of Duty World at War\zone\english"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'local')
zones = sys.argv[1:] or ['code_post_gfx', 'common', 'ui', 'patch', 'nazi_zombie_prototype', 'nazi_zombie_factory_patch']
for zn in zones:
    W = zw.Walker(zw.load_zone(os.path.join(ROOT, zn + '.ff'))); W.walk()
    cnt = collections.Counter(a[0] for a in W.assets)
    with open(os.path.join(OUT, 'survey_%s.txt' % zn), 'w', encoding='utf-8') as f:
        f.write('# %s counts: %s\n' % (zn, dict(cnt)))
        for a in W.assets:
            t = a[0]
            if t in ('font', 'menulist', 'menu', 'localize', 'rawfile', 'stringtable'):
                f.write('%s\t%s\t%d\n' % (t, a[1], a[3]))
            elif t == 'material':
                mi = ta.material_info(W, a[2])
                imgs = ','.join('%s:%s' % (x['semantic'], (x['image'] or {}).get('name')) for x in mi.get('textures', []))
                f.write('material\t%s\t%s\t%s\n' % (a[1], mi['techniqueSet'], imgs))
    print(zn, dict(cnt), flush=True)
