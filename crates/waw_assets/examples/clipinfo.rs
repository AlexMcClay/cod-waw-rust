//! Summarises a map's collision (clipMap): brushes by contents and
//! material, brush models, terrain triangles. With a box argument, lists
//! the brushes inside it.
//!
//! `cargo run --release -p waw_assets --example clipinfo -- [zone] [x0 y0 z0 x1 y1 z1]`

use std::collections::BTreeMap;
use waw_assets::t4::{self, clipmap};
use waw_assets::zone::decompress;
use waw_assets::Install;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let name = args.first().cloned().unwrap_or_else(|| "nazi_zombie_prototype".into());
    let install = Install::locate(&[]).expect("install");
    let zd = t4::walk(decompress(&std::fs::read(install.fastfile(&name)).expect("read")).expect("decompress"));
    println!("complete {} unresolved {}", zd.complete(), zd.unresolved);
    let Some(cm) = &zd.clipmap else {
        println!("no clipmap");
        return;
    };
    println!(
        "clipmap {}: {} materials, {} brushes, {} leafs, {} leaf brush nodes, {} models, {} verts, {} tris, {} partitions, {} aabb trees",
        cm.name,
        cm.materials.len(),
        cm.brushes.len(),
        cm.leafs.len(),
        cm.leaf_brush_nodes.len(),
        cm.models.len(),
        cm.verts.len(),
        cm.tris.len(),
        cm.partitions.len(),
        cm.aabb_trees.len()
    );
    for (i, m) in cm.materials.iter().enumerate() {
        println!("  material {i:3} {:40} surf {:#010x} contents {:#010x}", m.name, m.surface_flags, m.contents);
    }
    let mut by_contents: BTreeMap<u32, usize> = BTreeMap::new();
    for b in &cm.brushes {
        *by_contents.entry(b.contents).or_default() += 1;
    }
    println!("brushes by contents:");
    for (c, n) in &by_contents {
        println!("  {c:#010x}: {n}");
    }
    let per_model = cm.model_brushes();
    for (m, list) in per_model.iter().enumerate() {
        if m < 4 || !list.is_empty() {
            let model = cm.models.get(m);
            println!(
                "  model {m}: {} brushes  bounds {:?}..{:?} leaf node {:?} aabbs {:?}",
                list.len(),
                model.map(|m| m.mins),
                model.map(|m| m.maxs),
                model.map(|m| m.leaf.leaf_brush_node),
                model.map(|m| (m.leaf.first_coll_aabb, m.leaf.coll_aabb_count))
            );
        }
        if m > 80 {
            break;
        }
    }
    let mut sides_hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut faces = 0;
    for b in &cm.brushes {
        *sides_hist.entry(b.sides.len()).or_default() += 1;
        faces += clipmap::brush_polygons(b).len();
    }
    println!("extra sides per brush: {sides_hist:?}; {faces} faces");
    let terrain = cm.world_terrain();
    println!("world terrain tris {} (of {})", terrain.len(), cm.tris.len());

    if args.get(1).map(String::as_str) == Some("brush") {
        for i in args[2..].iter().filter_map(|s| s.parse::<usize>().ok()) {
            let b = &cm.brushes[i];
            println!("brush {i}: contents {:#010x} {:?}..{:?} axial materials {:?}", b.contents, b.mins, b.maxs, b.axial_material);
            for s in &b.sides {
                println!("  side {:?} material {}", s.plane, cm.materials.get(s.material as usize).map(|m| m.name.as_str()).unwrap_or("?"));
            }
            for f in clipmap::brush_polygons(b) {
                println!("  face n {:?} mat {} {:?}", f.normal, cm.materials.get(f.material as usize).map(|m| m.name.as_str()).unwrap_or("?"), f.points);
            }
        }
        return;
    }
    if args.len() >= 7 {
        let v: Vec<f32> = args[1..7].iter().map(|s| s.parse().unwrap()).collect();
        let (lo, hi) = ([v[0], v[1], v[2]], [v[3], v[4], v[5]]);
        let world: std::collections::HashSet<u16> = per_model[0].iter().copied().collect();
        for e in waw_assets::mapents::parse(zd.map_ents.as_deref().unwrap_or("")) {
            let o = e.origin();
            if (0..3).all(|k| o[k] >= lo[k] - 150.0 && o[k] <= hi[k] + 150.0) {
                let m = e.submodel();
                let b = m.and_then(|m| per_model.get(m)).map(|l| l.len());
                println!("  entity {} '{}' model {:?} ({:?} brushes) at {:?} angles {:?}", e.classname(), e.targetname(), e.get("model"), b, o, e.angles());
            }
        }
        for (i, b) in cm.brushes.iter().enumerate() {
            if (0..3).all(|k| b.maxs[k] >= lo[k] && b.mins[k] <= hi[k]) {
                let mats: Vec<String> = b
                    .sides
                    .iter()
                    .map(|s| s.material as i64)
                    .chain(b.axial_material.iter().flatten().map(|&m| m as i64))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .map(|m| cm.materials.get(m as usize).map(|x| x.name.clone()).unwrap_or(format!("#{m}")))
                    .collect();
                let slopes: Vec<String> = b
                    .sides
                    .iter()
                    .filter(|s| s.plane.normal[2].abs() > 0.05 && s.plane.normal[2].abs() < 0.99)
                    .map(|s| format!("({:.2},{:.2},{:.2})", s.plane.normal[0], s.plane.normal[1], s.plane.normal[2]))
                    .collect();
                println!(
                    "  brush {i:5} {} contents {:#010x} {:?}..{:?} sides {} slopes {slopes:?} {mats:?}",
                    if world.contains(&(i as u16)) { "world" } else { "model" },
                    b.contents,
                    b.mins,
                    b.maxs,
                    b.sides.len()
                );
            }
        }
        for (t, m) in &terrain {
            if t.iter().any(|p| (0..3).all(|k| p[k] >= lo[k] && p[k] <= hi[k])) {
                println!("  terrain tri {:?} material {}", t, cm.materials.get(*m as usize).map(|x| x.name.as_str()).unwrap_or("?"));
            }
        }
    }
}
