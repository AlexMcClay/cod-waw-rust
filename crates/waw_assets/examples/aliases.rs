//! Lists sound alias names matching a filter.
use waw_assets::{t4, zone::decompress, Install};
fn main() {
    let f = std::env::args().nth(1).unwrap_or_default();
    let install = Install::locate(&[]).expect("install");
    let zd = t4::walk(decompress(&std::fs::read(install.fastfile("nazi_zombie_prototype")).unwrap()).unwrap());
    for s in &zd.sounds {
        if s.name.contains(&f) {
            let kinds: Vec<String> = s.aliases.iter().map(|a| match &a.file { t4::SoundFile::Loaded(_) => "L".into(), t4::SoundFile::Streamed { name, .. } => format!("S:{name}"), t4::SoundFile::None => "-".into() }).collect();
            println!("{} x{} {:?} flags {:#x} dist {}-{}", s.name, s.aliases.len(), &kinds[..kinds.len().min(1)], s.aliases.first().map_or(0, |a| a.flags), s.aliases.first().map_or(0.0, |a| a.dist_min), s.aliases.first().map_or(0.0, |a| a.dist_max));
        }
    }
    for w in zd.weapons.iter().filter(|_| std::env::var_os("WEAPONS").is_some()) {
        println!("weapon {} fire={:?} reload={:?} view={:?}", w.name, w.sound("fireSound").or(w.sound("fireSoundPlayer")), w.sound("reloadSoundPlayer"), w.view_model.map(|i| &zd.xmodels[i as usize].name));
    }
}
