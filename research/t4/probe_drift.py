# Locates where VIRTUAL block-offset accounting drifts, using string back-references as ground truth.
import zonewalk as zw, sys, bisect
z=zw.load_zone(sys.argv[1])
W=zw.Walker(z)
strpos={}   # our VIRTUAL boff -> fpos of strings we read
ornt=zw.Stream.read_nt
def rnt(s):
    b=s.top(); bo=s.off[b]; x,b2,bo2=ornt(s); strpos[bo2]=x.fpos; return x,b2,bo2
zw.Stream.read_nt=rnt
good=[];bad=[]
olx=zw.LoaderBase.load_xstring
def lx(self, lv=None):
    lv=lv or self.xs; v=lv.ref()
    if v and isinstance(v,int) and zw.ptype(v)==zw.OFFSET:
        blk,off=zw.decode_ptr(v)
        (good if off in strpos else bad).append((off, len(W.assets), self.S.pos))
    return olx(self, lv)
zw.LoaderBase.load_xstring=lx
W.walk()
# asset start virtual offsets
print('string backrefs good',len(good),'bad',len(bad))
if bad:
    mb=min(bad); print('lowest bad target VIRTUAL off %#x (referenced at asset #%d)'%(mb[0],mb[1]))
    gb=max([g for g in good if g[0]<mb[0]] or [(0,0,0)]); print('highest good target below it %#x'%gb[0])
# list assets (with VIRTUAL offsets at their start) around the drift window
W2=zw.Walker(z); starts=[]
ola=W2.load_asset
def la(t,lv):
    starts.append((W2.S.off[4], t, W2.S.pos)); return ola(t,lv)
W2.load_asset=la
W2.walk()
if bad:
    for i,(o,t,p) in enumerate(starts):
        if gb[0]-0x4000 <= o <= mb[0]+0x100: print('  asset-load start VIRTUAL %#x zone@%d %s'%(o,p,t))
