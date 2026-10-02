//! Triangle-soup collision for maps built from real level geometry.
//!
//! Triangles are bucketed into a uniform XZ grid. Queries: ray casts (bullets,
//! line of sight, ground probes) and sphere push-out (player and zombie
//! movement). World axes as in [`crate::geom`]: Y up, metres.

use crate::geom::V3;

#[derive(Debug, Clone, Copy)]
pub struct Tri {
    pub a: V3,
    pub b: V3,
    pub c: V3,
    /// Unit normal (counter-clockwise winding).
    pub n: V3,
}

impl Tri {
    pub fn new(a: V3, b: V3, c: V3) -> Option<Tri> {
        let n = b.sub(a).cross(c.sub(a));
        if n.len() < 1e-10 {
            return None;
        }
        Some(Tri { a, b, c, n: n.normalize() })
    }

    fn min(&self) -> V3 {
        self.a.min(self.b).min(self.c)
    }

    fn max(&self) -> V3 {
        self.a.max(self.b).max(self.c)
    }

    /// Ray/triangle intersection (two-sided). Returns the distance.
    pub fn ray(&self, o: V3, d: V3, max: f32) -> Option<f32> {
        let e1 = self.b.sub(self.a);
        let e2 = self.c.sub(self.a);
        let p = d.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let s = o.sub(self.a);
        let u = s.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = s.cross(e1);
        let v = d.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = e2.dot(q) * inv;
        (t >= 0.0 && t <= max).then_some(t)
    }

    /// Closest point on the triangle to `p` (Ericson, Real-Time Collision Detection 5.1.5).
    pub fn closest_point(&self, p: V3) -> V3 {
        let (a, b, c) = (self.a, self.b, self.c);
        let ab = b.sub(a);
        let ac = c.sub(a);
        let ap = p.sub(a);
        let d1 = ab.dot(ap);
        let d2 = ac.dot(ap);
        if d1 <= 0.0 && d2 <= 0.0 {
            return a;
        }
        let bp = p.sub(b);
        let d3 = ab.dot(bp);
        let d4 = ac.dot(bp);
        if d3 >= 0.0 && d4 <= d3 {
            return b;
        }
        let vc = d1 * d4 - d3 * d2;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            return a.add(ab.scale(d1 / (d1 - d3)));
        }
        let cp = p.sub(c);
        let d5 = ab.dot(cp);
        let d6 = ac.dot(cp);
        if d6 >= 0.0 && d5 <= d6 {
            return c;
        }
        let vb = d5 * d2 - d1 * d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            return a.add(ac.scale(d2 / (d2 - d6)));
        }
        let va = d3 * d6 - d5 * d4;
        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
            return b.add(c.sub(b).scale((d4 - d3) / ((d4 - d3) + (d5 - d6))));
        }
        let denom = 1.0 / (va + vb + vc);
        a.add(ab.scale(vb * denom)).add(ac.scale(vc * denom))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub t: f32,
    pub normal: V3,
    pub tri: u32,
}

/// Triangle soup with a uniform XZ grid.
#[derive(Debug, Clone)]
pub struct TriMesh {
    pub tris: Vec<Tri>,
    cell: f32,
    x0: f32,
    z0: f32,
    w: usize,
    h: usize,
    /// CSR layout: triangles of cell `i` are `index[start[i]..start[i + 1]]`.
    start: Vec<u32>,
    index: Vec<u32>,
    pub min: V3,
    pub max: V3,
}

impl TriMesh {
    pub fn new(tris: Vec<Tri>, cell: f32) -> TriMesh {
        let mut min = V3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = V3::new(f32::MIN, f32::MIN, f32::MIN);
        for t in &tris {
            min = min.min(t.min());
            max = max.max(t.max());
        }
        if tris.is_empty() {
            min = V3::new(0.0, 0.0, 0.0);
            max = min;
        }
        let x0 = min.x - cell;
        let z0 = min.z - cell;
        let w = (((max.x - x0) / cell).ceil() as usize + 2).max(1);
        let h = (((max.z - z0) / cell).ceil() as usize + 2).max(1);
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); w * h];
        for (i, t) in tris.iter().enumerate() {
            let (lo, hi) = (t.min(), t.max());
            let (i0, j0) = ((((lo.x - x0) / cell) as usize), (((lo.z - z0) / cell) as usize));
            let (i1, j1) = ((((hi.x - x0) / cell) as usize).min(w - 1), (((hi.z - z0) / cell) as usize).min(h - 1));
            for j in j0..=j1 {
                for ii in i0..=i1 {
                    buckets[j * w + ii].push(i as u32);
                }
            }
        }
        let mut start = Vec::with_capacity(w * h + 1);
        let mut index = Vec::new();
        for b in &buckets {
            start.push(index.len() as u32);
            index.extend_from_slice(b);
        }
        start.push(index.len() as u32);
        TriMesh { tris, cell, x0, z0, w, h, start, index, min, max }
    }

    fn cell_tris(&self, i: usize, j: usize) -> &[u32] {
        let c = j * self.w + i;
        &self.index[self.start[c] as usize..self.start[c + 1] as usize]
    }

    fn cell_coord(&self, x: f32, z: f32) -> (i64, i64) {
        (((x - self.x0) / self.cell).floor() as i64, ((z - self.z0) / self.cell).floor() as i64)
    }

    fn in_grid(&self, i: i64, j: i64) -> bool {
        i >= 0 && j >= 0 && (i as usize) < self.w && (j as usize) < self.h
    }

    /// Nearest hit along a unit-length ray, considering only triangles that
    /// pass `accept`.
    pub fn raycast_filtered(&self, o: V3, d: V3, max: f32, accept: impl Fn(&Tri) -> bool) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        let test_cell = |i: i64, j: i64, best: &mut Option<Hit>| {
            if !self.in_grid(i, j) {
                return;
            }
            for &ti in self.cell_tris(i as usize, j as usize) {
                let t = &self.tris[ti as usize];
                if !accept(t) {
                    continue;
                }
                let limit = best.map(|b| b.t).unwrap_or(max);
                if let Some(dist) = t.ray(o, d, limit) {
                    if best.is_none_or(|b| dist < b.t) {
                        *best = Some(Hit { t: dist, normal: t.n, tri: ti });
                    }
                }
            }
        };
        // 2D DDA across XZ cells (Amanatides & Woo).
        let (mut i, mut j) = self.cell_coord(o.x, o.z);
        let step_i: i64 = if d.x > 0.0 { 1 } else { -1 };
        let step_j: i64 = if d.z > 0.0 { 1 } else { -1 };
        let next_boundary = |c: i64, step: i64, origin: f32, base: f32| base + (c + if step > 0 { 1 } else { 0 }) as f32 * self.cell - origin;
        let mut t_max_x = if d.x.abs() < 1e-9 { f32::INFINITY } else { next_boundary(i, step_i, o.x, self.x0) / d.x };
        let mut t_max_z = if d.z.abs() < 1e-9 { f32::INFINITY } else { next_boundary(j, step_j, o.z, self.z0) / d.z };
        let t_dx = if d.x.abs() < 1e-9 { f32::INFINITY } else { self.cell / d.x.abs() };
        let t_dz = if d.z.abs() < 1e-9 { f32::INFINITY } else { self.cell / d.z.abs() };
        let mut t_cell_start = 0.0f32;
        for _ in 0..(self.w + self.h + 4) * 2 {
            test_cell(i, j, &mut best);
            let t_cell_end = t_max_x.min(t_max_z);
            if let Some(b) = best {
                if b.t <= t_cell_end {
                    break;
                }
            }
            if t_cell_start > max || t_cell_end > max {
                break;
            }
            if t_max_x < t_max_z {
                i += step_i;
                t_cell_start = t_max_x;
                t_max_x += t_dx;
            } else {
                j += step_j;
                t_cell_start = t_max_z;
                t_max_z += t_dz;
            }
            // Left the grid in the direction of travel.
            if (i < 0 && step_i < 0) || (j < 0 && step_j < 0) || (i as usize >= self.w && step_i > 0) || (j as usize >= self.h && step_j > 0) {
                break;
            }
        }
        best
    }

    pub fn raycast(&self, o: V3, d: V3, max: f32) -> Option<Hit> {
        self.raycast_filtered(o, d, max, |_| true)
    }

    /// True if nothing blocks the segment between `a` and `b`.
    pub fn line_clear(&self, a: V3, b: V3) -> bool {
        let v = b.sub(a);
        let len = v.len();
        len < 1e-6 || self.raycast(a, v.scale(1.0 / len), len).is_none()
    }

    /// Height of the highest walkable surface (normal.y >= `min_up`) below
    /// `(x, top, z)`, searching down to `bottom`.
    pub fn ground(&self, x: f32, z: f32, top: f32, bottom: f32, min_up: f32) -> Option<f32> {
        let o = V3::new(x, top, z);
        self.raycast_filtered(o, V3::new(0.0, -1.0, 0.0), top - bottom, |t| t.n.y.abs() >= min_up).map(|h| top - h.t)
    }

    /// Pushes a sphere out of every triangle passing `accept`. Returns the
    /// corrected centre and whether anything was touched.
    pub fn push_sphere(&self, mut c: V3, r: f32, iterations: usize, accept: impl Fn(&Tri) -> bool) -> (V3, bool) {
        let mut touched = false;
        for _ in 0..iterations {
            let (i0, j0) = self.cell_coord(c.x - r, c.z - r);
            let (i1, j1) = self.cell_coord(c.x + r, c.z + r);
            let mut moved = false;
            for j in j0.max(0)..=j1.min(self.h as i64 - 1) {
                for i in i0.max(0)..=i1.min(self.w as i64 - 1) {
                    for &ti in self.cell_tris(i as usize, j as usize) {
                        let t = &self.tris[ti as usize];
                        if !accept(t) {
                            continue;
                        }
                        let p = t.closest_point(c);
                        let d = c.sub(p);
                        let dist = d.len();
                        if dist < r - 1e-5 {
                            let dir = if dist > 1e-6 {
                                d.scale(1.0 / dist)
                            } else if t.n.dot(c.sub(t.a)) >= 0.0 {
                                t.n
                            } else {
                                t.n.scale(-1.0)
                            };
                            c = p.add(dir.scale(r));
                            moved = true;
                            touched = true;
                        }
                    }
                }
            }
            if !moved {
                break;
            }
        }
        (c, touched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10x10 floor at y=0 and a wall at x=3 (normal -X), 3 m tall.
    fn room() -> TriMesh {
        let v = V3::new;
        let mut tris = Vec::new();
        let quad = |tris: &mut Vec<Tri>, a: V3, b: V3, c: V3, d: V3| {
            tris.push(Tri::new(a, b, c).unwrap());
            tris.push(Tri::new(a, c, d).unwrap());
        };
        quad(&mut tris, v(-5.0, 0.0, -5.0), v(-5.0, 0.0, 5.0), v(5.0, 0.0, 5.0), v(5.0, 0.0, -5.0));
        quad(&mut tris, v(3.0, 0.0, -5.0), v(3.0, 3.0, -5.0), v(3.0, 3.0, 5.0), v(3.0, 0.0, 5.0));
        // A 0.3 m step at z > 2 (top surface only).
        quad(&mut tris, v(-5.0, 0.3, 2.0), v(-5.0, 0.3, 5.0), v(0.0, 0.3, 5.0), v(0.0, 0.3, 2.0));
        TriMesh::new(tris, 2.0)
    }

    #[test]
    fn rays_hit_nearest() {
        let m = room();
        let h = m.raycast(V3::new(-4.0, 1.0, 0.0), V3::new(1.0, 0.0, 0.0), 50.0).unwrap();
        assert!((h.t - 7.0).abs() < 1e-4, "{h:?}");
        assert!(m.raycast(V3::new(-4.0, 1.0, 0.0), V3::new(-1.0, 0.0, 0.0), 50.0).is_none());
        assert!(m.raycast(V3::new(-4.0, 1.0, 0.0), V3::new(1.0, 0.0, 0.0), 6.0).is_none());
        // Diagonal through several cells.
        let d = V3::new(1.0, 0.0, 0.3).normalize();
        assert!(m.raycast(V3::new(-4.9, 2.0, -4.0), d, 50.0).is_some());
        assert!(m.line_clear(V3::new(-4.0, 1.0, 0.0), V3::new(2.0, 1.0, 0.0)));
        assert!(!m.line_clear(V3::new(-4.0, 1.0, 0.0), V3::new(4.0, 1.0, 0.0)));
    }

    #[test]
    fn ground_probe_finds_highest_floor() {
        let m = room();
        assert!((m.ground(0.0, 0.0, 2.0, -2.0, 0.6).unwrap()).abs() < 1e-4);
        assert!((m.ground(-2.0, 3.0, 2.0, -2.0, 0.6).unwrap() - 0.3).abs() < 1e-4);
        assert!(m.ground(20.0, 0.0, 2.0, -2.0, 0.6).is_none());
    }

    #[test]
    fn spheres_are_pushed_out_of_walls() {
        let m = room();
        let (c, touched) = m.push_sphere(V3::new(2.8, 1.0, 0.0), 0.4, 4, |t| t.n.y.abs() < 0.7);
        assert!(touched);
        assert!((c.x - 2.6).abs() < 1e-3, "{c:?}");
        let (c, touched) = m.push_sphere(V3::new(1.0, 1.0, 0.0), 0.4, 4, |t| t.n.y.abs() < 0.7);
        assert!(!touched);
        assert_eq!(c, V3::new(1.0, 1.0, 0.0));
    }

    #[test]
    fn closest_point_regions() {
        let t = Tri::new(V3::new(0.0, 0.0, 0.0), V3::new(1.0, 0.0, 0.0), V3::new(0.0, 0.0, 1.0)).unwrap();
        let close = |a: V3, b: V3| a.sub(b).len() < 1e-5;
        assert!(close(t.closest_point(V3::new(0.2, 5.0, 0.2)), V3::new(0.2, 0.0, 0.2)));
        assert!(close(t.closest_point(V3::new(-1.0, 0.0, -1.0)), V3::new(0.0, 0.0, 0.0)));
        let e = t.closest_point(V3::new(1.0, 0.0, 1.0));
        assert!((e.x - 0.5).abs() < 1e-5 && (e.z - 0.5).abs() < 1e-5);
    }
}
