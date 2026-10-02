"""Helpers on top of zonewalk: decoding of materials/images/vertices (T4 / WaW PC)."""
import struct, math
import zonewalk as zw
from zonewalk import G, unwrap, XStr, BackRef, Rec, Data

TEXTURE_SEMANTIC = {0: '2d', 1: 'function', 2: 'colorMap', 5: 'normalMap', 8: 'specularMap', 11: 'waterMap'}


def r_hash_string(s):
    """R_HashString as used for MaterialTextureDef.nameHash (sampler names like 'colorMap').
    VERIFIED: colorMap=0xa0ab1041 normalMap=0x59d30d0f specularMap=0x34ecccb3 colorMap1=0xb60d1850"""
    h = 0
    for c in s.encode():
        h = ((c | 0x20) ^ (33 * h)) & 0xFFFFFFFF
    return h


def half_to_float(h):
    return struct.unpack('<e', struct.pack('<H', h))[0]


def unpack_unit_vec(packed):
    """PackedUnitVec (T4 'scale based'): bytes b0,b1,b2 = xyz, b3 = scale"""
    b0, b1, b2, b3 = packed & 0xFF, (packed >> 8) & 0xFF, (packed >> 16) & 0xFF, packed >> 24
    s = (b3 + 192.0) / 32385.0
    return ((b0 - 127.0) * s, (b1 - 127.0) * s, (b2 - 127.0) * s)


def unpack_texcoords_vu(packed):
    """XModel GfxPackedVertex.texCoord: u = half(high 16 bits), v = half(low 16 bits)"""
    return half_to_float((packed >> 16) & 0xFFFF), half_to_float(packed & 0xFFFF)


def data_bytes(W, d, nbytes=None):
    """bytes of a pointer field that resolved to inline Data, or to a back-reference (BackRef) into a
    previously loaded normal block (then nbytes must be given; resolved through the block->file segment map)"""
    d = unwrap(d)
    if isinstance(d, Data):
        return None if d.fpos is None else W.z[d.fpos:d.fpos + (d.size if nbytes is None else nbytes)]
    if isinstance(d, BackRef) and nbytes is not None:
        fp = W.S.file_pos(d.blk, d.off)
        if fp is not None and W.S.file_pos(d.blk, d.off + nbytes - 1) == fp + nbytes - 1:
            return W.z[fp:fp + nbytes]
    return None


def image_info(W, img):
    img = unwrap(img)
    if not isinstance(img, Rec): return {'name': None, 'unresolved': repr(img)}
    ld = unwrap(G(img, 'texture', 'loadDef'))
    info = {'name': W.cstr(G(img, 'name')), 'mapType': G(img, 'mapType'), 'semantic': G(img, 'semantic'),
            'category': G(img, 'category'), 'width': G(img, 'width'), 'height': G(img, 'height'), 'depth': G(img, 'depth'),
            'header_fpos': img.fpos, 'inline_pixels': False}
    if isinstance(ld, Rec):
        dd = unwrap(G(ld, 'data'))
        info.update({'inline_pixels': G(ld, 'resourceSize') > 0, 'has_loaddef': True, 'levelCount': G(ld, 'levelCount'), 'format': G(ld, 'format'),
                     'resourceSize': G(ld, 'resourceSize'), 'dimensions': G(ld, 'dimensions'),
                     'pixels_fpos': dd.fpos if isinstance(dd, Data) else None})
    return info


def material_info(W, mat):
    mat = unwrap(mat)
    if not isinstance(mat, Rec): return {'name': None, 'unresolved': repr(mat)}
    ts = unwrap(G(mat, 'techniqueSet'))
    out = {'name': W.cstr(G(mat, 'info', 'name')), 'header_fpos': mat.fpos,
           'techniqueSet': W.cstr(G(ts, 'name')) if isinstance(ts, Rec) else repr(ts),
           'sortKey': G(mat, 'info', 'sortKey'), 'surfaceTypeBits': G(mat, 'info', 'surfaceTypeBits'),
           'gameFlags': G(mat, 'info', 'gameFlags'), 'stateFlags': G(mat, 'stateFlags'),
           'cameraRegion': G(mat, 'cameraRegion'), 'textures': []}
    tt = unwrap(G(mat, 'textureTable'))
    n = G(mat, 'textureCount')
    if isinstance(tt, list):
        for td in tt[:n]:
            sem = G(td, 'semantic')
            img = G(td, 'u', 'image') if sem != 11 else None
            out['textures'].append({'nameHash': G(td, 'nameHash'), 'nameStart': chr(G(td, 'nameStart') & 0xFF),
                                    'nameEnd': chr(G(td, 'nameEnd') & 0xFF), 'samplerState': G(td, 'samplerState'),
                                    'semantic': TEXTURE_SEMANTIC.get(sem, sem), 'image': image_info(W, img) if img is not None else None})
    elif tt:
        out['textures_unresolved'] = repr(tt)
    return out


def color_map(mi):
    """primary color map = texture slot whose nameHash == R_HashString('colorMap'); fallback: first colorMap semantic"""
    for t in mi.get('textures', []):
        if t['nameHash'] == 0xa0ab1041 and t['image']:
            return t['image']['name']
    for t in mi.get('textures', []):
        if t['semantic'] == 'colorMap' and t['image']:
            return t['image']['name']
    return None


def walk(path=None):
    W = zw.Walker(zw.load_zone(path))
    W.walk()
    return W


def find_assets(W, typ, name_sub=None):
    return [a for a in W.assets if a[0] == typ and (name_sub is None or name_sub in (a[1] or ''))]
