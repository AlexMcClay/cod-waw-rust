//! Groups world materials by technique set.
use std::collections::BTreeMap;
use waw_assets::{t4, zone::decompress, Install};
fn main() {
    let install = Install::locate(&[]).expect("install");
    let zd = t4::walk(decompress(&std::fs::read(install.fastfile("nazi_zombie_prototype")).unwrap()).unwrap());
    let w = zd.world.as_ref().unwrap();
    println!("lit {:?} decal {:?} emissive {:?} static {}", w.lit_range, w.decal_range, w.emissive_range, w.static_surface_count);
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut seen = std::collections::HashSet::new();
    for (i, s) in w.surfaces.iter().enumerate() {
        let Some(m) = s.material else { continue };
        if !seen.insert((m, w.decal_range.contains(&(i as u32)))) { continue }
        let mat = &zd.materials[m as usize];
        let tag = if w.decal_range.contains(&(i as u32)) { "DECAL " } else { "" };
        by.entry(format!("{tag}{}", mat.techset.clone().unwrap_or_default())).or_default().push(format!("{}[{}]", mat.name, mat.sort_key));
    }
    for (k, v) in by { println!("{k:40} {} e.g. {:?}", v.len(), &v[..v.len().min(4)]); }
}
