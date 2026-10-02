//! Decoding of the large arrays the walker leaves as zone offsets: map
//! vertices and triangles, model surfaces and their skin weights.
//! Coordinates stay in game space (inches, Z up).

use super::{WorldInfo, WorldSurface, XSurfInfo, ZoneData};

fn f32_at(z: &[u8], p: usize) -> f32 {
    f32::from_le_bytes([z[p], z[p + 1], z[p + 2], z[p + 3]])
}

fn u16_at(z: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([z[p], z[p + 1]])
}

/// IEEE 754 binary16 to f32.
pub fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let man = (h & 0x3ff) as f32;
    sign * match exp {
        0 => man * 2f32.powi(-24),
        31 => {
            if man == 0.0 {
                f32::INFINITY
            } else {
                f32::NAN
            }
        }
        e => (1.0 + man / 1024.0) * 2f32.powi(e - 15),
    }
}

/// The engine's packed unit vector (3 signed bytes scaled by a 4th).
pub fn packed_unit_vec(b: [u8; 4]) -> [f32; 3] {
    let s = (b[3] as f32 + 192.0) / 32385.0;
    [(b[0] as f32 - 127.0) * s, (b[1] as f32 - 127.0) * s, (b[2] as f32 - 127.0) * s]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    /// Sign of the binormal (bitangent = sign * cross(normal, tangent)).
    pub binormal_sign: f32,
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

/// Vertex `i` of the map (44-byte `GfxWorldVertex`).
pub fn world_vertex(zone: &ZoneData, world: &WorldInfo, i: u32) -> Option<Vertex> {
    let base = world.vertices_fpos? + 44 * i as usize;
    let z = &zone.data;
    if base + 44 > z.len() || i >= world.vertex_count {
        return None;
    }
    Some(Vertex {
        pos: [f32_at(z, base), f32_at(z, base + 4), f32_at(z, base + 8)],
        color: [z[base + 16], z[base + 17], z[base + 18], z[base + 19]],
        uv: [f32_at(z, base + 20), f32_at(z, base + 24)],
        normal: packed_unit_vec([z[base + 36], z[base + 37], z[base + 38], z[base + 39]]),
        tangent: packed_unit_vec([z[base + 40], z[base + 41], z[base + 42], z[base + 43]]),
        binormal_sign: f32_at(z, base + 12),
    })
}

/// Lightmap UV of map vertex `i`.
pub fn world_lightmap_uv(zone: &ZoneData, world: &WorldInfo, i: u32) -> Option<[f32; 2]> {
    let base = world.vertices_fpos? + 44 * i as usize + 28;
    Some([f32_at(&zone.data, base), f32_at(&zone.data, base + 4)])
}

/// Triangles of a map surface as global vertex indices.
pub fn world_triangles(zone: &ZoneData, world: &WorldInfo, s: &WorldSurface) -> Vec<[u32; 3]> {
    let Some(ip) = world.indices_fpos else { return Vec::new() };
    let z = &zone.data;
    (0..s.tri_count as usize)
        .filter_map(|t| {
            let at = ip + 2 * (s.base_index as usize + 3 * t);
            if at + 6 > z.len() {
                return None;
            }
            let v = |k: usize| s.first_vertex + u16_at(z, at + 2 * k) as u32;
            Some([v(0), v(1), v(2)])
        })
        .collect()
}

/// A decoded model surface.
#[derive(Debug, Clone, Default)]
pub struct ModelMesh {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u16; 3]>,
    /// Up to four `(bone index, weight)` influences per vertex.
    pub skin: Vec<[(u16, f32); 4]>,
}

/// Decodes a model surface (32-byte `GfxPackedVertex`, half-float UVs).
pub fn model_surface(zone: &ZoneData, s: &XSurfInfo) -> ModelMesh {
    let z = &zone.data;
    let mut mesh = ModelMesh::default();
    if let Some(vp) = s.verts_fpos {
        for i in 0..s.vert_count as usize {
            let b = vp + 32 * i;
            if b + 32 > z.len() {
                break;
            }
            let tc = u32::from_le_bytes([z[b + 20], z[b + 21], z[b + 22], z[b + 23]]);
            mesh.vertices.push(Vertex {
                pos: [f32_at(z, b), f32_at(z, b + 4), f32_at(z, b + 8)],
                color: [z[b + 16], z[b + 17], z[b + 18], z[b + 19]],
                uv: [half((tc >> 16) as u16), half((tc & 0xffff) as u16)],
                normal: packed_unit_vec([z[b + 24], z[b + 25], z[b + 26], z[b + 27]]),
                tangent: packed_unit_vec([z[b + 28], z[b + 29], z[b + 30], z[b + 31]]),
                binormal_sign: f32_at(z, b + 12),
            });
        }
    }
    if let Some(tp) = s.tris_fpos {
        for t in 0..s.tri_count as usize {
            let b = tp + 6 * t;
            if b + 6 > z.len() {
                break;
            }
            mesh.triangles.push([u16_at(z, b), u16_at(z, b + 2), u16_at(z, b + 4)]);
        }
    }
    mesh.skin = model_skin(zone, s);
    mesh
}

/// Bone influences per vertex: from the blend stream for skinned surfaces,
/// or from the rigid vertex lists (one bone per run of vertices).
pub fn model_skin(zone: &ZoneData, s: &XSurfInfo) -> Vec<[(u16, f32); 4]> {
    let n = s.vert_count as usize;
    let mut out = vec![[(0u16, 1.0f32), (0, 0.0), (0, 0.0), (0, 0.0)]; n];
    if !s.rigid.is_empty() {
        let mut v = 0usize;
        for l in &s.rigid {
            for _ in 0..l.vert_count {
                if v < n {
                    out[v][0] = (l.bone_offset / 64, 1.0);
                }
                v += 1;
            }
        }
        return out;
    }
    let Some(bp) = s.blend_fpos else { return out };
    let z = &zone.data;
    let mut p = bp;
    let mut rd = || {
        let v = u16_at(z, p);
        p += 2;
        v
    };
    let mut v = 0usize;
    for (influences, count) in s.blend_counts.iter().enumerate() {
        for _ in 0..(*count).max(0) {
            let mut w = [(0u16, 0.0f32); 4];
            w[0].0 = rd() / 64;
            let mut rest = 0.0;
            for slot in w.iter_mut().take(influences + 1).skip(1) {
                slot.0 = rd() / 64;
                slot.1 = rd() as f32 / 65535.0;
                rest += slot.1;
            }
            w[0].1 = (1.0 - rest).max(0.0);
            if v < n {
                out[v] = w;
            }
            v += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats() {
        assert_eq!(half(0x3c00), 1.0);
        assert_eq!(half(0xc000), -2.0);
        assert_eq!(half(0x3800), 0.5);
        assert_eq!(half(0), 0.0);
    }

    #[test]
    fn packed_normals_are_unit() {
        // Straight up: z byte at max, scale byte chosen so |n| ~ 1.
        let n = packed_unit_vec([127, 127, 254, 63]);
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!(n[2] > 0.9 && (len - 1.0).abs() < 0.05, "{n:?}");
    }
}
