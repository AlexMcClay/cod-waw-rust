# compare our OBJ with OAT's OBJ: triangles as sets of rounded vertex positions (handles OAT vertex dedup + axis swaps)
import sys, itertools
def load(p):
    V=[];F=[];VT=[]
    for l in open(p):
        if l.startswith('v '): V.append(tuple(float(x) for x in l.split()[1:4]))
        elif l.startswith('vt '): VT.append(tuple(float(x) for x in l.split()[1:3]))
        elif l.startswith('f '): F.append(tuple(int(x.split('/')[0]) for x in l.split()[1:4]))
    return V,F,VT
a,b=sys.argv[1],sys.argv[2]
Va,Fa,VTa=load(a); Vb,Fb,VTb=load(b)
print('ours: %d v %d f | oat: %d v %d f'%(len(Va),len(Fa),len(Vb),len(Fb)))
def key(V,f,perm,sign): return frozenset(tuple(round(sign[k]*V[i-1][perm[k]],2) for k in range(3)) for i in f)
best=None
for perm in itertools.permutations(range(3)):
    for sign in itertools.product((1,-1),repeat=3):
        sa=set(key(Va,f,perm,sign) for f in Fa[:200]); sb=set(key(Vb,f,(0,1,2),(1,1,1)) for f in Fb)
        m=len(sa&sb)
        if best is None or m>best[0]: best=(m,perm,sign)
m,perm,sign=best
sa=set(key(Va,f,perm,sign) for f in Fa); sb=set(key(Vb,f,(0,1,2),(1,1,1)) for f in Fb)
print('best axis mapping ours->oat perm',perm,'sign',sign,': triangles matched %d / ours %d / oat %d'%(len(sa&sb),len(sa),len(sb)))
uva=set(tuple(round(x,3) for x in t) for t in VTa); uvb=set(tuple(round(x,3) for x in t) for t in VTb)
print('uv sets overlap %d / %d'%(len(uva&uvb),len(uvb)))
