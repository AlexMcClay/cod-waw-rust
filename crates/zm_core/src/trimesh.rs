//! Triangle-soup collision for maps built from real level geometry.
//!
//! Triangles are bucketed into a uniform XZ grid. Queries: ray casts (bullets,
//! line of sight, ground probes) and sphere push-out (player and zombie
//! movement). World axes as in [`crate::geom`]: Y up, metres.
//!
//! Every triangle says what it blocks ([`blocks`]). The plain queries
//! (`raycast`, `ground`, `push_sphere`, ...) only see [`blocks::SOLID`]
//! triangles; the `*_mask` queries pick triangles by any set of flags, so a
//! map can carry collision that only stops the player (player clip) or only
//! AI (monster clip) next to its solid geometry.

use crate::geom::V3;

/// What a triangle blocks.
pub mod blocks {
    /// Ordinary solid geometry: everything (seen by the plain queries).
    pub const SOLID: u8 = 1;
    /// Stops the player.
    pub const PLAYER: u8 = 2;
    /// Stops AI movement.
    pub const AI: u8 = 4;
    pub const ALL: u8 = SOLID | PLAYER | AI;
}

#[derive(Debug, Clone, Copy)]
pub struct Tri {
    pub a: V3,
    pub b: V3,
    pub c: V3,
    /// Unit normal (counter-clockwise winding).
    pub n: V3,
    /// [`blocks`] flags ([`blocks::ALL`] unless set otherwise).
    pub blocks: u8,
}

impl Tri {
    pub fn new(a: V3, b: V3, c: V3) -> Option<Tri> {
        let n = b.sub(a).cross(c.sub(a));
        if n.len() < 1e-10 {
            return None;
        }
        Some(Tri { a, b, c, n: n.normalize(), blocks: blocks::ALL })
    }

    /// The same triangle blocking only `flags`.
    pub fn blocking(mut self, flags: u8) -> Tri {
        self.blocks = flags;
        self
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

    /// Nearest hit along a unit-length ray, considering only solid triangles
    /// that pass `accept`.
    pub fn raycast_filtered(&self, o: V3, d: V3, max: f32, accept: impl Fn(&Tri) -> bool) -> Option<Hit> {
        self.raycast_mask(o, d, max, blocks::SOLID, accept)
    }

    /// Nearest hit along a unit-length ray, considering only triangles that
    /// block any of `mask` and pass `accept`.
    pub fn raycast_mask(&self, o: V3, d: V3, max: f32, mask: u8, accept: impl Fn(&Tri) -> bool) -> Option<Hit> {
        let accept = |t: &Tri| t.blocks & mask != 0 && accept(t);
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
        self.ground_mask(x, z, top, bottom, min_up, blocks::SOLID)
    }

    /// [`TriMesh::ground`] over the triangles blocking any of `mask`.
    pub fn ground_mask(&self, x: f32, z: f32, top: f32, bottom: f32, min_up: f32, mask: u8) -> Option<f32> {
        let o = V3::new(x, top, z);
        self.raycast_mask(o, V3::new(0.0, -1.0, 0.0), top - bottom, mask, |t| t.n.y.abs() >= min_up).map(|h| top - h.t)
    }

    /// Pushes a sphere out of every solid triangle passing `accept`. Returns
    /// the corrected centre and whether anything was touched.
    pub fn push_sphere(&self, c: V3, r: f32, iterations: usize, accept: impl Fn(&Tri) -> bool) -> (V3, bool) {
        self.push_sphere_mask(c, r, iterations, blocks::SOLID, accept)
    }

    /// [`TriMesh::push_sphere`] over the triangles blocking any of `mask`.
    pub fn push_sphere_mask(&self, mut c: V3, r: f32, iterations: usize, mask: u8, accept: impl Fn(&Tri) -> bool) -> (V3, bool) {
        let accept = |t: &Tri| t.blocks & mask != 0 && accept(t);
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
    /// Indices of the triangles in the cells overlapping the XZ square
    /// `(x ± r, z ± r)` (a triangle can appear more than once).
    fn near(&self, x: f32, z: f32, r: f32) -> impl Iterator<Item = u32> + '_ {
        let (i0, j0) = self.cell_coord(x - r, z - r);
        let (i1, j1) = self.cell_coord(x + r, z + r);
        let (i0, j0) = (i0.max(0), j0.max(0));
        let (i1, j1) = (i1.min(self.w as i64 - 1), j1.min(self.h as i64 - 1));
        (j0..=j1).flat_map(move |j| (i0..=i1).flat_map(move |i| self.cell_tris(i as usize, j as usize).iter().copied()))
    }

    /// Pushes a vertical cylinder (centre `(x, z)`, radius `r`, from height
    /// `y0` to `y1`) sideways out of the triangles blocking any of `mask`
    /// that pass `accept`. Only the part of each triangle between `y0` and
    /// `y1` counts, so anything lower than `y0` (a step) or above `y1` is
    /// ignored. Returns the corrected `(x, z)` and whether anything was
    /// touched.
    #[allow(clippy::too_many_arguments)]
    pub fn push_cylinder_mask(&self, mut x: f32, mut z: f32, r: f32, y0: f32, y1: f32, iterations: usize, mask: u8, accept: impl Fn(&Tri) -> bool) -> (f32, f32, bool) {
        let mut touched = false;
        let mut seen: Vec<u32> = Vec::new();
        for _ in 0..iterations {
            let mut moved = false;
            seen.clear();
            seen.extend(self.near(x, z, r));
            seen.sort_unstable();
            seen.dedup();
            for &ti in &seen {
                let t = &self.tris[ti as usize];
                if t.blocks & mask == 0 || !accept(t) {
                    continue;
                }
                let poly = clip_to_slab(&[t.a, t.b, t.c], y0, y1);
                if poly.is_empty() {
                    continue;
                }
                let Some((qx, qz, inside)) = closest_2d(&poly, x, z) else { continue };
                let (dx, dz) = (x - qx, z - qz);
                let dist = (dx * dx + dz * dz).sqrt();
                if !inside && dist >= r - 1e-5 {
                    continue;
                }
                // Push direction: away from the closest point, or along the
                // triangle's horizontal normal when exactly on it.
                let (mut ux, mut uz) = if dist > 1e-6 { (dx / dist, dz / dist) } else { (t.n.x, t.n.z) };
                let ul = (ux * ux + uz * uz).sqrt();
                if ul < 1e-6 {
                    continue;
                }
                ux /= ul;
                uz /= ul;
                if inside {
                    // Centre inside the footprint: leave through the nearest edge.
                    x = qx - ux * r;
                    z = qz - uz * r;
                } else {
                    x = qx + ux * r;
                    z = qz + uz * r;
                }
                moved = true;
                touched = true;
            }
            if !moved {
                break;
            }
        }
        (x, z, touched)
    }

    /// Where a sphere of radius `r` lowered at `(x, z)` comes to rest on the
    /// walkable triangles (|normal.y| >= `min_up`) blocking any of `mask`:
    /// the height of the sphere's bottom, between `bottom` and `top`.
    /// Triangles the sphere would rest on above `top` are ignored (too high
    /// to step onto). Unlike a single ray, the result rises smoothly over
    /// step edges, like a ball rolling up stairs.
    #[allow(clippy::too_many_arguments)]
    pub fn support_sphere_mask(&self, x: f32, z: f32, r: f32, top: f32, bottom: f32, min_up: f32, mask: u8) -> Option<f32> {
        let mut best: Option<f32> = None;
        let mut seen: Vec<u32> = self.near(x, z, r).collect();
        seen.sort_unstable();
        seen.dedup();
        for ti in seen {
            let t = &self.tris[ti as usize];
            if t.blocks & mask == 0 || t.n.y.abs() < min_up {
                continue;
            }
            let lo = t.min();
            let hi = t.max();
            if lo.x > x + r || hi.x < x - r || lo.z > z + r || hi.z < z - r || hi.y < bottom || lo.y > top {
                continue;
            }
            let Some(c) = sphere_rest(t, x, z, r) else { continue };
            let feet = c - r;
            if feet <= top + 1e-4 && feet >= bottom && best.is_none_or(|b| feet > b) {
                best = Some(feet);
            }
        }
        best
    }
}

/// The part of a convex polygon between heights `y0` and `y1`.
fn clip_to_slab(poly: &[V3], y0: f32, y1: f32) -> Vec<V3> {
    let clip = |poly: Vec<V3>, keep: &dyn Fn(f32) -> f32| -> Vec<V3> {
        // Keeps the vertices with keep(y) >= 0.
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            let (da, db) = (keep(a.y), keep(b.y));
            if da >= 0.0 {
                out.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                let s = da / (da - db);
                out.push(a.add(b.sub(a).scale(s)));
            }
        }
        out
    };
    let p = clip(poly.to_vec(), &|y| y - y0);
    if p.is_empty() {
        return p;
    }
    clip(p, &|y| y1 - y)
}

/// Closest point of a polygon's XZ projection to `(x, z)`, and whether
/// `(x, z)` lies inside the projection (only for projections with area).
fn closest_2d(poly: &[V3], x: f32, z: f32) -> Option<(f32, f32, bool)> {
    let n = poly.len();
    if n == 0 {
        return None;
    }
    let mut best = (poly[0].x, poly[0].z, f32::MAX);
    let mut area = 0.0;
    let mut sign_pos = true;
    let mut sign_neg = true;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (ex, ez) = (b.x - a.x, b.z - a.z);
        let len2 = ex * ex + ez * ez;
        let s = if len2 > 1e-12 { (((x - a.x) * ex + (z - a.z) * ez) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let (qx, qz) = (a.x + ex * s, a.z + ez * s);
        let d2 = (x - qx) * (x - qx) + (z - qz) * (z - qz);
        if d2 < best.2 {
            best = (qx, qz, d2);
        }
        let cross = ex * (z - a.z) - ez * (x - a.x);
        sign_pos &= cross >= 0.0;
        sign_neg &= cross <= 0.0;
        area += a.x * b.z - b.x * a.z;
    }
    let inside = area.abs() > 1e-6 && (sign_pos || sign_neg);
    Some((best.0, best.1, inside))
}

/// Height of the centre of a sphere of radius `r` lowered from above at
/// `(x, z)` until it touches the triangle, if it touches it at all.
fn sphere_rest(t: &Tri, x: f32, z: f32, r: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    let mut take = |c: f32| {
        if best.is_none_or(|b| c > b) {
            best = Some(c);
        }
    };
    // Face: the contact point is the centre minus r along the (upward) normal.
    let n = if t.n.y < 0.0 { t.n.scale(-1.0) } else { t.n };
    if n.y > 1e-4 {
        let c = (r + n.dot(t.a) - n.x * x - n.z * z) / n.y;
        let p = V3::new(x, c, z).sub(n.scale(r));
        if inside_tri(t, p) {
            take(c);
        }
    }
    // Edges: the centre is at distance r from the segment.
    for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
        let e = b.sub(a);
        let l2 = e.dot(e);
        if l2 < 1e-10 {
            continue;
        }
        let (wx, wz) = (x - a.x, z - a.z);
        let k = wx * e.x + wz * e.z;
        let qa = 1.0 - e.y * e.y / l2;
        if qa < 1e-6 {
            continue; // vertical edge: its end points cover it
        }
        let qb = -2.0 * k * e.y / l2;
        let qc = wx * wx + wz * wz - k * k / l2 - r * r;
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 {
            continue;
        }
        let h = (-qb + disc.sqrt()) / (2.0 * qa);
        let s = (k + h * e.y) / l2;
        if (0.0..=1.0).contains(&s) {
            take(a.y + h);
        }
    }
    // Corners.
    for v in [t.a, t.b, t.c] {
        let d2 = (x - v.x) * (x - v.x) + (z - v.z) * (z - v.z);
        if d2 <= r * r {
            take(v.y + (r * r - d2).sqrt());
        }
    }
    best
}

/// Whether `p` (on the triangle's plane) lies inside the triangle.
fn inside_tri(t: &Tri, p: V3) -> bool {
    let n = t.b.sub(t.a).cross(t.c.sub(t.a));
    let e = 1e-6 * n.dot(n).sqrt();
    let s0 = t.b.sub(t.a).cross(p.sub(t.a)).dot(n);
    let s1 = t.c.sub(t.b).cross(p.sub(t.b)).dot(n);
    let s2 = t.a.sub(t.c).cross(p.sub(t.c)).dot(n);
    s0 >= -e && s1 >= -e && s2 >= -e
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

    fn quad(tris: &mut Vec<Tri>, a: V3, b: V3, c: V3, d: V3, flags: u8) {
        tris.push(Tri::new(a, b, c).unwrap().blocking(flags));
        tris.push(Tri::new(a, c, d).unwrap().blocking(flags));
    }

    /// A box from `lo` to `hi` (all six faces, outward normals).
    fn cube(tris: &mut Vec<Tri>, lo: V3, hi: V3, flags: u8) {
        let v = |x: f32, y: f32, z: f32| V3::new(x, y, z);
        let (a, b) = (lo, hi);
        quad(tris, v(a.x, b.y, a.z), v(a.x, b.y, b.z), v(b.x, b.y, b.z), v(b.x, b.y, a.z), flags); // top
        quad(tris, v(a.x, a.y, a.z), v(b.x, a.y, a.z), v(b.x, a.y, b.z), v(a.x, a.y, b.z), flags); // bottom
        quad(tris, v(b.x, a.y, a.z), v(b.x, b.y, a.z), v(b.x, b.y, b.z), v(b.x, a.y, b.z), flags); // +x
        quad(tris, v(a.x, a.y, a.z), v(a.x, a.y, b.z), v(a.x, b.y, b.z), v(a.x, b.y, a.z), flags); // -x
        quad(tris, v(a.x, a.y, b.z), v(b.x, a.y, b.z), v(b.x, b.y, b.z), v(a.x, b.y, b.z), flags); // +z
        quad(tris, v(a.x, a.y, a.z), v(a.x, b.y, a.z), v(b.x, b.y, a.z), v(b.x, a.y, a.z), flags); // -z
    }

    #[test]
    fn plain_queries_ignore_clip_only_triangles() {
        let mut tris = Vec::new();
        quad(&mut tris, V3::new(-5.0, 0.0, -5.0), V3::new(-5.0, 0.0, 5.0), V3::new(5.0, 0.0, 5.0), V3::new(5.0, 0.0, -5.0), blocks::ALL);
        // Player clip: a slab at 1 m only the player stands on.
        quad(&mut tris, V3::new(-1.0, 1.0, -1.0), V3::new(-1.0, 1.0, 1.0), V3::new(1.0, 1.0, 1.0), V3::new(1.0, 1.0, -1.0), blocks::PLAYER);
        let m = TriMesh::new(tris, 2.0);
        assert!((m.ground(0.0, 0.0, 3.0, -1.0, 0.7).unwrap()).abs() < 1e-5);
        assert!((m.ground_mask(0.0, 0.0, 3.0, -1.0, 0.7, blocks::PLAYER).unwrap() - 1.0).abs() < 1e-5);
        assert!((m.ground_mask(0.0, 0.0, 3.0, -1.0, 0.7, blocks::AI).unwrap()).abs() < 1e-5);
        assert!(m.line_clear(V3::new(0.0, 2.0, 0.0), V3::new(0.0, 0.5, 0.0)));
        assert!(m.raycast_mask(V3::new(0.0, 2.0, 0.0), V3::new(0.0, -1.0, 0.0), 1.5, blocks::PLAYER, |_| true).is_some());
    }

    /// Stairs of 0.15 m risers and 0.23 m treads going up along +x.
    fn stairs() -> TriMesh {
        let mut tris = Vec::new();
        quad(&mut tris, V3::new(-5.0, 0.0, -2.0), V3::new(-5.0, 0.0, 2.0), V3::new(0.0, 0.0, 2.0), V3::new(0.0, 0.0, -2.0), blocks::ALL);
        for i in 0..10 {
            let x0 = i as f32 * 0.23;
            cube(&mut tris, V3::new(x0, 0.0, -1.0), V3::new(x0 + 0.23, 0.15 * (i + 1) as f32, 1.0), blocks::ALL);
        }
        TriMesh::new(tris, 1.0)
    }

    #[test]
    fn sphere_support_rides_up_stairs_smoothly() {
        let m = stairs();
        let r = 0.38;
        let mut feet = 0.0f32;
        let mut x = -1.0;
        let mut max_jump = 0.0f32;
        while x < 2.0 {
            let g = m.support_sphere_mask(x, 0.0, r, feet + 0.46, feet - 2.0, 0.7, blocks::PLAYER).unwrap();
            assert!(g >= feet - 1e-4, "went down at {x}: {g} < {feet}");
            max_jump = max_jump.max(g - feet);
            feet = g;
            x += 0.01;
        }
        // Flat floor far from the stairs, and the top tread under the centre.
        assert!(m.support_sphere_mask(-2.0, 0.0, r, 0.46, -1.0, 0.7, blocks::PLAYER).unwrap().abs() < 1e-5);
        assert!((feet - 1.5).abs() < 0.01, "{feet}");
        // A ray would jump 0.15 m at every riser; the sphere never jumps
        // more than a few centimetres per centimetre travelled.
        assert!(max_jump < 0.06, "{max_jump}");
        // Too high to step onto: a 1 m block is ignored, the floor is found.
        let mut tris = Vec::new();
        quad(&mut tris, V3::new(-5.0, 0.0, -5.0), V3::new(-5.0, 0.0, 5.0), V3::new(5.0, 0.0, 5.0), V3::new(5.0, 0.0, -5.0), blocks::ALL);
        cube(&mut tris, V3::new(0.0, 0.0, -1.0), V3::new(1.0, 1.0, 1.0), blocks::ALL);
        let m = TriMesh::new(tris, 1.0);
        assert!(m.support_sphere_mask(-0.2, 0.0, r, 0.46, -1.0, 0.7, blocks::PLAYER).unwrap().abs() < 1e-5);
    }

    #[test]
    fn cylinder_push_ignores_what_lies_below_the_step() {
        let mut tris = Vec::new();
        // A wall at x = 3 and a 0.2 m kerb from x = 1.
        quad(&mut tris, V3::new(3.0, 0.0, -5.0), V3::new(3.0, 3.0, -5.0), V3::new(3.0, 3.0, 5.0), V3::new(3.0, 0.0, 5.0), blocks::ALL);
        cube(&mut tris, V3::new(1.0, 0.0, -1.0), V3::new(2.0, 0.2, 1.0), blocks::PLAYER);
        let m = TriMesh::new(tris, 2.0);
        let (x, z, hit) = m.push_cylinder_mask(2.8, 0.0, 0.4, 0.45, 1.8, 3, blocks::PLAYER, |t| t.n.y.abs() < 0.7);
        assert!(hit && (x - 2.6).abs() < 1e-4 && z.abs() < 1e-4, "{x} {z}");
        let (x, _, hit) = m.push_cylinder_mask(0.9, 0.0, 0.4, 0.45, 1.8, 3, blocks::PLAYER, |t| t.n.y.abs() < 0.7);
        assert!(!hit && (x - 0.9).abs() < 1e-6);
        // From the floor the kerb is in the way when the band starts low.
        let (x, _, hit) = m.push_cylinder_mask(0.9, 0.0, 0.4, 0.05, 1.8, 3, blocks::PLAYER, |t| t.n.y.abs() < 0.7);
        assert!(hit && (x - 0.6).abs() < 1e-4, "{x}");
    }
}
