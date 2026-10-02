//! Turns Nacht's entity layout into the game's [`Level`] (windows, doors,
//! wall-buys, spawners...) and builds zombie navigation over its path nodes.

use super::build::{to_bevy, NachtScene};
use bevy::prelude::Vec3;
use waw_assets::zombiemap::{SpawnGroup, ZombieMap};
use zm_core::geom::{Aabb, V3};
use zm_core::level::{Door, Level, WallBuy, Window};
use zm_core::navgraph::NavGraph;
use zm_core::trimesh::TriMesh;

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

fn area_of(g: SpawnGroup) -> usize {
    match g {
        SpawnGroup::Init | SpawnGroup::Other => 0,
        SpawnGroup::Door => 1,
        SpawnGroup::Upstairs => 2,
    }
}

/// World-space bounds of brush submodel `n` placed at `origin`.
fn submodel_box(scene: &NachtScene, n: usize, origin: Vec3) -> Option<Aabb> {
    let (lo, hi) = scene.submodel_bounds.get(&n)?;
    Some(Aabb::new(v3(*lo + origin), v3(*hi + origin)))
}

fn union(a: Option<Aabb>, b: Aabb) -> Aabb {
    match a {
        Some(a) => Aabb::new(a.min.min(b.min), a.max.max(b.max)),
        None => b,
    }
}

pub fn build_level(scene: &NachtScene) -> Level {
    let m: &ZombieMap = &scene.map;
    let start = to_bevy(m.player_start);

    // Windows: centred on the struct at the opening, facing the exterior goal.
    let spawners: Vec<(Vec3, usize)> = m.spawners.iter().map(|s| (to_bevy(s.origin), area_of(s.group))).collect();
    let mut windows = Vec::new();
    let mut window_fills = Vec::new();
    for w in &m.windows {
        let at = to_bevy(w.at);
        let out = to_bevy(w.outside);
        let d = Vec3::new(out.x - at.x, 0.0, out.z - at.z).normalize_or_zero();
        let mut fill: Option<Aabb> = None;
        let mut board_models = Vec::new();
        for b in &w.boards {
            let o = to_bevy(b.origin);
            board_models.push((b.submodel, v3(o)));
            if let Some(bx) = submodel_box(scene, b.submodel, o) {
                fill = Some(union(fill, bx));
            }
        }
        // Block the whole opening, floor to above the boards, a little thick.
        let fill = fill.unwrap_or_else(|| Aabb::new(v3(at - Vec3::new(0.6, 0.0, 0.6)), v3(at + Vec3::new(0.6, 2.2, 0.6))));
        let fill = Aabb::new(V3::new(fill.min.x - 0.12, at.y - 0.2, fill.min.z - 0.12), V3::new(fill.max.x + 0.12, fill.max.y + 0.4, fill.max.z + 0.12));
        window_fills.push(fill);
        let nearest = spawners.iter().min_by(|a, b| a.0.distance(out).total_cmp(&b.0.distance(out))).map(|s| s.1).unwrap_or(0);
        windows.push(Window {
            center: V3::new(at.x, (fill.min.y + fill.max.y) * 0.5, at.z),
            outward: (d.x, d.z),
            area: nearest,
            boards: w.boards.len().clamp(1, 255) as u8,
            board_models,
        });
    }

    let mut doors = Vec::new();
    for d in &m.doors {
        let mut blocker: Option<Aabb> = None;
        for (n, origin) in d.blockers.iter().filter_map(|n| scene.entities.iter().find(|e| e.submodel() == Some(*n)).map(|e| (*n, to_bevy(e.origin())))) {
            if let Some(b) = submodel_box(scene, n, origin) {
                blocker = Some(union(blocker, b));
            }
        }
        for (_, o, _) in &d.props {
            let p = to_bevy(*o);
            blocker = Some(union(blocker, Aabb::new(v3(p - Vec3::new(0.9, 0.0, 0.9)), v3(p + Vec3::new(0.9, 1.6, 0.9)))));
        }
        let blocker = blocker.unwrap_or_else(|| {
            let c = d.triggers.first().map(|t| to_bevy(*t)).unwrap_or(start);
            Aabb::new(v3(c - Vec3::new(1.0, 1.0, 1.0)), v3(c + Vec3::new(1.0, 1.5, 1.0)))
        });
        let name = match d.unlocks {
            Some(SpawnGroup::Door) => "Help Room".to_string(),
            Some(SpawnGroup::Upstairs) => "Upstairs".to_string(),
            _ => "Debris".to_string(),
        };
        doors.push(Door { name, blocker, cost: d.cost.max(1), opens: d.unlocks.map(area_of).unwrap_or(0) });
    }

    let wall_buys = m
        .wall_buys
        .iter()
        .filter(|w| zm_core::weapons::find(&zm_core::weapons::default_weapons(), &w.weapon).is_some())
        .map(|w| WallBuy { weapon_id: w.weapon.clone(), cost: w.cost, pos: v3(to_bevy(w.origin)), facing: (0.0, 0.0) })
        .collect();
    let real_box = scene.chest.as_ref().map(|c| Aabb::new(v3(c.bounds.0), v3(c.bounds.1)));
    let crate_box = real_box.or_else(|| m.chest.as_ref().map(|c| {
            let p = to_bevy(c.origin);
            Aabb::new(V3::new(p.x - 0.5, p.y - 0.5, p.z - 0.5), V3::new(p.x + 0.5, p.y + 0.4, p.z + 0.5))
        }))
        .unwrap_or(Aabb::new(V3::new(1000.0, -100.0, 1000.0), V3::new(1001.0, -99.0, 1001.0)));

    let yaw = m.player_yaw.to_radians();
    Level {
        walls: Vec::new(),
        window_fills,
        windows,
        doors,
        wall_buys,
        crate_box,
        areas: Vec::new(),
        player_start: (start.x, start.z),
        player_start_y: start.y,
        // Game yaw 0 looks along +X; the controller's yaw 0 looks along -Z.
        player_yaw: yaw - std::f32::consts::FRAC_PI_2,
        lights: m.lights.iter().map(|l| v3(to_bevy(*l))).collect(),
        spawners: spawners.iter().map(|(p, a)| (v3(*p), *a)).collect(),
    }
}

/// Line of sight for walking between two nav points (knee and chest height).
fn walkable(mesh: &TriMesh, a: V3, b: V3) -> bool {
    [0.5f32, 1.2].iter().all(|h| mesh.line_clear(V3::new(a.x, a.y + h, a.z), V3::new(b.x, b.y + h, b.z)))
}

fn seg_hits(b: &Aabb, a: V3, c: V3) -> bool {
    let d = c.sub(a);
    let len = d.len();
    len > 1e-4 && b.ray_hit(a, d.scale(1.0 / len), len).is_some()
}

/// Navigation graph over the map's path nodes. Edges through a window are
/// never walkable (zombies vault instead); edges through a door are gated.
pub fn build_nav(scene: &NachtScene, level: &Level, mesh: &TriMesh) -> NavGraph {
    let nodes: Vec<V3> = scene.map.path_nodes.iter().map(|p| v3(to_bevy(*p))).collect();
    let mut graph = NavGraph::build(nodes, 7.0, |a, b| {
        if (a.y - b.y).abs() > 2.5 {
            return None;
        }
        let up = V3::new(0.0, 0.8, 0.0);
        if level.window_fills.iter().any(|w| seg_hits(w, a.add(up), b.add(up))) {
            return None;
        }
        let gate = level.doors.iter().position(|d| seg_hits(&d.blocker, a.add(up), b.add(up))).map(|i| i as u16);
        if gate.is_none() && !walkable(mesh, a, b) {
            return None;
        }
        Some(gate)
    });
    // Ensure every node has at least one link (snap isolated ones to their nearest).
    let n = graph.nodes.len();
    let mut extra = Vec::new();
    for i in 0..n {
        if graph.edges[i].is_empty() {
            let me = graph.nodes[i];
            if let Some(j) = (0..n).filter(|&j| j != i).min_by(|&x, &y| graph.nodes[x].sub(me).len().total_cmp(&graph.nodes[y].sub(me).len())) {
                if graph.nodes[j].sub(me).len() < 3.0 {
                    extra.push((i, j));
                }
            }
        }
    }
    for (i, j) in extra {
        graph.connect(i, j, None);
    }
    graph
}
