import os, numpy as np, matplotlib
matplotlib.use('Agg'); import matplotlib.pyplot as plt
from t4assets import walk
import decode_xanim as dx, xanim_pose as xp
from render_xanim import draw_mesh, OUT
W = walk(); X = dx.all_xanims(W)
S = xp.Skeleton(W, [('char_ger_honorgd_body1_1', None), ('char_ger_honorgd_zombiehead1_1', None)])
rows = [('ai_zombie_idle_base', 'ai_zombie_idle_v1'), ('ai_zombie_idle_crawl_base', 'ai_zombie_idle_crawl')]
fig, axes = plt.subplots(len(rows), 6, figsize=(14, 5.5))
for r, (base, add) in enumerate(rows):
    B = dx.decode(W, X[base]); Ad = dx.decode(W, X[add])
    for c in range(6):
        f = Ad['numframes'] * c / 5
        if c == 0: gq, gt = S.pose(B, 0); t = base + ' only'
        else: gq, gt = S.pose(B, 0, additive=Ad, add_frame=f); t = '+ %s f%.0f' % (add.replace('ai_zombie_', ''), f)
        meshes = [(S.skin(gq, gt, 0), (0.55, 0.55, 0.45)), (S.skin(gq, gt, 1), (0.85, 0.7, 0.6))]
        ax = axes[r][c]; draw_mesh(ax, meshes, lambda P, depth=False: P[:, 1] if depth else P[:, [0, 2]])
        ax.axhline(0, color='k', lw=0.5); ax.set_xlim(-50, 70); ax.set_ylim(-5, 75); ax.set_aspect('equal'); ax.set_xticks([]); ax.set_yticks([])
        ax.set_title(t, fontsize=7)
fig.suptitle('assetType 6 (additive) layered on a base pose: local q = q_base * q_add, t = t_base + t_add (ASSUMED order)')
fig.tight_layout(); p = os.path.join(OUT, 'zombie_additive.png'); fig.savefig(p, dpi=75); print('wrote', p)
