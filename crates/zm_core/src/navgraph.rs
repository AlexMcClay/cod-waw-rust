//! Waypoint-graph navigation for maps that come with path nodes (the real
//! Nacht der Untoten ships 634 of them in its entity list).
//!
//! Like [`crate::nav`], zombies share one Dijkstra field toward the player
//! that is refreshed a few times per second. Edges can be gated by a door so
//! they only count once that door has been bought.

use crate::geom::V3;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

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
        let cost = self.nodes[a].sub(self.nodes[b]).len();
        self.edges[a].push(Edge { to: b as u32, cost, gate });
        self.edges[b].push(Edge { to: a as u32, cost, gate });
    }

    /// Closest node accepted by `visible` (e.g. line of sight), preferring
    /// nodes at a similar height.
    pub fn nearest(&self, p: V3, visible: impl Fn(V3) -> bool) -> Option<usize> {
        let mut order: Vec<(f32, usize)> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let d = n.sub(p);
                (d.x * d.x + d.z * d.z + (d.y * 3.0).powi(2), i)
            })
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        order.iter().take(12).find(|(_, i)| visible(self.nodes[*i])).or(order.first()).map(|(_, i)| *i)
    }

    /// Dijkstra distances from `target` over edges whose gate is open.
    pub fn field(&self, target: usize, open: &[bool]) -> Vec<f32> {
        let mut dist = vec![f32::INFINITY; self.nodes.len()];
        if target >= dist.len() {
            return dist;
        }
        dist[target] = 0.0;
        let mut heap = BinaryHeap::new();
        heap.push(Item { cost: 0.0, node: target as u32 });
        while let Some(Item { cost, node }) = heap.pop() {
            if cost > dist[node as usize] {
                continue;
            }
            for e in &self.edges[node as usize] {
                if e.gate.is_some_and(|g| !open.get(g as usize).copied().unwrap_or(false)) {
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
    /// that is closest to the target), or `None` when already there/unreachable.
    pub fn next(&self, node: usize, field: &[f32], open: &[bool]) -> Option<usize> {
        let here = *field.get(node)?;
        if !here.is_finite() || here == 0.0 {
            return None;
        }
        self.edges[node]
            .iter()
            .filter(|e| e.gate.is_none_or(|g| open.get(g as usize).copied().unwrap_or(false)))
            .map(|e| (field[e.to as usize] + e.cost, e.to as usize))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, n)| n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn nearest_respects_visibility() {
        let g = corridor();
        assert_eq!(g.nearest(V3::new(4.2, 0.0, 0.0), |_| true), Some(2));
        assert_eq!(g.nearest(V3::new(4.2, 0.0, 0.0), |n| n.x > 5.0), Some(3));
    }

    #[test]
    fn walking_reaches_the_target() {
        let g = corridor();
        let open = [true];
        let f = g.field(5, &open);
        let mut n = 0;
        for _ in 0..10 {
            match g.next(n, &f, &open) {
                Some(m) => n = m,
                None => break,
            }
        }
        assert_eq!(n, 5);
    }
}
