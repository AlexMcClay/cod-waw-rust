import zonewalk as zw, traceback, sys
z=zw.load_zone()
W=zw.Walker(z)
orig=W.load_asset
depth=[0]; log=[]
def la(t,lv):
    depth[0]+=1
    r=lv.ref()
    log.append('  '*depth[0]+'load %s ptr %s pos %d'%(t, r if isinstance(r,int) else type(r).__name__, W.S.pos))
    try: orig(t,lv)
    finally: depth[0]-=1
W.load_asset=la
try: W.walk()
except Exception as e:
    print('\n'.join(log[-int(sys.argv[1]) if len(sys.argv)>1 else -15:])); traceback.print_exc()
