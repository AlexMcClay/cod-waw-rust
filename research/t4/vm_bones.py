from t4assets import walk
import decode_xanim as dx, parse_xmodel as px
W = walk()
xm = {a[1]: a[2] for a in W.assets if a[0]=='xmodel'}
for m in ['viewmodel_usa_marine_arms', 'viewmodel_ger_kar98_rifle', 'char_ger_honorgd_zombiehead1_1', 'char_ger_honorgd_body1_1']:
    info = px.xmodel_info(W, xm[m])
    print(m, info['numBones'], 'root', info['numRootBones'])
    print('  ', [(b['name'], info['bones'][b['parent']]['name'] if b['parent'] >= 0 else None) for b in info['bones']][:80])
A = dx.decode(W, dx.all_xanims(W)['viewmodel_kar98_idle'])
print('anim', [(b['name'], b['quatType'][:4], b['transType'][:5]) for b in A['bones']])
