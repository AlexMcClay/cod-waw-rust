import sys, json
from t4assets import walk
import decode_xanim as dx
W = walk(sys.argv[1] if len(sys.argv) > 1 else None)
rows = []
for name, r in dx.all_xanims(W).items():
    A = dx.decode(W, r)
    d = A['delta']
    dist = None
    if d and d['trans']:
        a, b = d['trans'][0][1], d['trans'][-1][1]
        dist = [round(b[i] - a[i], 1) for i in range(3)]
    yaw = None
    if d and d['quat']:
        import math
        q = d['quat'][-1][1]; yaw = round(math.degrees(2 * math.atan2(q[2], q[3])), 1)
    rows.append({'name': name, 'numframes': A['numframes'], 'fps': A['framerate'], 'len_s': round(A['numframes'] / A['framerate'], 3) if A['framerate'] else None,
                 'loop': A['loop'], 'delta': A['delta_flag'], 'assetType': A['assetType'], 'bones': len(A['bones']),
                 'delta_trans_end': dist, 'delta_yaw_end': yaw, 'notifies': A['notifies'], 'isDefault': A['isDefault'], 'freq': A['frequency']})
json.dump(rows, open('xanim_list_%s.json' % ('nacht' if len(sys.argv) < 2 else __import__('os').path.basename(sys.argv[1]).replace('.ff', '')), 'w'), indent=0)
for x in rows:
    print('%-45s nf %4d fps %4.0f len %6.2fs loop %d delta %d type %d bones %3d dT %s yaw %s notes %s' % (x['name'], x['numframes'], x['fps'], x['len_s'] or 0, x['loop'], x['delta'], x['assetType'], x['bones'], x['delta_trans_end'], x['delta_yaw_end'], [n for n, t in x['notifies']][:6]))
