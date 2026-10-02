//! The map's collision (`clipMap_t`): convex brushes, terrain/patch
//! triangles, brush models and their contents flags, as the game's physics
//! uses them. Coordinates are game units (inches, Z up).
//!
//! Brushes are stored the way the game traces them: an axial box
//! (`mins`/`maxs`) plus extra bevelled/angled sides. [`brush_polygons`]
//! rebuilds each brush's faces from those planes.

/// Contents flags (`CONTENTS_*`) found on brushes and materials.
pub mod contents {
    pub const SOLID: u32 = 0x1;
    pub const FOLIAGE: u32 = 0x2;
    pub const NONCOLLIDING: u32 = 0x4;
    pub const GLASS: u32 = 0x10;
    pub const WATER: u32 = 0x20;
    pub const CANSHOOTCLIP: u32 = 0x40;
    pub const MISSILECLIP: u32 = 0x80;
    pub const ITEM: u32 = 0x100;
    pub const VEHICLECLIP: u32 = 0x200;
    pub const ITEMCLIP: u32 = 0x400;
    pub const SKY: u32 = 0x800;
    pub const AI_NOSIGHT: u32 = 0x1000;
    pub const CLIPSHOT: u32 = 0x2000;
    pub const MOVER: u32 = 0x4000;
    pub const PLAYERCLIP: u32 = 0x10000;
    pub const MONSTERCLIP: u32 = 0x20000;
    pub const TELEPORTER: u32 = 0x40000;
    pub const JUMPPAD: u32 = 0x80000;
    pub const CLUSTERPORTAL: u32 = 0x100000;
    pub const DONOTENTER: u32 = 0x200000;
    pub const DONOTENTER_LARGE: u32 = 0x400000;
    pub const MANTLE: u32 = 0x1000000;
    pub const DETAIL: u32 = 0x8000000;
    pub const STRUCTURAL: u32 = 0x10000000;
    pub const TRANSPARENT: u32 = 0x20000000;
    pub const TRIGGER: u32 = 0x40000000;
    pub const NODROP: u32 = 0x80000000;

    /// What blocks a player: world solids, glass and player clip.
    pub const PLAYER_SOLID: u32 = SOLID | GLASS | PLAYERCLIP;
    /// What blocks AI movement: world solids, glass and monster clip.
    pub const MONSTER_SOLID: u32 = SOLID | GLASS | MONSTERCLIP;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipPlane {
    pub normal: [f32; 3],
    pub dist: f32,
}

#[derive(Debug, Clone)]
pub struct ClipMaterial {
    pub name: String,
    pub surface_flags: u32,
    pub contents: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ClipSide {
    pub plane: ClipPlane,
    /// Index into [`ClipMapInfo::materials`].
    pub material: u32,
}

#[derive(Debug, Clone)]
pub struct ClipBrush {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub contents: u32,
    /// Materials of the six axial faces: `[min/max][x/y/z]`.
    pub axial_material: [[i16; 3]; 2],
    /// Sides beyond the axial box.
    pub sides: Vec<ClipSide>,
}

/// A collision leaf (also the root of each brush model).
#[derive(Debug, Clone, Copy)]
pub struct ClipLeaf {
    pub first_coll_aabb: u16,
    pub coll_aabb_count: u16,
    pub brush_contents: u32,
    pub terrain_contents: u32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// Index into [`ClipMapInfo::leaf_brush_nodes`] (negative or out of range: none).
    pub leaf_brush_node: i32,
}

#[derive(Debug, Clone)]
pub struct ClipModel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub radius: f32,
    pub leaf: ClipLeaf,
}

/// A node of the per-leaf brush tree: a leaf with brush indices, or a split
/// with two children (offsets relative to this node).
#[derive(Debug, Clone)]
pub struct LeafBrushNode {
    pub axis: u8,
    pub leaf_brush_count: i16,
    pub contents: u32,
    pub brushes: Vec<u16>,
    pub dist: f32,
    pub range: f32,
    pub child_offset: [u16; 2],
}

/// A node of the terrain/patch AABB trees. Leaves (`child_count == 0`)
/// point at a [`ClipPartition`] of triangles.
#[derive(Debug, Clone, Copy)]
pub struct ClipAabbTree {
    pub origin: [f32; 3],
    pub half_size: [f32; 3],
    pub material: u16,
    pub child_count: u16,
    /// First child, or the partition index of a leaf.
    pub index: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct ClipPartition {
    pub first_tri: i32,
    pub tri_count: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ClipMapInfo {
    pub name: String,
    pub materials: Vec<ClipMaterial>,
    pub brushes: Vec<ClipBrush>,
    pub leafs: Vec<ClipLeaf>,
    pub leaf_brush_nodes: Vec<LeafBrushNode>,
    /// Brush models; `models[0]` is the world.
    pub models: Vec<ClipModel>,
    /// Terrain and patch collision triangles.
    pub verts: Vec<[f32; 3]>,
    pub tris: Vec<[u16; 3]>,
    pub partitions: Vec<ClipPartition>,
    pub aabb_trees: Vec<ClipAabbTree>,
}

impl ClipMapInfo {
    /// Brush indices under a leaf's brush tree.
    pub fn leaf_brushes(&self, leaf: &ClipLeaf) -> Vec<u16> {
        let mut out = Vec::new();
        if leaf.leaf_brush_node >= 0 {
            self.collect_node(leaf.leaf_brush_node as usize, &mut out, 0);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    fn collect_node(&self, i: usize, out: &mut Vec<u16>, depth: usize) {
        let Some(n) = self.leaf_brush_nodes.get(i) else { return };
        if depth > 64 {
            return;
        }
        if n.leaf_brush_count > 0 {
            out.extend_from_slice(&n.brushes);
        } else if n.leaf_brush_count == 0 {
            for &c in &n.child_offset {
                if c > 0 {
                    self.collect_node(i + c as usize, out, depth + 1);
                }
            }
        }
    }

    /// Brushes of each brush model (`[0]` = brushes not owned by any
    /// submodel, i.e. the static world).
    pub fn model_brushes(&self) -> Vec<Vec<u16>> {
        let mut owned = vec![false; self.brushes.len()];
        let mut out = vec![Vec::new(); self.models.len().max(1)];
        for (m, model) in self.models.iter().enumerate().skip(1) {
            let list = self.leaf_brushes(&model.leaf);
            for &b in &list {
                if let Some(o) = owned.get_mut(b as usize) {
                    *o = true;
                }
            }
            out[m] = list;
        }
        out[0] = (0..self.brushes.len() as u16).filter(|&b| !owned[b as usize]).collect();
        out
    }

    /// Terrain/patch triangles under an AABB tree range, with each
    /// triangle's material index.
    pub fn terrain_tris(&self, first: usize, count: usize) -> Vec<([[f32; 3]; 3], u16)> {
        self.terrain_of(&[(first, count)])
    }

    /// Terrain/patch triangles of the static world: everything under the
    /// collision leaves' AABB trees, each partition once, except what the
    /// brush models own (their triangles are in model space, see
    /// [`Self::model_terrain`]).
    pub fn world_terrain(&self) -> Vec<([[f32; 3]; 3], u16)> {
        self.model_terrain().swap_remove(0)
    }

    /// Terrain/patch triangles of each brush model (`[0]` = the static
    /// world: partitions no submodel owns). Submodel triangles are in the
    /// model's own space, placed by its entity like its brushes.
    pub fn model_terrain(&self) -> Vec<Vec<([[f32; 3]; 3], u16)>> {
        let mut owned = std::collections::HashSet::new();
        let mut out = vec![Vec::new(); self.models.len().max(1)];
        for (m, model) in self.models.iter().enumerate().skip(1) {
            let parts = self.partitions_of(&[(model.leaf.first_coll_aabb as usize, model.leaf.coll_aabb_count as usize)]);
            owned.extend(parts.iter().map(|p| p.0));
            out[m] = self.partition_tris(&parts);
        }
        let roots: Vec<(usize, usize)> = self.leafs.iter().map(|l| (l.first_coll_aabb as usize, l.coll_aabb_count as usize)).collect();
        let world: Vec<(usize, u16)> = self.partitions_of(&roots).into_iter().filter(|p| !owned.contains(&p.0)).collect();
        out[0] = self.partition_tris(&world);
        out
    }

    fn terrain_of(&self, roots: &[(usize, usize)]) -> Vec<([[f32; 3]; 3], u16)> {
        self.partition_tris(&self.partitions_of(roots))
    }

    /// Partitions (with their material) under AABB tree ranges, each once.
    fn partitions_of(&self, roots: &[(usize, usize)]) -> Vec<(usize, u16)> {
        let mut parts: Vec<(usize, u16)> = Vec::new();
        for &(first, count) in roots {
            for i in first..first + count {
                self.collect_aabb(i, &mut parts, 0);
            }
        }
        parts.sort_unstable();
        parts.dedup_by_key(|p| p.0);
        parts
    }

    fn partition_tris(&self, parts: &[(usize, u16)]) -> Vec<([[f32; 3]; 3], u16)> {
        let mut out = Vec::new();
        for &(pi, material) in parts {
            let p = self.partitions[pi];
            for k in 0..p.tri_count as usize {
                let Some(tri) = self.tris.get(p.first_tri.max(0) as usize + k) else { continue };
                let v = |i: u16| self.verts.get(i as usize).copied();
                if let (Some(a), Some(b), Some(c)) = (v(tri[0]), v(tri[1]), v(tri[2])) {
                    out.push(([a, b, c], material));
                }
            }
        }
        out
    }

    /// Partitions (with their material) under AABB tree node `i`.
    fn collect_aabb(&self, i: usize, out: &mut Vec<(usize, u16)>, depth: usize) {
        let Some(t) = self.aabb_trees.get(i) else { return };
        if depth > 64 {
            return;
        }
        if t.child_count > 0 {
            let first = t.index.max(0) as usize;
            for c in first..first + t.child_count as usize {
                self.collect_aabb(c, out, depth + 1);
            }
            return;
        }
        if (t.index.max(0) as usize) < self.partitions.len() {
            out.push((t.index.max(0) as usize, t.material));
        }
    }

    /// Contents of the material at `i` (0 when out of range).
    pub fn material_contents(&self, i: i64) -> u32 {
        usize::try_from(i).ok().and_then(|i| self.materials.get(i)).map(|m| m.contents).unwrap_or(0)
    }
}

/// One face of a polygonised brush.
#[derive(Debug, Clone)]
pub struct BrushFace {
    /// Convex polygon, counter-clockwise seen from outside.
    pub points: Vec<[f32; 3]>,
    pub normal: [f32; 3],
    /// Material index (`-1` when the side has none).
    pub material: i64,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f64 {
    a[0] as f64 * b[0] as f64 + a[1] as f64 * b[1] as f64 + a[2] as f64 * b[2] as f64
}

/// The faces of a convex brush: its six axial planes plus its extra sides,
/// each a big quad clipped by every other plane.
pub fn brush_polygons(b: &ClipBrush) -> Vec<BrushFace> {
    let mut planes: Vec<(ClipPlane, i64)> = Vec::with_capacity(6 + b.sides.len());
    for axis in 0..3 {
        let mut n = [0.0; 3];
        n[axis] = -1.0;
        planes.push((ClipPlane { normal: n, dist: -b.mins[axis] }, b.axial_material[0][axis] as i64));
        let mut n = [0.0; 3];
        n[axis] = 1.0;
        planes.push((ClipPlane { normal: n, dist: b.maxs[axis] }, b.axial_material[1][axis] as i64));
    }
    for s in &b.sides {
        planes.push((s.plane, s.material as i64));
    }
    let extent: f64 = (0..3).map(|i| (b.maxs[i] - b.mins[i]) as f64).fold(1.0, f64::max) * 4.0;
    let centre: [f64; 3] = std::array::from_fn(|i| (b.mins[i] as f64 + b.maxs[i] as f64) * 0.5);
    let mut faces = Vec::new();
    for (pi, (p, mat)) in planes.iter().enumerate() {
        let n: [f64; 3] = p.normal.map(|v| v as f64);
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len < 1e-6 {
            continue;
        }
        let n = n.map(|v| v / len);
        let d = p.dist as f64 / len;
        // Base quad on the plane, centred on the projection of the brush centre.
        let cd = n[0] * centre[0] + n[1] * centre[1] + n[2] * centre[2];
        let o: [f64; 3] = std::array::from_fn(|i| centre[i] + n[i] * (d - cd));
        let up = if n[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let u = cross(up, n);
        let ul = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        let u = u.map(|v| v / ul);
        let v = cross(n, u);
        let corner = |su: f64, sv: f64| -> [f64; 3] { std::array::from_fn(|i| o[i] + (u[i] * su + v[i] * sv) * extent) };
        // Counter-clockwise around +n: u, v, n is right-handed.
        let mut poly = vec![corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
        for (qi, (q, _)) in planes.iter().enumerate() {
            if qi == pi || poly.is_empty() {
                continue;
            }
            let qn: [f64; 3] = q.normal.map(|v| v as f64);
            let ql = (qn[0] * qn[0] + qn[1] * qn[1] + qn[2] * qn[2]).sqrt();
            if ql < 1e-6 {
                continue;
            }
            let qn = qn.map(|v| v / ql);
            let qd = q.dist as f64 / ql;
            // A coincident plane facing the same way: keep the first one only.
            if (qn[0] * n[0] + qn[1] * n[1] + qn[2] * n[2]) > 1.0 - 1e-6 && (qd - d).abs() < 0.01 {
                if qi < pi {
                    poly.clear();
                }
                continue;
            }
            poly = clip_polygon(&poly, qn, qd);
        }
        if poly.len() < 3 {
            continue;
        }
        faces.push(BrushFace {
            points: poly.iter().map(|p| p.map(|v| v as f32)).collect(),
            normal: n.map(|v| v as f32),
            material: *mat,
        });
    }
    faces
}

/// Keeps the part of a polygon behind the plane (`n·p <= d`).
fn clip_polygon(poly: &[[f64; 3]], n: [f64; 3], d: f64) -> Vec<[f64; 3]> {
    const EPS: f64 = 0.001;
    let dist = |p: &[f64; 3]| n[0] * p[0] + n[1] * p[1] + n[2] * p[2] - d;
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let (da, db) = (dist(&a), dist(&b));
        if da <= EPS {
            out.push(a);
        }
        if (da < -EPS && db > EPS) || (da > EPS && db < -EPS) {
            let t = da / (da - db);
            out.push(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t));
        }
    }
    out
}

/// Whether a point is inside (or on) a brush.
pub fn brush_contains(b: &ClipBrush, p: [f32; 3], eps: f32) -> bool {
    (0..3).all(|i| p[i] >= b.mins[i] - eps && p[i] <= b.maxs[i] + eps) && b.sides.iter().all(|s| dot(s.plane.normal, p) <= s.plane.dist as f64 + eps as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> ClipBrush {
        ClipBrush { mins: [0.0, 0.0, 0.0], maxs: [10.0, 20.0, 30.0], contents: contents::SOLID, axial_material: [[0; 3]; 2], sides: Vec::new() }
    }

    #[test]
    fn box_has_six_quads() {
        let faces = brush_polygons(&cube());
        assert_eq!(faces.len(), 6);
        for f in &faces {
            assert_eq!(f.points.len(), 4, "{f:?}");
            for p in &f.points {
                assert!(brush_contains(&cube(), *p, 0.01), "{p:?}");
            }
            // Winding agrees with the normal.
            let (a, b, c) = (f.points[0], f.points[1], f.points[2]);
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            assert!(dot(n, f.normal) > 0.0, "{f:?}");
        }
    }

    #[test]
    fn ramp_side_cuts_the_box() {
        // A wedge: the box under the plane z = x (normal (-1, 0, 1)/√2 · p <= 0).
        let mut b = cube();
        b.maxs = [10.0, 10.0, 10.0];
        let s = std::f32::consts::FRAC_1_SQRT_2;
        b.sides.push(ClipSide { plane: ClipPlane { normal: [-s, 0.0, s], dist: 0.0 }, material: 0 });
        let faces = brush_polygons(&b);
        // Bottom, max-x, two triangles (y sides) and the slope; the top
        // and min-x faces are cut away.
        assert_eq!(faces.len(), 5, "{faces:#?}");
        let slope = faces.iter().find(|f| f.normal[0] < -0.5).unwrap();
        assert_eq!(slope.points.len(), 4);
        assert!(slope.points.iter().all(|p| (p[2] - p[0]).abs() < 1e-3));
        let tris = faces.iter().filter(|f| f.points.len() == 3).count();
        assert_eq!(tris, 2);
    }

    /// Real-data check of Nacht's collision against its render geometry:
    /// `UNDEAD_WAW=<install> cargo test -p waw_assets -- --ignored`.
    #[test]
    #[ignore]
    fn nacht_clipmap_matches_the_map() {
        use crate::t4::{decode, walk};
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let ff = std::fs::read(std::path::Path::new(&root).join("zone/english/nazi_zombie_prototype.ff")).unwrap();
        let zd = walk(crate::zone::decompress(&ff).unwrap());
        assert!(zd.complete());
        let cm = zd.clipmap.as_ref().unwrap();
        assert_eq!(cm.name, "maps/nazi_zombie_prototype.d3dbsp");
        assert_eq!((cm.brushes.len(), cm.models.len(), cm.materials.len(), cm.tris.len()), (2715, 145, 259, 19500));
        let owned = cm.model_brushes();
        assert_eq!(owned[0].len(), 2549);
        // Window boards are one-brush models, in their own space.
        assert_eq!(owned[2].len(), 1);
        assert!(cm.brushes[owned[2][0] as usize].mins[0] >= -10.0 && cm.brushes[owned[2][0] as usize].maxs[0] <= 10.0);
        let clip = cm.brushes.iter().filter(|b| b.contents & contents::PLAYERCLIP != 0 && b.contents & contents::SOLID == 0).count();
        assert!(clip > 500, "{clip}");

        // Every brush closes into a polyhedron whose faces lie on the brush.
        for (i, b) in cm.brushes.iter().enumerate() {
            let faces = brush_polygons(b);
            assert!(faces.len() >= 4, "brush {i}: {} faces", faces.len());
            for f in &faces {
                assert!(f.points.iter().all(|&p| brush_contains(b, p, 0.05)), "brush {i}: {f:?}");
            }
        }

        // The stairs from the start room up to the help room: 6-unit risers
        // and 9-unit treads, solid, going up towards -x.
        let step = cm
            .brushes
            .iter()
            .find(|b| b.contents & contents::SOLID != 0 && b.mins == [37.5, 1000.0, 49.0] && b.maxs == [46.5, 1106.0, 55.0])
            .expect("stair step");
        assert!(brush_contains(step, [42.0, 1050.0, 54.0], 0.0));

        // Opaque render triangles lie on solid brushes or terrain.
        let terrain = cm.world_terrain();
        assert_eq!(terrain.len(), 19480);
        // Brush models' patches are in model space; they stay out of the
        // world (one used to stand as an invisible wall at the origin).
        let models = cm.model_terrain();
        assert_eq!(models.iter().skip(1).map(Vec::len).sum::<usize>(), 2);
        assert!(!terrain.iter().any(|(t, _)| t.iter().all(|p| p[0].abs() < 60.0 && p[1].abs() < 3.0 && p[2].abs() < 34.0)));
        let w = zd.world.as_ref().unwrap();
        let tri_near = |p: [f32; 3], t: &[[f32; 3]; 3]| {
            let n = {
                let (a, b, c) = (t[0], t[1], t[2]);
                let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]]
            };
            let len = dot(n, n).sqrt();
            if len < 1e-6 {
                return false;
            }
            let d = (dot(n, p) - dot(n, t[0])) / len;
            // On the plane and within the triangle's bounds.
            d.abs() < 1.0 && (0..3).all(|k| p[k] >= t[0][k].min(t[1][k]).min(t[2][k]) - 1.0 && p[k] <= t[0][k].max(t[1][k]).max(t[2][k]) + 1.0)
        };
        let (mut total, mut on) = (0.0f64, 0.0f64);
        for i in (0..w.static_surface_count as usize).step_by(5) {
            let s = &w.surfaces[i];
            let Some(m) = s.material.map(|m| &zd.materials[m as usize]) else { continue };
            let t = m.techset.as_deref().unwrap_or("");
            if w.decal_range.contains(&(i as u32)) || ["sky", "tools", "water", "add", "unlit", "sm_b"].iter().any(|k| t.contains(k)) || m.name.contains("clip") {
                continue;
            }
            for tri in decode::world_triangles(&zd, w, s).iter().step_by(3) {
                let p: Vec<[f32; 3]> = tri.iter().filter_map(|&v| decode::world_vertex(&zd, w, v).map(|x| x.pos)).collect();
                let c = [(p[0][0] + p[1][0] + p[2][0]) / 3.0, (p[0][1] + p[1][1] + p[2][1]) / 3.0, (p[0][2] + p[1][2] + p[2][2]) / 3.0];
                let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
                let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
                let cr = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let area = dot(cr, cr).sqrt() * 0.5;
                total += area;
                let solid = owned[0].iter().any(|&b| {
                    let b = &cm.brushes[b as usize];
                    b.contents & contents::SOLID != 0 && brush_contains(b, c, 1.0)
                }) || terrain.iter().any(|(t, m)| cm.material_contents(*m as i64) & contents::SOLID != 0 && tri_near(c, t));
                on += if solid { area } else { 0.0 };
            }
        }
        // About 97% by area; the rest is unreachable (ceilings above clip),
        // cloth and trims the game itself does not collide with.
        assert!(total > 1e6 && on >= total * 0.93, "{:.1}% of the render area on solid collision", 100.0 * on / total);
    }
}
