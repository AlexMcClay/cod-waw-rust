//! Prints a fastfile's asset list summary and its map entities.
//!
//! `cargo run -p waw_assets --example ffinfo -- <install> [zone_name]`

use std::collections::BTreeMap;
use std::path::PathBuf;
use waw_assets::mapents::{self, EntityList};
use waw_assets::zone::{asset_type, Zone};
use waw_assets::Install;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let explicit: Vec<PathBuf> = args.get(1).map(PathBuf::from).into_iter().collect();
    let install = Install::locate(&explicit).expect("install");
    let name = args.get(2).map(String::as_str).unwrap_or("nazi_zombie_prototype");
    let t = std::time::Instant::now();
    let zone = Zone::load(&install.fastfile(name)).expect("zone");
    println!("{name}: {} bytes, loaded in {:?}", zone.data.len(), t.elapsed());
    println!("blocks: {:?}", zone.block_sizes);
    println!("script strings: {}, assets: {}, data at {}", zone.script_strings.len(), zone.asset_types.len(), zone.data_start);
    let mut hist: BTreeMap<u32, usize> = BTreeMap::new();
    for &t in &zone.asset_types {
        *hist.entry(t).or_default() += 1;
    }
    for (t, n) in hist {
        println!("  {t:>3} {:<16} {n}", asset_type::name(t));
    }
    if let Some(text) = mapents::find_in_zone(&zone.data) {
        let ents = mapents::parse(text);
        println!("map entities: {}", ents.len());
        for e in ents.by_class("trigger_use") {
            println!("  {} {:?} cost={:?} weapon={:?} model={:?}", e.targetname(), e.origin(), e.get("zombie_cost"), e.get("zombie_weapon_upgrade"), e.get("model"));
        }
    }
}
