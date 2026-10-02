//! Prints each LOD-0 surface's material, technique set and colour map of
//! the named models: `modelmats <model>...`.
use waw_assets::{iwi::Iwi, t4, zone::decompress, Install, Iwd};

fn main() {
    let names: Vec<String> = std::env::args().skip(1).collect();
    let install = Install::locate(&[]).expect("install");
    let iwd = Iwd::open(&install.main_dir()).expect("iwd");
    for zone in ["nazi_zombie_prototype", "common"] {
        let zd = t4::walk(decompress(&std::fs::read(install.fastfile(zone)).unwrap()).unwrap());
        for m in zd.xmodels.iter().filter(|m| !m.surfs.is_empty() && names.iter().any(|n| m.name == *n)) {
            println!("{zone}/{} (coll surfs {})", m.name, m.num_coll_surfs);
            for s in m.lod_surfs(0) {
                let Some(mi) = m.materials.get(s).copied().flatten() else { continue };
                let mat = &zd.materials[mi as usize];
                let image = mat.color_map().map(|i| zd.image_name(i).to_string()).unwrap_or_default();
                let format = iwd.read(&format!("images/{image}.iwi")).and_then(|b| Iwi::parse(&b).ok()).map(|i| format!("{:?}", i.format)).unwrap_or("-".into());
                if let Some((p, n)) = mat.state_bits {
                    let bits: Vec<String> = (0..n).map(|i| { let o = p + 8 * i; format!("{:08x}:{:08x}", u32::from_le_bytes(zd.data[o..o+4].try_into().unwrap()), u32::from_le_bytes(zd.data[o+4..o+8].try_into().unwrap())) }).collect();
                    println!("    states {bits:?}");
                    println!("    entry {:?}", mat.state_entry.iter().enumerate().filter(|(_, v)| **v != 0xff).map(|(i, v)| (i, *v)).collect::<Vec<_>>());
                }
                println!("  {:40} techset {:40} colour {image} ({format}) sort {} lit state {:?}", mat.name, mat.techset.as_deref().unwrap_or("-"), mat.sort_key, zd.lit_state(mat));
            }
        }
    }
}
