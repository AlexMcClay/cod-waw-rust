# Empirically determine XModel::trans stride (3 vs 4 floats) and quat format by checking
# global(child) == global(parent) * local(child) using baseMat (global bind pose) for all xmodels.
import struct, math
from t4assets import walk, data_bytes
from zonewalk import G
def qmul(a,b):
    ax,ay,az,aw=a; bx,by,bz,bw=b
    return (aw*bx+ax*bw+ay*bz-az*by, aw*by-ax*bz+ay*bw+az*bx, aw*bz+ax*by-ay*bx+az*bw, aw*bw-ax*bx-ay*by-az*bz)
def qrot(q,v):
    x,y,z,w=q; p=(v[0],v[1],v[2],0.0)
    r=qmul(qmul(q,p),(-x,-y,-z,w)); return r[:3]
W=walk()
res={3:[0,0],4:[0,0]}; qerr=[]
for a in W.assets:
    if a[0]!='xmodel' or a[1].startswith(','): continue
    xm=a[2]; nb=G(xm,'numBones'); nr=G(xm,'numRootBones')
    if nb<=nr: continue
    bm=data_bytes(W,G(xm,'baseMat'),32*nb); pl=data_bytes(W,G(xm,'parentList'),nb-nr)
    tb=data_bytes(W,G(xm,'trans'),16*(nb-nr)); qb=data_bytes(W,G(xm,'quats'),8*(nb-nr))
    if not (bm and pl and tb and qb): continue
    gq=[struct.unpack_from('<4f',bm,32*i) for i in range(nb)]; gt=[struct.unpack_from('<3f',bm,32*i+16) for i in range(nb)]
    for i in range(nr,nb):
        k=i-nr; p=i-pl[k]
        for stride in (3,4):
            if 4*stride*k+12>len(tb): continue
            lt=struct.unpack_from('<3f',tb,4*stride*k)
            # baseMat quats may be scaled; normalise
            q=gq[p]; n=math.sqrt(sum(c*c for c in q)) or 1; q=tuple(c/n for c in q)
            pred=tuple(gt[p][j]+qrot(q,lt)[j] for j in range(3))
            err=max(abs(pred[j]-gt[i][j]) for j in range(3))
            res[stride][0 if err<0.01 else 1]+=1
        lq=tuple(c/32767 for c in struct.unpack_from('<4h',qb,8*k))
        q=gq[p]; n=math.sqrt(sum(c*c for c in q)); q=tuple(c/n for c in q)
        gi=gq[i]; ni=math.sqrt(sum(c*c for c in gi)); gi=tuple(c/ni for c in gi)
        pq=qmul(q,lq); d=abs(sum(pq[j]*gi[j] for j in range(4)))
        qerr.append(1-d)
print('trans stride 3 floats: ok/bad', res[3], '  stride 4 floats: ok/bad', res[4])
print('local quat (int16/32767) composed with parent global vs child global: max 1-|dot| = %.5f, mean %.6f' % (max(qerr), sum(qerr)/len(qerr)))
