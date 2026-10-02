import sys, types, importlib
import zonewalk as zw
src0=open('t4_loaders_gen.py').read()
z=zw.load_zone()
for X in (128,16):
  for Y in (16,1):
    src=src0.replace("'lodData'), 128,","'lodData'), %d,"%X).replace("'surfaceCastsSunShadow'), 128,","'surfaceCastsSunShadow'), %d,"%X).replace("), 16, 32 *","), %d, 32 *"%Y)
    m=types.ModuleType('t4_loaders_gen'); exec(compile(src,'gen','exec'),m.__dict__); sys.modules['t4_loaders_gen']=m
    W=zw.Walker(z); W.LOADERS=m.LOADERS; W.walk()
    print(X,Y,hex(W.S.off[1]), 'target 0xc8410')
