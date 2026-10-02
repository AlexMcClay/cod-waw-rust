import sys, math
from t4assets import walk
import decode_xanim as dx, xanim_pose as xp
W = walk()
S = xp.Skeleton(W, [('char_ger_honorgd_body1_1', None), ('char_ger_honorgd_zombiehead1_1', None)])
X = dx.all_xanims(W); I = S.index
def run(anim, rotate=True, step=4):
    A = dx.decode(W, X[anim]); nf = A['numframes']
    print(anim, 'nf', nf, 'delta trans keys', len(A['delta']['trans']), 'quat keys', len(A['delta']['quat']), 'first', A['delta']['trans'][:1], A['delta']['quat'][:1])
    print('  notifies (frame):', [(n, round(t * nf, 1)) for n, t in A['notifies']])
    for f in range(0, nf + 1, step):
        gq, gt = S.pose(A, f); dq, dt = xp.delta_at(A, f)
        def world(n):
            p = xp.qrot(dq, gt[I[n]]) if rotate else gt[I[n]]
            return tuple(round(p[c] + dt[c], 1) for c in range(3))
        print('  %3d yaw %7.1f ballL %s ballR %s' % (f, math.degrees(2 * math.atan2(dq[2], dq[3])), world('j_ball_le'), world('j_ball_ri')))
run(sys.argv[1], rotate=(len(sys.argv) < 3))
