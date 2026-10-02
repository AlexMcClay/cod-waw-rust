//! Prints the colour map of materials whose name contains a filter, with the
//! image's format in the install's IWD archives.
//!
//! `cargo run -p waw_assets --example matimages -- <zone> <filter>`
use waw_assets::{iwd::Iwd, iwi::Iwi, t4, zone::decompress, Install};
fn main() {
    let mut a = std::env::args().skip(1);
    let (zone, filter) = (a.next().unwrap_or_else(|| "nazi_zombie_prototype".into()), a.next().unwrap_or_default());
    let install = Install::locate(&[]).expect("install");
    let iwd = Iwd::open(&install.main_dir()).expect("iwd");
    for z in [zone.as_str(), "common", "patch"] {
        let Ok(bytes) = std::fs::read(install.fastfile(z)) else { continue };
        let zd = t4::walk(decompress(&bytes).unwrap());
        for m in zd.materials.iter().filter(|m| m.name.contains(&filter) && m.techset.is_some()) {
            let img = m.color_map().map(|t| zd.image_name(t).to_string());
            let fmt = img.as_deref().and_then(|n| iwd.read_image(n)).map(|b| Iwi::parse(&b).map(|i| format!("{:?} {}x{}", i.format, i.width, i.height)).unwrap_or_else(|e| format!("unreadable: {e:?}")));
            println!("{z}: {} techset {:?} image {:?} {:?}", m.name, m.techset, img, fmt);
        }
    }
}
