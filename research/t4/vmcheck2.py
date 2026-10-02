import xanim_pose as xp
from t4assets import walk
W = walk()
S = xp.Skeleton(W, [('viewmodel_usa_marine_arms', None), ('viewmodel_ger_kar98_rifle', 'tag_weapon')])
for mi in range(2):
    m = S.models[mi][1]
    print(m['bones'][0].keys())
    for b in m['bones'][:12]:
        print({k:(([round(x,3) for x in v]) if isinstance(v,(list,tuple)) else v) for k,v in b.items()})
