# checks that streamed sound aliases (dir + name) exist as sound/<dir>/<name> inside the install's IWD archives
import zipfile, glob, json, os
root = r"D:\SteamLibrary\steamapps\common\Call of Duty World at War\main"
names = {}
for p in sorted(glob.glob(os.path.join(root, '*.iwd'))):
    z = zipfile.ZipFile(p)
    for n in z.namelist():
        if n.lower().startswith('sound/'): names[n.lower()] = os.path.basename(p)
print('sound entries in IWDs', len(names), list(names)[:3])
L = json.load(open('sounds.json'))
st = set()
for l in L:
    for a in l['aliases']:
        if 'stream' in a: st.add((a['stream']['dir'], a['stream']['name']))
key = lambda s: ('sound/%s/%s' % (s[0], s[1])).replace(chr(92), '/').lower()
f = [s for s in st if key(s) in names]
print('streamed files', len(st), 'found', len(f), 'missing', [s for s in st if s not in f][:10])
print('example', [(key(s), names[key(s)]) for s in f[:3]])
