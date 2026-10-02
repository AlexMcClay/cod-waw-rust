import sys, math
from t4assets import walk
import decode_xanim as dx, xanim_pose as xp
W = walk()
S = xp.Skeleton(W, [('char_ger_honorgd_body1_1', None), ('char_ger_honorgd_zombiehead1_1', None)])
# bind check vs baseMat
err = 0
for mi, (mn, info, surfs, bmap) in enumerate(S.models):
    for b, k in zip(info['bones'], bmap):
        err = max(err, math.dist(b['baseTrans'], S.bind[1][k]))
print('bind chain vs baseMat max pos err', err)
anim = sys.argv[1] if len(sys.argv) > 1 else 'ai_zombie_walk_v1'
A = dx.decode(W, dx.all_xanims(W)[anim])
nf = A['numframes']
I = S.index
print('frame  deltaT(x,y,z)  yaw | ankleL z  ankleR z  ballL z  ballR z | world ballL x  ballR x | head z')
for f in range(0, nf + 1, max(1, nf // 24)):
    gq, gt = S.pose(A, f)
    dq, dt = xp.delta_at(A, f)
    def world(n):
        p = xp.qrot(dq, gt[I[n]]); return tuple(p[c] + dt[c] for c in range(3))
    yaw = math.degrees(2 * math.atan2(dq[2], dq[3]))
    print('%4d (%7.2f %6.2f %5.2f) %5.1f | %6.2f %6.2f %6.2f %6.2f | %7.2f %7.2f | %6.2f' % (f, *dt, yaw,
          gt[I['j_ankle_le']][2], gt[I['j_ankle_ri']][2], gt[I['j_ball_le']][2], gt[I['j_ball_ri']][2],
          world('j_ball_le')[0], world('j_ball_ri')[0], gt[I['j_head']][2]))
