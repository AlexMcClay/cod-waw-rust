import xanim_pose as xp, decode_xanim as dx
from t4assets import walk
W = walk()
S = xp.Skeleton(W, [('viewmodel_usa_marine_arms', None), ('viewmodel_ger_kar98_rifle', 'tag_weapon')])
allA = dx.all_xanims(W)
A = dx.decode(W, allA['viewmodel_kar98_idle'])
ads = dx.decode(W, allA['viewmodel_kar98_ads_up'])
gq, gt = S.pose(A, 0, layers=[(ads, 0)])
bq, bt = S.bind
for n in ['tag_view','tag_torso','j_shoulder_ri','j_elbow_ri','j_wrist_ri','tag_weapon','j_gun']:
    k = S.index[n]
    print(n, 'posed', [round(x,2) for x in gt[k]], [round(x,3) for x in gq[k]], '| bind', [round(x,2) for x in bt[k]])
m = S.models[0][1]
for b in m['bones'][:4]:
    print(b['name'], b['parent'], [round(x,3) for x in b['localQuat']], [round(x,2) for x in b['localTrans']], [round(x,3) for x in b['baseQuat']], [round(x,2) for x in b['baseTrans']])
