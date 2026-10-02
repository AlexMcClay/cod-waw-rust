//! Turns Nacht's entity layout into the game's [`Level`] (windows, doors,
//! wall-buys, spawners...) and builds zombie navigation over its path nodes.

use super::build::{to_bevy, NachtScene};
use bevy::prelude::Vec3;
use waw_assets::zombiemap::{SpawnGroup, ZombieMap};
use zm_core::geom::{Aabb, V3};
use zm_core::level::{Door, Level, WallBuy, Window};
use zm_core::navgraph::{self, NavGraph};
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
        // The box model collides through the map's collision mesh.
        crate_solid: real_box.is_none(),
        areas: Vec::new(),
        player_start: (start.x, start.z),
        player_start_y: start.y,
        // Game yaw 0 looks along +X; the controller's yaw 0 looks along -Z.
        player_yaw: yaw - std::f32::consts::FRAC_PI_2,
        lights: m.lights.iter().map(|l| v3(to_bevy(*l))).collect(),
        spawners: spawners.iter().map(|(p, a)| (v3(*p), *a)).collect(),
        crate_weapons: Some(zm_core::weapons::NACHT_CRATE),
    }
}

fn seg_hits(b: &Aabb, a: V3, c: V3) -> bool {
    let d = c.sub(a);
    let len = d.len();
    len > 1e-4 && b.ray_hit(a, d.scale(1.0 / len), len).is_some()
}

/// Longest link between two path nodes. The map's nodes are a few metres
/// apart; longer links only add redundant edges.
const MAX_LINK: f32 = 7.0;

/// Navigation graph over the map's path nodes, linked in 3D: two nodes are
/// joined only when a zombie can walk straight between them (continuous
/// ground without steps or drops above [`navgraph::STEP`], nothing in the
/// way; see [`navgraph::walkable`]), so floors only connect through stairs
/// and ramps. Edges through a window are never walkable (zombies vault
/// instead); edges through a door or debris are gated by it.
pub fn build_nav(scene: &NachtScene, level: &Level, mesh: &TriMesh) -> NavGraph {
    let t0 = std::time::Instant::now();
    // Nodes sit above the floor in the map (outdoors up to a metre or
    // more); put them on it.
    let nodes: Vec<V3> = scene
        .map
        .path_nodes
        .iter()
        .map(|p| {
            let n = v3(to_bevy(*p));
            let g = navgraph::ground(mesh, n.x, n.z, n.y + 0.6, n.y - 2.5);
            V3::new(n.x, g.unwrap_or(n.y), n.z)
        })
        .collect();
    let up = V3::new(0.0, 0.8, 0.0);
    // A link is behind a door when a zombie's body (feet to head, its
    // width around the line) would touch the door's blocker anywhere on
    // the way: debris on a staircase sits above the lower node's height.
    let gate_of = |a: V3, b: V3| {
        level
            .doors
            .iter()
            .position(|d| {
                let bx = d.blocker.inflate_xz(navgraph::BODY + 0.05);
                [0.3f32, 0.9, 1.5].iter().any(|h| seg_hits(&bx, a.add(V3::new(0.0, *h, 0.0)), b.add(V3::new(0.0, *h, 0.0))))
            })
            .map(|i| i as u16)
    };
    let through_window = |a: V3, b: V3| level.window_fills.iter().any(|w| seg_hits(w, a.add(up), b.add(up)));
    let mut graph = NavGraph::build(nodes, MAX_LINK, |a, b| {
        if (a.y - b.y).abs() > MAX_LINK * 0.75 || through_window(a, b) || !navgraph::walkable(mesh, a, b) {
            return None;
        }
        Some(gate_of(a, b))
    });
    // A node the strict test left alone (e.g. tucked against a prop) joins
    // its nearest neighbour on the same floor that it can see.
    let n = graph.nodes.len();
    let mut extra = Vec::new();
    for i in 0..n {
        if !graph.edges[i].is_empty() {
            continue;
        }
        let me = graph.nodes[i];
        let near = graph
            .by_distance(me)
            .into_iter()
            .skip(1)
            .take(8)
            .find(|(d, j)| {
                let o = graph.nodes[*j];
                *d < 3.0 && (o.y - me.y).abs() <= navgraph::STEP && !through_window(me, o) && navgraph::clear(mesh, me.add(up), o.add(up))
            });
        if let Some((_, j)) = near {
            extra.push((i, j));
        }
    }
    let patched = extra.len();
    for (i, j) in extra {
        let gate = gate_of(graph.nodes[i], graph.nodes[j]);
        graph.connect(i, j, gate);
    }
    let comps = graph.components();
    let mut sizes = std::collections::HashMap::<u32, usize>::new();
    for c in &comps {
        *sizes.entry(*c).or_default() += 1;
    }
    let mut sizes: Vec<usize> = sizes.into_values().collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    bevy::log::info!(
        "nav: {} nodes, {} links ({} patched) in {:.2}s; components {:?}",
        graph.nodes.len(),
        graph.link_count(),
        patched,
        t0.elapsed().as_secs_f32(),
        &sizes[..sizes.len().min(8)]
    );
    graph
}

/// The real map for `--ignored` tests (needs a World at War install).
#[cfg(test)]
pub mod testmap {
    use super::*;
    use std::sync::OnceLock;

    pub struct TestMap {
        pub scene: NachtScene,
        pub level: Level,
        pub nav: NavGraph,
    }

    pub fn get() -> &'static TestMap {
        static MAP: OnceLock<TestMap> = OnceLock::new();
        MAP.get_or_init(|| {
            let explicit: Vec<std::path::PathBuf> = std::env::var_os("UNDEAD_WAW").map(Into::into).into_iter().collect();
            let install = waw_assets::install::Install::locate(&explicit).expect("World at War install");
            let iwd = waw_assets::iwd::Iwd::open(&install.main_dir()).expect("iwd");
            let scene = super::super::build::build(&install, &iwd, false, super::super::build::Wanted { aliases: Vec::new(), weapon_ids: Vec::new() })
                .expect("Nacht");
            let level = build_level(&scene);
            let nav = build_nav(&scene, &level, &scene.collision);
            TestMap { scene, level, nav }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs a World at War install"]
    fn nacht_nav_graph() {
        let m = testmap::get();
        let g = &m.nav;
        let comps = g.components();
        let mut sizes = std::collections::HashMap::<u32, usize>::new();
        for c in &comps {
            *sizes.entry(*c).or_default() += 1;
        }
        let main = *sizes.iter().max_by_key(|(_, n)| **n).unwrap().0;
        println!("nodes {} links {} components {:?}", g.nodes.len(), g.link_count(), sizes.values().collect::<Vec<_>>());
        for (i, n) in g.nodes.iter().enumerate() {
            if comps[i] != main {
                println!("  off main: node {i} at ({:.2} {:.2} {:.2}) links {}", n.x, n.y, n.z, g.edges[i].len());
            }
        }
        let mut hist = std::collections::BTreeMap::<i32, usize>::new();
        for n in &g.nodes {
            *hist.entry((n.y * 2.0).floor() as i32).or_default() += 1;
        }
        println!("node heights (0.5 m bins): {hist:?}");
        for (b, e) in &m.scene.map.traversals {
            println!("traversal {:?} -> {:?}", to_bevy(*b), to_bevy(*e));
        }
        for (i, d) in m.level.doors.iter().enumerate() {
            let gated = g.edges.iter().flatten().filter(|e| e.gate == Some(i as u16)).count() / 2;
            println!("door {i} {} blocker {:?}..{:?} gated links {gated}", d.name, d.blocker.min, d.blocker.max);
        }
        for (i, w) in m.level.windows.iter().enumerate() {
            let f = m.level.window_fills[i];
            println!("window {i} center {:?} area {} fill {:?}..{:?}", w.center, w.area, f.min, f.max);
        }
        for e in m.scene.entities.iter().filter(|e| e.classname().starts_with("trigger_multiple") || e.targetname() == "playable_area") {
            let b = e.submodel().and_then(|n| m.scene.submodel_bounds.get(&n));
            println!("{} {} model {:?} origin {:?} bounds {:?}", e.classname(), e.targetname(), e.submodel(), to_bevy(e.origin()), b);
        }
        // `NAV_DUMP=<dir>`: nodes, links and collision for plotting.
        if let Some(dir) = std::env::var_os("NAV_DUMP").map(std::path::PathBuf::from) {
            let mut s = String::new();
            for (i, n) in g.nodes.iter().enumerate() {
                s += &format!("n {i} {} {} {} {}\n", n.x, n.y, n.z, comps[i]);
            }
            for (i, es) in g.edges.iter().enumerate() {
                for e in es.iter().filter(|e| e.to as usize > i) {
                    s += &format!("e {i} {} {}\n", e.to, e.gate.map(|g| g as i32).unwrap_or(-1));
                }
            }
            std::fs::write(dir.join("nav.txt"), s).unwrap();
            let mut b = Vec::new();
            for t in &m.scene.collision.tris {
                for v in [t.a, t.b, t.c, t.n] {
                    for f in [v.x, v.y, v.z] {
                        b.extend_from_slice(&f.to_le_bytes());
                    }
                }
            }
            std::fs::write(dir.join("tris.bin"), b).unwrap();
        }
        // Inside (start room, help room, upstairs) is one piece, separate
        // from the yard and the hill outside (they only meet at windows).
        let (sx, sz) = m.level.player_start;
        let inside = comps[g.by_distance(V3::new(sx, m.level.player_start_y, sz))[0].1];
        println!("inside: {} nodes", sizes[&inside]);
        assert!(sizes[&inside] >= 200, "inside is split");
        let outside = comps[g.by_distance(v3(to_bevy(m.scene.map.spawners[0].origin)))[0].1];
        assert_ne!(inside, outside, "inside leaks to the outside");
        // Upstairs (by the stairs' top) and the help room are inside.
        assert!(g.nodes.iter().enumerate().any(|(i, n)| comps[i] == inside && n.y > 3.0));
        // With every door and debris pile closed, the start room reaches
        // neither upstairs nor the help room (behind door 1, x > 8).
        let start = g.by_distance(V3::new(sx, m.level.player_start_y, sz))[0].1;
        let closed = g.field(start, &vec![false; m.level.doors.len()]);
        let leaks: Vec<usize> = (0..g.nodes.len()).filter(|&i| closed[i].is_finite() && (g.nodes[i].y > 3.3 || g.nodes[i].x > 8.0)).collect();
        println!("reachable with doors closed: {}", closed.iter().filter(|c| c.is_finite()).count());
        assert!(leaks.is_empty(), "past closed doors: {:?}", leaks.iter().map(|&i| (i, g.nodes[i])).collect::<Vec<_>>());
        let open = g.field(start, &vec![true; m.level.doors.len()]);
        assert!((0..g.nodes.len()).all(|i| comps[i] != inside || open[i].is_finite()));
    }
}
