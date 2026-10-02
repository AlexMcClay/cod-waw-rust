//! Compares a map's render geometry with its collision (clipMap): how much
//! of the opaque static world surface lies on a solid brush or terrain
//! triangle, and how much player-blocking collision is invisible.
//!
//! `cargo run --release -p waw_assets --example clipcover -- [zone]`

use std::collections::{BTreeMap, HashMap};
use waw_assets::t4::clipmap::{self, contents as c};
use waw_assets::t4::{self, decode};
use waw_assets::zone::decompress;
use waw_assets::Install;

type P = [f32; 3];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: P, b: P) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P, b: P) -> P {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn area(t: &[P; 3]) -> f32 {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    dot(n, n).sqrt() * 0.5
}

/// Distance from `p` to triangle `t` (Ericson 5.1.5).
fn tri_dist(p: P, t: &[P; 3]) -> f32 {
    let (a, b, cc) = (t[0], t[1], t[2]);
    let lerp = |a: P, b: P, s: f32| [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s, a[2] + (b[2] - a[2]) * s];
    let ab = sub(b, a);
    let ac = sub(cc, a);
    let ap = sub(p, a);
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    let q = if d1 <= 0.0 && d2 <= 0.0 {
        a
    } else {
        let bp = sub(p, b);
        let (d3, d4) = (dot(ab, bp), dot(ac, bp));
        let cp = sub(p, cc);
        let (d5, d6) = (dot(ab, cp), dot(ac, cp));
        let vc = d1 * d4 - d3 * d2;
        let vb = d5 * d2 - d1 * d6;
        let va = d3 * d6 - d5 * d4;
        if d3 >= 0.0 && d4 <= d3 {
            b
        } else if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            lerp(a, b, d1 / (d1 - d3))
        } else if d6 >= 0.0 && d5 <= d6 {
            cc
        } else if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            lerp(a, cc, d2 / (d2 - d6))
        } else if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
            lerp(b, cc, (d4 - d3) / ((d4 - d3) + (d5 - d6)))
        } else {
            let den = 1.0 / (va + vb + vc);
            let (v, w) = (vb * den, vc * den);
            [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w]
        }
    };
    let d = sub(p, q);
    dot(d, d).sqrt()
}

const CELL: f32 = 64.0;
fn cell(p: P) -> (i32, i32) {
    ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32)
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "nazi_zombie_prototype".into());
    let install = Install::locate(&[]).expect("install");
    let zd = t4::walk(decompress(&std::fs::read(install.fastfile(&name)).expect("read")).expect("decompress"));
    let (Some(cm), Some(w)) = (&zd.clipmap, &zd.world) else {
        println!("no clipmap/world");
        return;
    };
    let world_brushes = &cm.model_brushes()[0];
    // Grid of brushes and terrain triangles (world only).
    let mut grid: HashMap<(i32, i32), (Vec<u16>, Vec<usize>)> = HashMap::new();
    for &bi in world_brushes {
        let b = &cm.brushes[bi as usize];
        let (lo, hi) = (cell(b.mins), cell(b.maxs));
        for i in lo.0..=hi.0 {
            for j in lo.1..=hi.1 {
                grid.entry((i, j)).or_default().0.push(bi);
            }
        }
    }
    let terrain = cm.world_terrain();
    for (ti, (t, _)) in terrain.iter().enumerate() {
        let lo = cell([t[0][0].min(t[1][0]).min(t[2][0]), t[0][1].min(t[1][1]).min(t[2][1]), 0.0]);
        let hi = cell([t[0][0].max(t[1][0]).max(t[2][0]), t[0][1].max(t[1][1]).max(t[2][1]), 0.0]);
        for i in lo.0..=hi.0 {
            for j in lo.1..=hi.1 {
                grid.entry((i, j)).or_default().1.push(ti);
            }
        }
    }
    let mut tc: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, m) in &terrain {
        *tc.entry(cm.material_contents(*m as i64)).or_default() += 1;
    }
    println!("world brushes {}, terrain tris {} by contents {tc:x?}", world_brushes.len(), terrain.len());

    let covered = |p: P, mask: u32| -> bool {
        let Some((bs, ts)) = grid.get(&cell(p)) else { return false };
        bs.iter().any(|&bi| {
            let b = &cm.brushes[bi as usize];
            b.contents & mask != 0 && clipmap::brush_contains(b, p, 1.0)
        }) || ts.iter().any(|&ti| cm.material_contents(terrain[ti].1 as i64) & mask != 0 && tri_dist(p, &terrain[ti].0) < 1.0)
    };

    // Render collision as the game builds it: static, opaque, non-decal.
    let skip = |techset: &str, name: &str| {
        let t = techset;
        t.contains("sky") || t.contains("tools") || t.contains("shadowcaster") || t.contains("water") || t.contains("distortion") || t.contains("add") || name.contains("caulk") || name.contains("clip") || t.contains("unlit") || t.split("sm_").nth(1).is_some_and(|s| s.starts_with('b'))
    };
    let (mut total, mut cov, mut cov_any) = (0.0f64, 0.0f64, 0.0f64);
    let mut missing: BTreeMap<String, f64> = BTreeMap::new();
    let mut missing_at: BTreeMap<String, P> = BTreeMap::new();
    for i in 0..w.static_surface_count.min(w.surfaces.len() as u32) as usize {
        let s = &w.surfaces[i];
        if w.decal_range.contains(&(i as u32)) {
            continue;
        }
        let Some(m) = s.material.map(|m| &zd.materials[m as usize]) else { continue };
        let techset = m.techset.clone().unwrap_or_default();
        if skip(techset.trim_start_matches(','), &m.name) {
            continue;
        }
        for t in decode::world_triangles(&zd, w, s) {
            let p: Vec<P> = t.iter().filter_map(|&v| decode::world_vertex(&zd, w, v).map(|x| x.pos)).collect();
            if p.len() != 3 {
                continue;
            }
            let tri = [p[0], p[1], p[2]];
            let a = area(&tri) as f64;
            total += a;
            let centre = [(p[0][0] + p[1][0] + p[2][0]) / 3.0, (p[0][1] + p[1][1] + p[2][1]) / 3.0, (p[0][2] + p[1][2] + p[2][2]) / 3.0];
            if covered(centre, c::SOLID) {
                cov += a;
            }
            if covered(centre, !(c::DETAIL | c::STRUCTURAL)) {
                cov_any += a;
            } else {
                *missing.entry(m.name.clone()).or_default() += a;
                missing_at.entry(m.name.clone()).or_insert(centre);
            }
        }
    }
    println!(
        "render collision area {:.0} sq ft: {:.2}% on solid brushes/terrain, {:.2}% on any collision",
        total / 144.0,
        100.0 * cov / total,
        100.0 * cov_any / total
    );
    let mut miss: Vec<_> = missing.into_iter().collect();
    miss.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!("not on any collision (sq ft, first spot):");
    for (n, a) in miss.iter().take(25) {
        println!("  {n:50} {:9.1}  {:?}", a / 144.0, missing_at[n]);
    }

    // Player-blocking faces that are not near render geometry: invisible clip.
    let mut by_kind: BTreeMap<&str, (usize, f64)> = BTreeMap::new();
    for &bi in world_brushes {
        let b = &cm.brushes[bi as usize];
        let kind = if b.contents & c::SOLID != 0 {
            "solid"
        } else if b.contents & c::PLAYERCLIP != 0 {
            "playerclip"
        } else if b.contents & c::MONSTERCLIP != 0 {
            "monsterclip only"
        } else {
            "other"
        };
        let e = by_kind.entry(kind).or_default();
        e.0 += 1;
        for f in clipmap::brush_polygons(b) {
            for k in 1..f.points.len() - 1 {
                e.1 += area(&[f.points[0], f.points[k], f.points[k + 1]]) as f64 / 144.0;
            }
        }
    }
    println!("world brushes by kind (count, face area sq ft): {by_kind:?}");
}
