//! Lists the raw files (scripts, tables, vision files) of zones whose names
//! contain a filter: `rawlist <filter> [zone...]`.
use waw_assets::{t4, zone::decompress, Install};
fn main() {
    let mut args = std::env::args().skip(1);
    let filter = args.next().unwrap_or_default().to_ascii_lowercase();
    let mut zones: Vec<String> = args.collect();
    if zones.is_empty() {
        zones = vec!["nazi_zombie_prototype".into(), "patch".into(), "common".into()];
    }
    let install = Install::locate(&[]).expect("install");
    for z in &zones {
        let Ok(bytes) = std::fs::read(install.fastfile(z)) else { continue };
        let zd = t4::walk(decompress(&bytes).unwrap());
        for (name, _, len) in &zd.rawfiles {
            if name.to_ascii_lowercase().contains(&filter) {
                println!("{z}: {name} ({len} bytes)");
            }
        }
    }
}
