//! Lists inline images of a zone and checks a few model names.
use waw_assets::{t4, zone::decompress, Install};
fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "nazi_zombie_prototype".into());
    let install = Install::locate(&[]).expect("install");
    let zd = t4::walk(decompress(&std::fs::read(install.fastfile(&name)).unwrap()).unwrap());
    for i in &zd.images {
        if let Some(p) = &i.inline {
            println!("{} type={} {}x{} fmt={:#x} levels={} len={}", i.name, i.map_type, p.dims[0], p.dims[1], p.format, p.levels, p.len);
        }
    }
    for m in &zd.xmodels {
        if m.name.contains("colt") || m.name.contains("knife") || m.name.contains("hands") || m.name.contains("skybox") {
            println!("model {} surfs {} bones {}", m.name, m.surfs.len(), m.bones.len());
        }
    }
    if let Some(w) = &zd.world {
        println!("sky {:?} skybox {:?}", w.sky_image.map(|i| zd.image_name(i)), w.sky_box_model);
    }
}
