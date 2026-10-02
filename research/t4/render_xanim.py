"""
render_xanim.py - visual verification of decoded xanims: skinned meshes rendered to PNG + OBJ export.

  py render_xanim.py zombie  [anim]           -> xanim_out/<anim>_frames.png, <anim>_fNNN.obj
  py render_xanim.py viewmodel [gunmodel] [anim prefix]
"""
import sys, os, math
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.collections import PolyCollection
from t4assets import walk
import decode_xanim as dx, xanim_pose as xp

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'xanim_out')
os.makedirs(OUT, exist_ok=True)


def draw_mesh(ax, surfs_list, proj, color=(0.75, 0.72, 0.65), light=(0.4, -0.3, 0.85)):
    polys, cols, depth = [], [], []
    L = np.array(light) / np.linalg.norm(light)
    for surfs, col in surfs_list:
        for P, tris in surfs:
            T = np.array(tris)
            if len(T) == 0: continue
            a, b, c = P[T[:, 0]], P[T[:, 1]], P[T[:, 2]]
            n = np.cross(b - a, c - a)
            nl = np.linalg.norm(n, axis=1) + 1e-9
            n = n / nl[:, None]
            near = getattr(proj, 'near', None)
            if near is not None:
                keep = (a[:, 0] > near) & (b[:, 0] > near) & (c[:, 0] > near)
                a, b, c, n, T = a[keep], b[keep], c[keep], n[keep], T[keep]
            sa, sb, sc = proj(a), proj(b), proj(c)
            d = (proj(a, depth=True) + proj(b, depth=True) + proj(c, depth=True)) / 3
            shade = 0.35 + 0.65 * np.abs(n @ L)
            for i in range(len(T)):
                polys.append([sa[i], sb[i], sc[i]]); depth.append(d[i])
                cols.append(tuple(min(1, col[k] * shade[i]) for k in range(3)))
    order = np.argsort(depth)[::-1]   # far first
    pc = PolyCollection([polys[i] for i in order], facecolors=[cols[i] for i in order], edgecolors='none')
    ax.add_collection(pc)


def zombie(anim='ai_zombie_walk_v1', frames=None):
    W = walk()
    S = xp.Skeleton(W, [('char_ger_honorgd_body1_1', None), ('char_ger_honorgd_zombiehead1_1', None)])
    A = dx.decode(W, dx.all_xanims(W)[anim])
    nf = A['numframes']
    frames = frames or [round(nf * k / 7) for k in range(8)]
    # side view: x (forward) horizontal, z up; also front view y,z
    fig, axes = plt.subplots(2, len(frames), figsize=(2.6 * len(frames), 8))
    for j, f in enumerate(frames):
        gq, gt = S.pose(A, f)
        dq, dt = xp.delta_at(A, f)
        meshes = [(S.skin(gq, gt, 0), (0.55, 0.55, 0.45)), (S.skin(gq, gt, 1), (0.85, 0.7, 0.6))]
        if f in (frames[0], frames[len(frames) // 2]):
            xp.write_obj(os.path.join(OUT, '%s_f%03d.obj' % (anim, f)), [('body', meshes[0][0]), ('head', meshes[1][0])],
                         '%s frame %d, entity space (Z up, X forward), root motion NOT applied' % (anim, f))
        for row, (ui, vi, di, title) in enumerate([(0, 2, 1, 'side (x fwd, z up)'), (1, 2, 0, 'front (y, z)')]):
            ax = axes[row][j]
            sgn = -1 if row == 1 else 1
            def proj(P, depth=False, ui=ui, vi=vi, di=di, sgn=sgn):
                if depth: return P[:, di] * (1 if row == 0 else -1)
                return np.stack([sgn * P[:, ui], P[:, vi]], axis=1)
            draw_mesh(ax, meshes, proj)
            # skeleton
            for k in range(len(S.names)):
                p = S.parent[k]
                if p < 0 or S.names[k].startswith('tag_') : continue
                a, b = np.array(gt[p]), np.array(gt[k])
                ax.plot([sgn * a[ui], sgn * b[ui]], [a[vi], b[vi]], 'r-', lw=0.6)
            ax.axhline(0, color='k', lw=0.5)
            ax.set_xlim(-40, 40); ax.set_ylim(-5, 75); ax.set_aspect('equal'); ax.set_xticks([]); ax.set_yticks([])
            ax.set_title('f%d %s\nroot dx=%.1f' % (f, title.split()[0], dt[0]) if row == 0 else 'front', fontsize=8)
    fig.suptitle('%s (%d frames @ %g fps, loop=%s) on char_ger_honorgd_body1_1 + zombiehead1_1' % (anim, nf, A['framerate'], A['loop']))
    fig.tight_layout()
    p = os.path.join(OUT, '%s_frames.png' % anim)
    fig.savefig(p, dpi=80); print('wrote', p)


def viewmodel(gun='viewmodel_ger_kar98_rifle', prefix='viewmodel_kar98_', anims=None):
    W = walk()
    S = xp.Skeleton(W, [('viewmodel_usa_marine_arms', None), (gun, 'tag_weapon')])
    allA = dx.all_xanims(W)
    anims = anims or [(prefix + 'idle', 0), (prefix + 'fire', 2), (prefix + 'rechamber', 15), (prefix + 'reload', 40),
                      (prefix + 'pullout', 0), (prefix + 'idle', 'ads')]
    anims = [(n, f) if n in allA else (prefix + 'reload_empty', 50) for n, f in anims]
    ads = dx.decode(W, allA[prefix + 'ads_up'] if prefix + 'ads_up' in allA else allA[prefix + 'ads_up_pc'])
    fig, axes = plt.subplots(3, len(anims), figsize=(3.2 * len(anims), 9.6))
    for j, (an, f) in enumerate(anims):
        A = dx.decode(W, allA[an])
        # tag_torso is driven only by ads_up/ads_down: hip = ads_up frame 0, full ADS = ads_up last frame
        if f == 'ads':
            f = 0; layer = (ads, ads['numframes']); an_t = an + '+ADS'
        else:
            layer = (ads, 0); an_t = an + ' (hip)'
        gq, gt = S.pose(A, f, layers=[layer])
        meshes = [(S.skin(gq, gt, 0), (0.7, 0.6, 0.5)), (S.skin(gq, gt, 1), (0.45, 0.4, 0.35))]
        xp.write_obj(os.path.join(OUT, '%s_f%03d%s.obj' % (an, f, '_ads' if 'ADS' in an_t else '')), [('arms', meshes[0][0]), ('gun', meshes[1][0])],
                     '%s frame %d, tag_view space (X forward, Y left, Z up)' % (an, f))
        # row 0: eye view (perspective from tag_view origin looking +X): screen = (-y/x, z/x)
        def proj_eye(P, depth=False):
            x = np.maximum(P[:, 0], 0.5)
            if depth: return P[:, 0]
            return np.stack([-P[:, 1] / x, P[:, 2] / x], axis=1)
        proj_eye.near = 2.0
        def proj_top(P, depth=False):
            if depth: return -P[:, 2]
            return np.stack([P[:, 0], P[:, 1]], axis=1)
        def proj_side(P, depth=False):
            if depth: return P[:, 1]
            return np.stack([P[:, 0], P[:, 2]], axis=1)
        for row, proj in enumerate([proj_eye, proj_side, proj_top]):
            ax = axes[row][j]
            draw_mesh(ax, meshes, proj)
            if row == 0:
                ax.set_xlim(-1.0, 1.0); ax.set_ylim(-0.75, 0.5)
                ax.plot([0], [0], 'g+')
            elif row == 1:
                ax.set_xlim(-10, 40); ax.set_ylim(-25, 10); ax.plot([0], [0], 'g+')
            else:
                ax.set_xlim(-10, 40); ax.set_ylim(-20, 15); ax.plot([0], [0], 'g+')
            ax.set_aspect('equal'); ax.set_xticks([]); ax.set_yticks([])
            ax.set_title('%s f%d/%d' % (an_t.replace('viewmodel_', ''), f, A['numframes']) if row == 0 else ('side (x fwd, z up)' if row == 1 else 'top (x fwd, y left up)'), fontsize=8)
    fig.suptitle('viewmodel_usa_marine_arms + %s (j_gun attached to tag_weapon); top: eye view from tag_view' % gun)
    fig.tight_layout()
    p = os.path.join(OUT, '%s_poses.png' % prefix.rstrip('_'))
    fig.savefig(p, dpi=80); print('wrote', p)


def zombie_grid(anims=None, ncols=6, tag='zombie_anims'):
    """side views (x fwd, z up) with ROOT MOTION APPLIED (pose rotated by delta yaw, translated by delta trans)"""
    W = walk()
    S = xp.Skeleton(W, [('char_ger_honorgd_body1_1', None), ('char_ger_honorgd_zombiehead1_1', None)])
    X = dx.all_xanims(W)
    anims = anims or ['ai_zombie_sprint_v1', 'ai_zombie_attack_v1', 'ai_zombie_door_tear_v1', 'ai_zombie_traverse_v1',
                      'ai_zombie_death_v1', 'ai_zombie_crawl']
    fig, axes = plt.subplots(len(anims), ncols, figsize=(2.4 * ncols, 2.6 * len(anims)))
    for r, an in enumerate(anims):
        A = dx.decode(W, X[an]); nf = A['numframes']
        for c in range(ncols):
            f = round(nf * c / (ncols - 1))
            gq, gt = S.pose(A, f)
            dq, dt = xp.delta_at(A, f)
            R = xp.qmat(dq); T = np.array(dt)
            meshes = [([(P @ R.T + T, tr) for P, tr in S.skin(gq, gt, 0)], (0.55, 0.55, 0.45)),
                      ([(P @ R.T + T, tr) for P, tr in S.skin(gq, gt, 1)], (0.85, 0.7, 0.6))]
            ax = axes[r][c]
            draw_mesh(ax, meshes, lambda P, depth=False: P[:, 1] if depth else P[:, [0, 2]])
            ax.axhline(0, color='k', lw=0.5)
            ax.set_xlim(T[0] - 45, T[0] + 75); ax.set_ylim(-5, 75); ax.set_aspect('equal'); ax.set_xticks([]); ax.set_yticks([])
            notes = [n for n, t in A['notifies'] if abs(t * nf - f) <= nf / (2 * (ncols - 1)) and n != 'end']
            ax.set_title('%s f%d/%d\nroot (%.0f,%.0f,%.0f) %s' % (an.replace('ai_zombie_', ''), f, nf, *dt, ','.join(notes)[:28]), fontsize=7)
    fig.suptitle('zombie anims, side view (x fwd, z up), root motion from delta part applied; ground z=0')
    fig.tight_layout()
    p = os.path.join(OUT, '%s.png' % tag)
    fig.savefig(p, dpi=75); print('wrote', p)


if __name__ == '__main__':
    what = sys.argv[1] if len(sys.argv) > 1 else 'zombie'
    if what == 'zombie':
        zombie(*(sys.argv[2:3]))
    elif what == 'grid':
        zombie_grid(sys.argv[2:] or None)
    else:
        viewmodel(*sys.argv[2:4])
