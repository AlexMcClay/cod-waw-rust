//! Grid navigation for zombies inside the bunker.
//!
//! Instead of running A* per zombie, the game builds one *flow field* (a
//! Dijkstra distance map from the player) a few times per second; every
//! zombie then just steps toward the neighbouring cell with the lowest
//! distance, or walks straight at the player when it has line of sight.

use crate::level::Level;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub const CELL: f32 = 0.5;

#[derive(Debug, Clone)]
pub struct NavGrid {
    pub x0: f32,
    pub z0: f32,
    pub w: usize,
    pub h: usize,
    pub blocked: Vec<bool>,
}

#[derive(Copy, Clone, PartialEq)]
struct Node {
    cost: f32,
    idx: usize,
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

const NEIGH: [(i32, i32, f32); 8] = [
    (1, 0, 1.0),
    (-1, 0, 1.0),
    (0, 1, 1.0),
    (0, -1, 1.0),
    (1, 1, std::f32::consts::SQRT_2),
    (1, -1, std::f32::consts::SQRT_2),
    (-1, 1, std::f32::consts::SQRT_2),
    (-1, -1, std::f32::consts::SQRT_2),
];

impl NavGrid {
    /// Build the grid. `door_open[i]` says whether `level.doors[i]` is cleared.
    pub fn build(level: &Level, radius: f32, door_open: &[bool]) -> NavGrid {
        let b = level.interior_bounds();
        let x0 = b.min.x - CELL;
        let z0 = b.min.z - CELL;
        let w = ((b.max.x - x0) / CELL).ceil() as usize + 2;
        let h = ((b.max.z - z0) / CELL).ceil() as usize + 2;
        let mut blocked = vec![true; w * h];
        let mut solids: Vec<_> = level.walls.iter().filter(|a| a.min.y < 1.0).map(|a| a.inflate_xz(radius)).collect();
        solids.extend(level.window_fills.iter().map(|a| a.inflate_xz(radius)));
        if level.crate_solid {
            solids.push(level.crate_box.inflate_xz(radius));
        }
        for (i, d) in level.doors.iter().enumerate() {
            if !door_open.get(i).copied().unwrap_or(false) {
                solids.push(d.blocker.inflate_xz(radius));
            }
        }
        for j in 0..h {
            for i in 0..w {
                let (x, z) = (x0 + (i as f32 + 0.5) * CELL, z0 + (j as f32 + 0.5) * CELL);
                let inside = level.area_at(x, z).is_some()
                    || level.doors.iter().any(|d| d.blocker.inflate_xz(0.05).contains_xz(x, z));
                let solid = solids.iter().any(|s| s.contains_xz(x, z));
                blocked[j * w + i] = !inside || solid;
            }
        }
        NavGrid { x0, z0, w, h, blocked }
    }

    pub fn cell_of(&self, x: f32, z: f32) -> Option<(usize, usize)> {
        let i = ((x - self.x0) / CELL).floor();
        let j = ((z - self.z0) / CELL).floor();
        if i < 0.0 || j < 0.0 || i as usize >= self.w || j as usize >= self.h {
            None
        } else {
            Some((i as usize, j as usize))
        }
    }

    pub fn center(&self, i: usize, j: usize) -> (f32, f32) {
        (self.x0 + (i as f32 + 0.5) * CELL, self.z0 + (j as f32 + 0.5) * CELL)
    }

    pub fn is_open(&self, i: usize, j: usize) -> bool {
        i < self.w && j < self.h && !self.blocked[j * self.w + i]
    }

    pub fn open_at(&self, x: f32, z: f32) -> bool {
        self.cell_of(x, z).is_some_and(|(i, j)| self.is_open(i, j))
    }

    /// Nearest open cell to a point (spiral search), used when a target
    /// stands in an inflated wall margin.
    pub fn nearest_open(&self, x: f32, z: f32) -> Option<(usize, usize)> {
        let (ci, cj) = self.cell_of(x, z)?;
        if self.is_open(ci, cj) {
            return Some((ci, cj));
        }
        for r in 1..8i32 {
            let mut best: Option<((usize, usize), f32)> = None;
            for dj in -r..=r {
                for di in -r..=r {
                    if di.abs() != r && dj.abs() != r {
                        continue;
                    }
                    let (i, j) = (ci as i32 + di, cj as i32 + dj);
                    if i < 0 || j < 0 {
                        continue;
                    }
                    let (i, j) = (i as usize, j as usize);
                    if self.is_open(i, j) {
                        let (cx, cz) = self.center(i, j);
                        let d = (cx - x).powi(2) + (cz - z).powi(2);
                        if best.is_none_or(|(_, bd)| d < bd) {
                            best = Some(((i, j), d));
                        }
                    }
                }
            }
            if let Some((c, _)) = best {
                return Some(c);
            }
        }
        None
    }

    /// Walkable straight line between two points (sampled every quarter cell).
    pub fn line_clear(&self, a: (f32, f32), b: (f32, f32)) -> bool {
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dz * dz).sqrt();
        let steps = (len / (CELL * 0.25)).ceil().max(1.0) as usize;
        (0..=steps).all(|s| {
            let t = s as f32 / steps as f32;
            self.open_at(a.0 + dx * t, a.1 + dz * t)
        })
    }

    /// Dijkstra distance field to `target`. Unreachable cells are `f32::INFINITY`.
    pub fn flow_field(&self, target: (f32, f32)) -> Vec<f32> {
        let mut dist = vec![f32::INFINITY; self.w * self.h];
        let Some((ti, tj)) = self.nearest_open(target.0, target.1) else {
            return dist;
        };
        let start = tj * self.w + ti;
        dist[start] = 0.0;
        let mut heap = BinaryHeap::new();
        heap.push(Node { cost: 0.0, idx: start });
        while let Some(Node { cost, idx }) = heap.pop() {
            if cost > dist[idx] {
                continue;
            }
            let (i, j) = ((idx % self.w) as i32, (idx / self.w) as i32);
            for (di, dj, c) in NEIGH {
                let (ni, nj) = (i + di, j + dj);
                if ni < 0 || nj < 0 || !self.is_open(ni as usize, nj as usize) {
                    continue;
                }
                // No corner cutting on diagonals.
                if di != 0 && dj != 0
                    && (!self.is_open((i + di) as usize, j as usize) || !self.is_open(i as usize, (j + dj) as usize))
                {
                    continue;
                }
                let n = nj as usize * self.w + ni as usize;
                let nc = cost + c;
                if nc < dist[n] {
                    dist[n] = nc;
                    heap.push(Node { cost: nc, idx: n });
                }
            }
        }
        dist
    }

    /// Next waypoint (world XZ) to move toward when standing at `pos`,
    /// following `field`. Returns `None` if unreachable.
    pub fn next_waypoint(&self, field: &[f32], pos: (f32, f32)) -> Option<(f32, f32)> {
        let (i, j) = self.nearest_open(pos.0, pos.1)?;
        let here = field[j * self.w + i];
        if !here.is_finite() {
            return None;
        }
        // If we're off-grid (pushed into a margin) head to our cell centre first.
        if !self.open_at(pos.0, pos.1) {
            return Some(self.center(i, j));
        }
        let mut best = (here, (i, j));
        for (di, dj, _) in NEIGH {
            let (ni, nj) = (i as i32 + di, j as i32 + dj);
            if ni < 0 || nj < 0 {
                continue;
            }
            let (ni, nj) = (ni as usize, nj as usize);
            if !self.is_open(ni, nj) {
                continue;
            }
            if di != 0 && dj != 0 && (!self.is_open(ni, j) || !self.is_open(i, nj)) {
                continue;
            }
            let d = field[nj * self.w + ni];
            if d < best.0 {
                best = (d, (ni, nj));
            }
        }
        Some(self.center(best.1 .0, best.1 .1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doors_gate_reachability() {
        let level = Level::bunker();
        let closed = NavGrid::build(&level, 0.3, &[false, false]);
        let field = closed.flow_field(level.player_start);
        let east = level.windows.iter().find(|w| w.area == 1).unwrap().inside_point();
        let (i, j) = closed.nearest_open(east.0, east.1).unwrap();
        assert!(!field[j * closed.w + i].is_finite(), "east wing should be cut off");

        let open = NavGrid::build(&level, 0.3, &[true, true]);
        let field = open.flow_field(level.player_start);
        for w in &level.windows {
            let p = w.inside_point();
            let (i, j) = open.nearest_open(p.0, p.1).unwrap();
            assert!(field[j * open.w + i].is_finite(), "window {w:?} unreachable");
        }
    }

    #[test]
    fn walking_the_field_reaches_target() {
        let level = Level::bunker();
        let g = NavGrid::build(&level, 0.3, &[true, true]);
        let target = (15.0, 0.0);
        let field = g.flow_field(target);
        // Start in the north wing, which needs two turns to reach the east wing.
        let mut pos = (-8.0, 12.0);
        for _ in 0..400 {
            if (pos.0 - target.0).hypot(pos.1 - target.1) < 0.6 {
                return;
            }
            let wp = g.next_waypoint(&field, pos).expect("reachable");
            let (dx, dz) = (wp.0 - pos.0, wp.1 - pos.1);
            let d = dx.hypot(dz).max(1e-6);
            let step = d.min(0.25);
            pos = (pos.0 + dx / d * step, pos.1 + dz / d * step);
        }
        panic!("never reached target, ended at {pos:?}");
    }

    #[test]
    fn line_of_sight() {
        let level = Level::bunker();
        let g = NavGrid::build(&level, 0.3, &[false, false]);
        assert!(g.line_clear((-5.0, 0.0), (5.0, 0.0)));
        assert!(!g.line_clear((0.0, 0.0), (15.0, 0.0)));
    }
}
