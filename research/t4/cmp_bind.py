import sys, math, pickle
from t4assets import walk
import decode_xanim as dx, parse_xmodel as px
W = walk()
xm = {a[1]: a[2] for a in W.assets if a[0]=='xmodel'}
model = sys.argv[1]; anim = sys.argv[2]
info = px.xmodel_info(W, xm[model])
A = dx.decode(W, dx.all_xanims(W)[anim])
mb = {b['name']: b for b in info['bones']}
def ang(a, b):
    d = abs(sum(x*y for x,y in zip(a,b))); return math.degrees(2*math.acos(min(1,d)))
print(model, 'bones', len(info['bones']), 'root', info['numRootBones'], [b['name'] for b in info['bones'][:info['numRootBones']]])
missing = [b['name'] for b in A['bones'] if b['name'] not in mb]
print('anim bones not in model:', len(missing), missing[:20])
print('model bones not in anim:', [n for n in mb if n not in {b['name'] for b in A['bones']}])
for b in A['bones']:
    m = mb.get(b['name'])
    if not m: continue
    lq = m.get('localQuat'); lt = m.get('localTrans')
    if lq: l = math.sqrt(sum(c*c for c in lq)); lq = tuple(c/l for c in lq)
    s = '%-22s %-17s %-13s' % (b['name'], b['quatType'], b['transType'])
    if b['rot'] and lq: s += ' angle(anim0,bind) %6.1f  angle(bind,id) %6.1f' % (ang(b['rot'][0][1], lq), ang(lq, (0,0,0,1)))
    elif lq: s += ' (no rot) angle(bind,id) %6.1f' % ang(lq, (0,0,0,1))
    if b['trans'] and lt: s += ' |trans0| %.3f |bindT| %.3f diff %.3f' % (math.dist(b['trans'][0][1], (0,0,0)), math.dist(lt,(0,0,0)), math.dist(b['trans'][0][1], lt))
    elif lt: s += ' (no trans) |bindT| %.3f' % math.dist(lt,(0,0,0))
    print(s)
