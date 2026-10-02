def load(p):
    V=[];F=[]
    for l in open(p):
        if l.startswith('v '): V.append(tuple(float(x) for x in l.split()[1:4]))
        elif l.startswith('f '): F.append(tuple(int(x.split('/')[0]) for x in l.split()[1:4]))
    return V,F
def match(ours, oat):
    Va,Fa=load(ours); Vb,Fb=load(oat)
    # ours is game space (x,y,z); OAT obj is (x, z, -y)
    sa=set(frozenset((round(Va[i-1][0],2),round(Va[i-1][2],2),round(-Va[i-1][1],2)) for i in f) for f in Fa)
    sb=set(frozenset((round(Vb[i-1][0],2),round(Vb[i-1][1],2),round(Vb[i-1][2],2)) for i in f) for f in Fb)
    return len(sa&sb), len(sa), len(sb)
