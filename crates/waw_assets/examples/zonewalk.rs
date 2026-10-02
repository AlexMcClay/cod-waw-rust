//! Walks a fastfile with the full T4 loader and prints a summary.
//!
//! `cargo run --release -p waw_assets --example zonewalk -- [zone_name]`

use std::collections::BTreeMap;
use waw_assets::t4;
use waw_assets::zone::decompress;
use waw_assets::Install;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "nazi_zombie_prototype".into());
    let install = Install::locate(&[]).expect("install");
    let t = std::time::Instant::now();
    let data = decompress(&std::fs::read(install.fastfile(&name)).expect("read")).expect("decompress");
    let zd = t4::walk(data);
    println!("{name}: walked in {:?}", t.elapsed());
    println!(
        "end {} / {}  virtual {:#x} / {:#x}  unresolved {}  complete {}  stopped {:?}",
        zd.end_pos,
        zd.data.len(),
        zd.virtual_end,
        zd.virtual_expected,
        zd.unresolved,
        zd.complete(),
        zd.stopped
    );
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    for (t, _) in &zd.assets {
        *hist.entry(format!("{t:?}")).or_default() += 1;
    }
    println!("assets {}: {hist:?}", zd.assets.len());
    println!(
        "images {} materials {} xmodels {} sounds {} loaded {} weapons {} localize {}",
        zd.images.len(),
        zd.materials.len(),
        zd.xmodels.len(),
        zd.sounds.len(),
        zd.loaded_sounds.len(),
        zd.weapons.len(),
        zd.localize.len()
    );
    if let Some(w) = &zd.world {
        let tris: usize = w.surfaces.iter().map(|s| s.tri_count as usize).sum();
        let mats: std::collections::HashSet<_> = w.surfaces.iter().filter_map(|s| s.material).collect();
        println!(
            "world {}: {} verts, {} indices, {} surfaces ({} tris, {} materials), {} models, {} smodels, bounds {:?}..{:?}",
            w.name,
            w.vertex_count,
            w.index_count,
            w.surfaces.len(),
            tris,
            mats.len(),
            w.models.len(),
            w.smodels.len(),
            w.mins,
            w.maxs
        );
        if let Some(s) = w.surfaces.first() {
            let m = s.material.map(|m| &zd.materials[m as usize]);
            println!("  surface 0 material {:?} colorMap {:?}", m.map(|m| &m.name), m.and_then(|m| m.color_map()).map(|i| zd.image_name(i)));
        }
    }
    for n in ["char_ger_honorgd_zomb_behead", "viewmodel_usa_marine_arms", "viewmodel_usa_colt45_pistol"] {
        if let Some(m) = zd.xmodel(n) {
            let r = m.lod_surfs(0);
            let (v, t): (usize, usize) = m.surfs[r.clone()].iter().fold((0, 0), |a, s| (a.0 + s.vert_count as usize, a.1 + s.tri_count as usize));
            println!("xmodel {n}: {} bones, {} surfs, lod0 {} verts {} tris", m.bones.len(), m.surfs.len(), v, t);
        }
    }
    for w in zd.weapons.iter().take(5) {
        println!("weapon {} ({}): fire {:?} view {:?}", w.name, w.display_name, w.sound("fireSound"), w.view_model.map(|i| &zd.xmodels[i as usize].name));
    }
}
