//! Lists the animations of zones whose names contain a filter, with their
//! length and notetracks: `xanims <filter> [zone...]`.
use waw_assets::{t4, zone::decompress, Install};
fn main() {
    let mut args = std::env::args().skip(1);
    let filter = args.next().unwrap_or_default().to_ascii_lowercase();
    let mut zones: Vec<String> = args.collect();
    if zones.is_empty() {
        zones = vec!["nazi_zombie_prototype".into(), "common".into()];
    }
    let install = Install::locate(&[]).expect("install");
    for z in &zones {
        let Ok(bytes) = std::fs::read(install.fastfile(z)) else { continue };
        let zd = t4::walk(decompress(&bytes).unwrap());
        for a in zd.xanims.iter().filter(|a| a.name.to_ascii_lowercase().contains(&filter)) {
            match t4::anim::decode(&zd, a) {
                Ok(c) => println!(
                    "{z}: {} {:.2}s loop {} notes {:?}",
                    a.name,
                    c.duration(),
                    c.looping,
                    c.notify.iter().map(|(n, t)| format!("{n}@{t:.2}")).collect::<Vec<_>>()
                ),
                Err(e) => println!("{z}: {} ({e})", a.name),
            }
        }
    }
}
