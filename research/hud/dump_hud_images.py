"""Check which HUD images exist as images/<name>.iwi in the install IWDs and decode previews to local/images/.
Usage: py dump_hud_images.py"""
import os
import hudlib

# material -> image name (from the zone walk, see WAW_ZOMBIE_HUD.md section 4)
IMAGES = [
    'chalkmarks_1', 'chalkmarks_2', 'chalkmarks_3', 'chalkmarks_4', 'chalkmarks_5',
    'scorebar_zom_1', 'scorebar_zom_2', 'scorebar_zom_3', 'scorebar_zom_4',
    'scorebar_zom_long_1', 'scorebar_zom_long_2', 'scorebar_zom_long_3', 'scorebar_zom_long_4',
    'hud_us_grenade', 'us_smokegrenade', 'grenadeicon', 'grenadepointer',
    'ammo_counter_bullet', 'ammo_counter_beltbullet', 'ammo_counter_riflebullet', 'ammo_counter_rocket',
    'ammo_counter_shotgunshell', 'ammo_counter_tesla',
    'hud_bullets_pistol', 'hud_bullets_rifle', 'hud_bullets_sniper', 'hud_bullets_spread',
    'hud_bullets_support_back', 'hud_bullets_support_front',
    'overlay_low_health', 'compasshealthoverlay', 'damage_feedback', 'damage_feedback_j',
    'center_cross', 'side_small', 'mg42_cross', 'grenade_launch_reticle', 'hud_flamethrower_reticle',
    'hint_usable', 'hint_health', 'hint_mantle', 'zombie', 'nazi_intro', 'white', 'black',
    'hud_thompson', 'hud_colt', 'hud_kar98k', 'hud_mp40', 'hud_shotgun', 'hud_bar', 'hud_double_barrel',
    'scope_overlay_german', 'line_horizontal_scorebar', 'gamefonts_pc', 'devfonts',
    'fullscreen_electric_shock', 'stopwatch', 'saving',
]


def main():
    idx = hudlib.iwd_index()
    out = os.path.join(hudlib.LOCAL, 'images')
    lines = []
    for n in IMAGES:
        e = idx.get(n)
        if not e:
            lines.append('%-28s MISSING from IWDs' % n)
            continue
        try:
            fmt, w, h, rgba = hudlib.decode_iwi(hudlib.read_iwi(n))
            hudlib.save_png(os.path.join(out, n + '.png'), w, h, rgba)
            lines.append('%-28s %-5s %4dx%-4d %s' % (n, fmt, w, h, os.path.basename(e[0])))
        except Exception as ex:
            lines.append('%-28s present in %s but not decoded: %s' % (n, os.path.basename(e[0]), ex))
    open(os.path.join(out, 'summary.txt'), 'w').write('\n'.join(lines) + '\n')
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
