# sanity: vertical ray through Nacht player start (-37,202,57) against nacht_world.obj -> floor below / ceiling above
V=[];F=[];mat={};cur=None
for l in open('nacht_world.obj'):
    if l.startswith('v '): V.append(tuple(map(float,l.split()[1:4])))
    elif l.startswith('usemtl'): cur=l.split()[1]
    elif l.startswith('f '): F.append((tuple(int(x.split('/')[0])-1 for x in l.split()[1:4]),cur))
px,py,pz=-37.0,202.0,57.0
hits=[]
for (a,b,c),m in F:
    A,B,C=V[a],V[b],V[c]
    d=(B[0]-A[0])*(C[1]-A[1])-(C[0]-A[0])*(B[1]-A[1])
    if abs(d)<1e-9: continue
    u=((px-A[0])*(C[1]-A[1])-(C[0]-A[0])*(py-A[1]))/d
    v=((B[0]-A[0])*(py-A[1])-(px-A[0])*(B[1]-A[1]))/d
    if u<0 or v<0 or u+v>1: continue
    hits.append((A[2]+u*(B[2]-A[2])+v*(C[2]-A[2]),m))
hits.sort()
below=[h for h in hits if h[0]<=pz]; above=[h for h in hits if h[0]>pz]
print('floor below spawn:', below[-1] if below else None, ' ceiling above:', above[0] if above else None, ' total hits', len(hits))
