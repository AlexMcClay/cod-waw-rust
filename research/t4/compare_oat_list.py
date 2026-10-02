# compares assets_walk.txt (our walker, load order incl. inline dependencies) with OAT Unlinker --list output
from collections import Counter
a=[tuple(l.rsplit(', ',2)[0].split(', ',1)) for l in open('assets_walk.txt').read().splitlines()]
b=[tuple(l.strip().split(', ',1)) for l in open('oat/list.txt').read().splitlines()[4:] if ', ' in l and not l.startswith(('Unloaded','Finished'))]
sa=['%s|%s'%(t,n.lstrip(',').strip()) for t,n in a]
sb=['%s|%s'%(t,n.lstrip(',').strip()) for t,n in b]
print('ours',len(sa),'oat',len(sb))
print('only in ours',list((Counter(sa)-Counter(sb)).items())[:10])
print('only in OAT',list((Counter(sb)-Counter(sa)).items())[:10])
d=[i for i,(x,y) in enumerate(zip(sa,sb)) if x!=y]
print('order diffs',len(d), [(sa[i],sb[i]) for i in d[:5]])
