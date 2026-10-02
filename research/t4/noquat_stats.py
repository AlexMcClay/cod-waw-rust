import math, collections
from t4assets import walk
import decode_xanim as dx, parse_xmodel as px
W = walk()
xm = {a[1]: a[2] for a in W.assets if a[0]=='xmodel'}
def binds(name):
    info = px.xmodel_info(W, xm[name]); out = {}
    for b in info['bones']:
        q = b.get('localQuat')
        if q: l = math.sqrt(sum(c*c for c in q)); out[b['name']] = (tuple(c/l for c in q), b['localTrans'])
    return out
def ang(a, b):
    d = abs(sum(x*y for x,y in zip(a,b))); return math.degrees(2*math.acos(min(1,d)))
models = {'ai_': binds('char_ger_honorgd_body1_1'), 'viewmodel_': binds('viewmodel_usa_marine_arms')}
models['ai_'].update(binds('char_ger_honorgd_zombiehead1_1'))
anims = dx.all_xanims(W)
for pre, mb in models.items():
    st = collections.Counter(); ex = collections.defaultdict(set); nsz = collections.Counter()
    for n, r in anims.items():
        if not n.startswith(pre): continue
        A = dx.decode(W, r)
        if A['assetType'] == 6: continue
        for b in A['bones']:
            if b['name'] not in mb: continue
            bq = mb[b['name']][0]
            if b['quatType'] == 'NO_QUAT':
                k = 'noquat_bind_identity' if ang(bq,(0,0,0,1)) < 0.5 else 'noquat_bind_nonidentity'
                st[k] += 1
                if k == 'noquat_bind_nonidentity': ex[b['name']].add(round(ang(bq,(0,0,0,1)),1))
            elif b['quatType'].endswith('NO_SIZE'):
                a = ang(b['rot'][0][1], bq)
                nsz['nosize_eq_bind' if a < 0.5 else 'nosize_ne_bind'] += 1
            # first key vs bind for animated
            if b['transType'] == 'NO_TRANS': st['notrans'] += 1
    print(pre, dict(st), dict(nsz)); print('  nonidentity NO_QUAT bones:', dict(list(ex.items())[:15]))
