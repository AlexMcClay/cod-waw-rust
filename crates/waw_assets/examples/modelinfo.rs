//! Prints bounds and bones of models.
use waw_assets::{t4, zone::decompress, Install};
fn main() {
    let names: Vec<String> = std::env::args().skip(1).collect();
    let install = Install::locate(&[]).expect("install");
    for zone in ["nazi_zombie_prototype", "common"] {
        let zd = t4::walk(decompress(&std::fs::read(install.fastfile(zone)).unwrap()).unwrap());
        for m in zd.xmodels.iter().filter(|m| !m.surfs.is_empty() && names.iter().any(|n| m.name == *n)) {
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for s in &m.surfs[m.lod_surfs(0)] {
                for v in t4::decode::model_surface(&zd, s).vertices {
                    for k in 0..3 {
                        lo[k] = lo[k].min(v.pos[k]);
                        hi[k] = hi[k].max(v.pos[k]);
                    }
                }
            }
            println!("{zone}/{}: verts bounds {:?}..{:?} bones {:?}", m.name, lo, hi, m.bones.iter().take(8).map(|b| (&b.name, b.base_trans)).collect::<Vec<_>>());
            if std::env::var_os("HITBOXES").is_some() {
                for b in m.bones.iter().filter(|b| b.hit_loc != 0 || b.hit_box.is_some()) {
                    println!("    {:<20} hitloc {:>2} box {:?}", b.name, b.hit_loc, b.hit_box);
                }
            }
        }
    }
}
