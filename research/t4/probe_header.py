import struct
z=open('zone.bin','rb').read()
size,ext=struct.unpack_from('<II',z,0); bs=struct.unpack_from('<7I',z,8)
print('size',size,len(z)-36,'ext',ext,'blocks',[hex(b) for b in bs])
sc,sp,ac,ap=struct.unpack_from('<iiii',z,36)
print(sc,sp,ac,ap)
o=52
ptrs=struct.unpack_from('<%di'%sc,z,o); o+=4*sc
print('nonneg1 ptrs',[ (i,p) for i,p in enumerate(ptrs) if p!=-1][:10])
strs=[]
for i in range(sc):
    if ptrs[i]==-1:
        e=z.index(b'\0',o); strs.append(z[o:e].decode()); o=e+1
    else: strs.append(None)
print(strs[:5],strs[-2:],'table at',o)
tab=struct.unpack_from('<%di'%(2*ac),z,o)
types=tab[0::2]; ps=tab[1::2]
from collections import Counter
print(Counter(types)); print(Counter(ps))
print('data starts',o+8*ac)
