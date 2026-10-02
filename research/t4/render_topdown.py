# renders a top-down shaded view of nacht_world.obj around the player start to nacht_topdown.png (sanity check)
from PIL import Image, ImageDraw
V=[];F=[]
for l in open('nacht_world.obj'):
    if l.startswith('v '): V.append(tuple(map(float,l.split()[1:4])))
    elif l.startswith('f '): F.append(tuple(int(x.split('/')[0])-1 for x in l.split()[1:4]))
cx,cy,R=-37,202,1600; S=1024
img=Image.new('RGB',(S,S),(0,0,0)); d=ImageDraw.Draw(img)
def P(v): return ((v[0]-cx+R)*S/(2*R), (cy+R-v[1])*S/(2*R))
for f in sorted(F,key=lambda f:max(V[i][2] for i in f)):
    zs=[V[i][2] for i in f]
    if max(zs)>200: continue  # skip roofs
    a,b,c=(V[i] for i in f)
    n=((b[1]-a[1])*(c[2]-a[2])-(b[2]-a[2])*(c[1]-a[1]),(b[2]-a[2])*(c[0]-a[0])-(b[0]-a[0])*(c[2]-a[2]),(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]))
    import math; L=math.sqrt(sum(x*x for x in n)) or 1
    shade=int(60+150*abs(n[2])/L); h=min(255,max(0,int((sum(zs)/3+60)*0.8)))
    d.polygon([P(a),P(b),P(c)],fill=(shade,h//2+shade//3,shade//2))
x,y=P((-37,202,0)); d.ellipse([x-6,y-6,x+6,y+6],outline=(255,0,0),width=3)
img.save('nacht_topdown.png')
