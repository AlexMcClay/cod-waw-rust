# compares world_materials.txt (our parse) with OAT's material JSON dumps (texture slot semantic+image)
import json,glob,os
lines=open('world_materials.txt').read().split('\n')
mats={};cur=None
for l in lines:
    if l and not l.startswith(' '): cur=l.split('  ')[0]; mats[cur]=[]
    elif l.strip():
        p=l.split(); mats[cur].append((p[0], [x for x in p if x.startswith('image=')][0][6:].lstrip(',')))
files={}
for c in glob.glob('oat/dump/materials/**/*.json',recursive=True):
    d=json.load(open(c)); files[os.path.relpath(c,'oat/dump/materials')[:-5].replace(os.sep,'/')]=d
ok=bad=missing=0
for m,tex in mats.items():
    d=files.get(m) or files.get(m.replace('*','_')) or files.get('generated/_'+m.lstrip('*'))
    if d is None: missing+=1; print('missing',m); continue
    oat=sorted((t['semantic'],t['image']) for t in d.get('textures',[]))
    if oat==sorted(tex): ok+=1
    else: bad+=1; print('DIFF',m,oat,sorted(tex))
print('world materials compared with OAT: identical',ok,'different',bad,'not found',missing)
