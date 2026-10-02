import zlib,sys
p=r"D:\SteamLibrary\steamapps\common\Call of Duty World at War\zone\english\nazi_zombie_prototype.ff"
d=open(p,'rb').read()
print(d[:12])
z=zlib.decompress(d[12:])
print(len(z))
open('zone.bin','wb').write(z)
