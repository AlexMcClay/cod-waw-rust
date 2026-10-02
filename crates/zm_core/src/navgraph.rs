//! Waypoint-graph navigation for maps that come with path nodes (the real
//! Nacht der Untoten ships 634 of them in its entity list).
//!
//! Zombies share one Dijkstra field toward the player (seeded from every
//! node the player can walk to) that is refreshed a few times per second;
//! each zombie then walks along decreasing field values, the same "every
//! zombie always knows where the player is" behaviour as the original's
//! `find_flesh`. Edges can be gated by a door so they only count once that
//! door has been bought. Costs are 3D distances.
//!
//! [`walkable`] is the shared test for "can a zombie walk straight from A to
//! B": a ground profile sampled along the segment (no step or drop larger
//! than [`STEP`], no holes) plus body-width line checks at knee and chest
//! height. Graph links and direct pursuit both use it, so links never join
//! two floors unless a walkable slope (stairs, ramp) connects them.

use crate::geom::V3;
use crate::trimesh::TriMesh;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Highest step a zombie walks up or down without a traversal (WaW's AI
/// step size, 18 units).
pub const STEP: f32 = 0.46;
/// Spacing of the ground samples along a walk.
const SAMPLE: f32 = 0.3;
/// Knee and chest heights for the line checks, above the ground profile.
const KNEE: f32 = 0.5;
const CHEST: f32 = 1.3;

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub to: u32,
    pub cost: f32,
    /// Door that must be open for this edge to be usable.
    pub gate: Option<u16>,
}

#[derive(Debug, Clone, Default)]
pub struct NavGraph {
    pub nodes: Vec<V3>,
    pub edges: Vec<Vec<Edge>>,
}

#[derive(Copy, Clone, PartialEq)]
struct Item {
    cost: f32,
    node: u32,
}
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Distance with vertical offsets weighted up, so a node on another floor
/// right above or below counts as far away.
pub fn floor_dist(a: V3, b: V3) -> f32 {
    let d = a.sub(b);
    (d.x * d.x + d.z * d.z + (d.y * 3.0).powi(2)).sqrt()
}

/// Ground height under `(p.x, p.z)` within a step of `p.y` (up) or `down`.
pub fn ground_near(mesh: &TriMesh, p: V3, down: f32) -> Option<f32> {
    mesh.ground(p.x, p.z, p.y + STEP + 0.1, p.y - down, 0.6)
}

/// Ground heights sampled along a straight walk from `a` to `b` (both feet
/// positions), or `None` when the walk meets a hole, a drop or a climb
/// bigger than [`STEP`]. The first entry is the ground under `a`.
pub fn ground_profile(mesh: &TriMesh, a: V3, b: V3) -> Option<Vec<f32>> {
    let ga = ground_near(mesh, a, 1.2)?;
    let gb = ground_near(mesh, b, 1.2)?;
    let flat = ((b.x - a.x).powi(2) + (b.z - a.z).powi(2)).sqrt();
    let n = (flat / SAMPLE).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(n + 1);
    out.push(ga);
    let mut prev = ga;
    for i in 1..=n {
        let t = i as f32 / n as f32;
        let (x, z) = (a.x + (b.x - a.x) * t, a.z + (b.z - a.z) * t);
        let g = mesh.ground(x, z, prev + climb_limit(prev, gb) + 0.02, prev - STEP - 0.02, 0.6)?;
        out.push(g);
        prev = g;
    }
    ((prev - gb).abs() <= STEP).then_some(out)
}

/// How far a walk heading for ground height `to` may step up from `from`:
/// a full step while the destination is higher, but never much above the
/// destination's height, so a walk along the open side of a staircase
/// stays on the floor instead of wandering up the steps.
pub fn climb_limit(from: f32, to: f32) -> f32 {
    (to - from + 0.25).clamp(0.08, STEP)
}

/// True when a zombie can walk straight from feet position `a` to `b`:
/// continuous ground without big steps or drops, and nothing in the way at
/// knee height (body wide) or chest height.
pub fn walkable(mesh: &TriMesh, a: V3, b: V3) -> bool {
    let flat = V3::new(b.x - a.x, 0.0, b.z - a.z);
    let len = flat.len();
    if len < 1e-3 {
        return (a.y - b.y).abs() <= STEP;
    }
    let Some(profile) = ground_profile(mesh, a, b) else { return false };
    let side = V3::new(-flat.z / len, 0.0, flat.x / len).scale(0.2);
    let zero = V3::new(0.0, 0.0, 0.0);
    let n = profile.len() - 1;
    let point = |i: usize| {
        let t = i as f32 / n as f32;
        V3::new(a.x + (b.x - a.x) * t, profile[i], a.z + (b.z - a.z) * t)
    };
    // Line checks per stretch of ground; on slopes and stairs the lines are
    // lifted by how far the ground bulges above the stretch's chord, so
    // stair nosings don't count as walls.
    let mut i = 0;
    while i < n {
        let j = (i + CHUNK).min(n);
        let (p, q) = (point(i), point(j));
        let lift = (i..=j).map(|k| profile[k] - (p.y + (q.y - p.y) * (k - i) as f32 / (j - i) as f32)).fold(0.0f32, f32::max);
        let clear = [(KNEE, zero), (KNEE, side), (KNEE, side.scale(-1.0)), (CHEST, zero)].iter().all(|&(h, off)| {
            let up = V3::new(off.x, h + lift, off.z);
            mesh.line_clear(p.add(up), q.add(up))
        });
        if !clear {
            return false;
        }
        i = j;
    }
    // Rays slip between bars and railings: sweep the body (two spheres,
    // above step height) along the profile.
    let r = BODY + SWEEP_MARGIN;
    let step = r * 0.8;
    let total = len;
    let count = (total / step).ceil().max(1.0) as usize;
    (0..=count).all(|k| {
        let t = k as f32 / count as f32;
        let fi = t * n as f32;
        let (i0, i1) = (fi.floor() as usize, (fi.ceil() as usize).min(n));
        let g = profile[i0] + (profile[i1] - profile[i0]) * (fi - i0 as f32);
        let (x, z) = (a.x + (b.x - a.x) * t, a.z + (b.z - a.z) * t);
        BODY_HEIGHTS.iter().all(|h| !mesh.push_sphere(V3::new(x, g + h, z), r, 1, |tri| tri.n.y.abs() < 0.7).1)
    })
}

/// Body radius of a zombie for wall collision. [`walkable`] sweeps a
/// slightly fatter body so a walk it accepts is never blocked by
/// collision.
pub const BODY: f32 = 0.26;
const SWEEP_MARGIN: f32 = 0.04;
/// Heights (above the ground) of the two body spheres: the lower one sits
/// just above [`STEP`] so steps and kerbs are walked onto, not bumped into.
pub const BODY_HEIGHTS: [f32; 2] = [STEP + BODY + SWEEP_MARGIN + 0.02, CHEST + 0.1];

/// Ground samples per line-check stretch in [`walkable`].
const CHUNK: usize = 4;

impl NavGraph {
    /// Links every pair of nodes closer than `max_link` for which `link`
    /// returns `Some(gate)` (`Some(None)` = always open, `None` = no edge).
    pub fn build(nodes: Vec<V3>, max_link: f32, link: impl Fn(V3, V3) -> Option<Option<u16>>) -> NavGraph {
        let mut edges = vec![Vec::new(); nodes.len()];
        for a in 0..nodes.len() {
            for b in a + 1..nodes.len() {
                let d = nodes[a].sub(nodes[b]).len();
                if d > max_link {
                    continue;
                }
                if let Some(gate) = link(nodes[a], nodes[b]) {
                    edges[a].push(Edge { to: b as u32, cost: d, gate });
                    edges[b].push(Edge { to: a as u32, cost: d, gate });
                }
            }
        }
        NavGraph { nodes, edges }
    }

    /// Adds a one-off edge (e.g. a window traversal).
    pub fn connect(&mut self, a: usize, b: usize, gate: Option<u16>) {
        if a == b || self.edges[a].iter().any(|e| e.to as usize == b) {
            return;
        }
        let cost = self.nodes[a].sub(self.nodes[b]).len();
        self.edges[a].push(Edge { to: b as u32, cost, gate });
        self.edges[b].push(Edge { to: a as u32, cost, gate });
    }

    pub fn link_count(&self) -> usize {
        self.edges.iter().map(Vec::len).sum::<usize>() / 2
    }

    fn usable(e: &Edge, open: &[bool]) -> bool {
        e.gate.is_none_or(|g| open.get(g as usize).copied().unwrap_or(false))
    }

    /// Nodes ordered by [`floor_dist`] from `p` (nearest first).
    pub fn by_distance(&self, p: V3) -> Vec<(f32, usize)> {
        let mut order: Vec<(f32, usize)> = self.nodes.iter().enumerate().map(|(i, n)| (floor_dist(*n, p), i)).collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        order
    }

    /// Closest node accepted by `visible` (e.g. line of sight) among the 16
    /// nearest, preferring nodes at a similar height; falls back to the
    /// nearest one on the same floor, then to the nearest overall.
    pub fn nearest(&self, p: V3, visible: impl Fn(V3) -> bool) -> Option<usize> {
        let order = self.by_distance(p);
        order
            .iter()
            .take(16)
            .find(|(_, i)| visible(self.nodes[*i]))
            .or_else(|| order.iter().find(|(_, i)| (self.nodes[*i].y - p.y).abs() < 1.0))
            .or(order.first())
            .map(|(_, i)| *i)
    }

    /// Dijkstra distances from `target` over edges whose gate is open.
    pub fn field(&self, target: usize, open: &[bool]) -> Vec<f32> {
        self.field_from(&[(target, 0.0)], open)
    }

    /// Multi-source Dijkstra: distances to the closest seed, where each seed
    /// starts at its own cost (e.g. its distance to the player).
    pub fn field_from(&self, seeds: &[(usize, f32)], open: &[bool]) -> Vec<f32> {
        let mut dist = vec![f32::INFINITY; self.nodes.len()];
        let mut heap = BinaryHeap::new();
        for &(s, c) in seeds {
            if s < dist.len() && c < dist[s] {
                dist[s] = c;
                heap.push(Item { cost: c, node: s as u32 });
            }
        }
        while let Some(Item { cost, node }) = heap.pop() {
            if cost > dist[node as usize] {
                continue;
            }
            for e in &self.edges[node as usize] {
                if !Self::usable(e, open) {
                    continue;
                }
                let nc = cost + e.cost;
                if nc < dist[e.to as usize] {
                    dist[e.to as usize] = nc;
                    heap.push(Item { cost: nc, node: e.to });
                }
            }
        }
        dist
    }

    /// Next node to walk to from `node` following `field` (the neighbour
    /// that is closest to the target), or `None` when `node` is a seed (no
    /// neighbour is closer) or unreachable.
    pub fn next(&self, node: usize, field: &[f32], open: &[bool]) -> Option<usize> {
        let here = *field.get(node)?;
        if !here.is_finite() {
            return None;
        }
        self.edges[node]
            .iter()
            .filter(|e| Self::usable(e, open))
            .filter(|e| field[e.to as usize] < here - 1e-4)
            .map(|e| (field[e.to as usize] + e.cost, e.to as usize))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, n)| n)
    }

    /// The whole route from `node` down `field` (including `node`).
    pub fn route(&self, node: usize, field: &[f32], open: &[bool]) -> Vec<usize> {
        let mut out = vec![node];
        let mut n = node;
        while let Some(m) = self.next(n, field, open) {
            if out.len() > self.nodes.len() {
                break;
            }
            out.push(m);
            n = m;
        }
        out
    }

    /// A* shortest path between two nodes over open edges.
    pub fn astar(&self, from: usize, to: usize, open: &[bool]) -> Option<Vec<usize>> {
        let n = self.nodes.len();
        if from >= n || to >= n {
            return None;
        }
        let h = |i: usize| self.nodes[i].sub(self.nodes[to]).len();
        let mut g = vec![f32::INFINITY; n];
        let mut came = vec![usize::MAX; n];
        let mut heap = BinaryHeap::new();
        g[from] = 0.0;
        heap.push(Item { cost: h(from), node: from as u32 });
        while let Some(Item { cost, node }) = heap.pop() {
            let u = node as usize;
            if u == to {
                let mut path = vec![to];
                let mut c = to;
                while c != from {
                    c = came[c];
                    path.push(c);
                }
                path.reverse();
                return Some(path);
            }
            if cost > g[u] + h(u) + 1e-4 {
                continue;
            }
            for e in &self.edges[u] {
                if !Self::usable(e, open) {
                    continue;
                }
                let v = e.to as usize;
                let ng = g[u] + e.cost;
                if ng < g[v] {
                    g[v] = ng;
                    came[v] = u;
                    heap.push(Item { cost: ng + h(v), node: e.to });
                }
            }
        }
        None
    }

    /// Connected components over all edges (doors counted as open).
    pub fn components(&self) -> Vec<u32> {
        let mut comp = vec![u32::MAX; self.nodes.len()];
        let mut next = 0;
        for s in 0..self.nodes.len() {
            if comp[s] != u32::MAX {
                continue;
            }
            let mut stack = vec![s];
            comp[s] = next;
            while let Some(u) = stack.pop() {
                for e in &self.edges[u] {
                    if comp[e.to as usize] == u32::MAX {
                        comp[e.to as usize] = next;
                        stack.push(e.to as usize);
                    }
                }
            }
            next += 1;
        }
        comp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trimesh::Tri;

    /// Nodes on a line 0..5 (2 m apart) with a "door" between 2 and 3.
    fn corridor() -> NavGraph {
        let nodes: Vec<V3> = (0..6).map(|i| V3::new(i as f32 * 2.0, 0.0, 0.0)).collect();
        NavGraph::build(nodes, 2.5, |a, b| Some(if a.x.min(b.x) == 4.0 { Some(0) } else { None }))
    }

    #[test]
    fn doors_gate_paths() {
        let g = corridor();
        let closed = g.field(5, &[false]);
        assert!(closed[0].is_infinite());
        assert!((closed[3] - 4.0).abs() < 1e-5);
        let open = g.field(5, &[true]);
        assert!((open[0] - 10.0).abs() < 1e-5);
        assert_eq!(g.next(0, &open, &[true]), Some(1));
        assert_eq!(g.next(2, &open, &[true]), Some(3));
        assert_eq!(g.next(5, &open, &[true]), None);
        assert_eq!(g.next(0, &closed, &[false]), None);
        assert_eq!(g.astar(0, 5, &[false]), None);
        assert_eq!(g.astar(0, 5, &[true]), Some(vec![0, 1, 2, 3, 4, 5]));
    }

    #[test]
    fn nearest_respects_visibility_and_floors() {
        let g = corridor();
        assert_eq!(g.nearest(V3::new(4.2, 0.0, 0.0), |_| true), Some(2));
        assert_eq!(g.nearest(V3::new(4.2, 0.0, 0.0), |n| n.x > 5.0), Some(3));
        // A node right overhead (another floor) loses to one on this floor.
        let g = NavGraph::build(vec![V3::new(0.0, 3.0, 0.0), V3::new(2.0, 0.0, 0.0)], 1.0, |_, _| Some(None));
        assert_eq!(g.nearest(V3::new(0.0, 0.0, 0.0), |_| false), Some(1));
    }

    #[test]
    fn walking_reaches_the_target() {
        let g = corridor();
        let open = [true];
        let f = g.field(5, &open);
        assert_eq!(g.route(0, &f, &open), vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn multi_seed_fields_pick_the_closest_seed() {
        let g = corridor();
        let f = g.field_from(&[(0, 3.0), (5, 0.5)], &[true]);
        assert!((f[0] - 3.0).abs() < 1e-5);
        assert!((f[1] - 5.0).abs() < 1e-5);
        assert!((f[4] - 2.5).abs() < 1e-5);
        assert_eq!(g.next(1, &f, &[true]), Some(0));
        assert_eq!(g.next(3, &f, &[true]), Some(4));
    }

    fn quad(tris: &mut Vec<Tri>, a: V3, b: V3, c: V3, d: V3) {
        tris.push(Tri::new(a, b, c).unwrap());
        tris.push(Tri::new(a, c, d).unwrap());
    }

    /// Two floors: ground (y 0, x 0..20) and a balcony (y 3, x 10..20,
    /// z 0..4) with a flight of 15 stairs (0.2 m x 0.3 m) climbing +x from
    /// x 5.5 to x 10 along z 4..6, and a wall at x 3 with a door gap z 6..8.
    fn two_floors() -> TriMesh {
        let v = V3::new;
        let mut t = Vec::new();
        quad(&mut t, v(-5.0, 0.0, -2.0), v(-5.0, 0.0, 10.0), v(20.0, 0.0, 10.0), v(20.0, 0.0, -2.0));
        quad(&mut t, v(10.0, 3.0, 0.0), v(10.0, 3.0, 6.0), v(20.0, 3.0, 6.0), v(20.0, 3.0, 0.0));
        // The balcony slab edge (facing -x), z 0..4.
        quad(&mut t, v(10.0, 2.7, 0.0), v(10.0, 3.0, 0.0), v(10.0, 3.0, 4.0), v(10.0, 2.7, 4.0));
        for i in 0..15 {
            let x0 = 5.5 + i as f32 * 0.3;
            let y = 0.2 * (i + 1) as f32;
            quad(&mut t, v(x0, y, 4.0), v(x0, y, 6.0), v(x0 + 0.3, y, 6.0), v(x0 + 0.3, y, 4.0));
            quad(&mut t, v(x0, y - 0.2, 4.0), v(x0, y, 4.0), v(x0, y, 6.0), v(x0, y - 0.2, 6.0));
        }
        // Wall with a gap.
        quad(&mut t, v(3.0, 0.0, -2.0), v(3.0, 3.0, -2.0), v(3.0, 3.0, 6.0), v(3.0, 0.0, 6.0));
        quad(&mut t, v(3.0, 0.0, 8.0), v(3.0, 3.0, 8.0), v(3.0, 3.0, 10.0), v(3.0, 0.0, 10.0));
        TriMesh::new(t, 2.0)
    }

    #[test]
    fn walkable_checks_floors_stairs_and_walls() {
        let m = two_floors();
        let v = V3::new;
        // Same floor, open.
        assert!(walkable(&m, v(4.0, 0.0, 1.0), v(9.0, 0.0, 1.0)));
        // Through the wall: blocked; through the gap: fine.
        assert!(!walkable(&m, v(0.0, 0.0, 1.0), v(6.0, 0.0, 1.0)));
        assert!(walkable(&m, v(0.0, 0.0, 7.0), v(6.0, 0.0, 7.0)));
        // Up the stairs and down again.
        assert!(walkable(&m, v(4.5, 0.0, 5.0), v(12.0, 3.0, 5.0)));
        assert!(walkable(&m, v(12.0, 3.0, 5.0), v(4.5, 0.0, 5.0)));
        // Off the balcony edge (a 3 m drop) or straight up to it: no.
        assert!(!walkable(&m, v(12.0, 3.0, 2.0), v(8.0, 0.0, 2.0)));
        assert!(!walkable(&m, v(8.0, 0.0, 2.0), v(12.0, 3.0, 2.0)));
        // Under the balcony floor to a point below it: walkable on the ground floor.
        assert!(walkable(&m, v(11.0, 0.0, 2.0), v(15.0, 0.0, 2.0)));
        assert!(ground_profile(&m, v(9.0, 0.0, 2.0), v(15.0, 0.0, 2.0)).is_some(), "profile");
        assert!(walkable(&m, v(9.0, 0.0, 2.0), v(15.0, 0.0, 2.0)), "9-15");
    }

    #[test]
    fn graphs_link_floors_only_by_stairs() {
        let m = two_floors();
        let v = V3::new;
        let nodes = vec![
            v(9.0, 0.0, 2.0),  // 0 ground floor below the balcony edge
            v(4.5, 0.0, 5.0),  // 1 stair foot
            v(12.0, 3.0, 5.0), // 2 stair top
            v(12.0, 3.0, 2.0), // 3 balcony right above node 0
            v(15.0, 0.0, 2.0), // 4 ground floor under the balcony
            v(4.5, 0.0, 2.0),  // 5 in front of the stairs (their open side is a wall from below)
        ];
        let g = NavGraph::build(nodes, 9.0, |a, b| walkable(&m, a, b).then_some(None));
        let linked = |a: usize, b: usize| g.edges[a].iter().any(|e| e.to as usize == b);
        assert!(linked(1, 2));
        assert!(linked(2, 3));
        assert!(linked(0, 5) && linked(5, 1));
        assert!(!linked(0, 3), "floors must not link across the balcony edge");
        assert!(!linked(4, 3));
        assert!(linked(0, 4));
        // From under the balcony, the route to the balcony goes via the stairs.
        let f = g.field(3, &[]);
        assert_eq!(g.route(4, &f, &[]), vec![4, 0, 5, 1, 2, 3]);
        assert_eq!(g.astar(4, 3, &[]), Some(vec![4, 0, 5, 1, 2, 3]));
        assert!(g.components().iter().all(|c| *c == 0));
    }
}
