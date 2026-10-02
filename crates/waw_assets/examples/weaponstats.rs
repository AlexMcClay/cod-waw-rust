//! Prints the gameplay numbers of a zone's weapons and where they differ
//! from the plain-text weapon files in the IWD archives.
//!
//! `cargo run -p waw_assets --example weaponstats -- [zone] [weapon...]`
use waw_assets::{t4, zone::decompress, Install, Iwd};

fn main() {
    let mut args = std::env::args().skip(1);
    let zone = args.next().unwrap_or_else(|| "nazi_zombie_prototype".into());
    let only: Vec<String> = args.collect();
    let install = Install::locate(&[]).expect("install");
    let iwd = Iwd::open(&install.main_dir()).expect("iwd");
    let zd = t4::walk(decompress(&std::fs::read(install.fastfile(&zone)).unwrap()).unwrap());
    for w in &zd.weapons {
        if !only.is_empty() && !only.iter().any(|o| o.eq_ignore_ascii_case(&w.name)) {
            continue;
        }
        println!("== {} ({})", w.name, w.display_name);
        let file = iwd.read(&format!("weapons/sp/{}", w.name)).map(|b| b.iter().map(|&c| c as char).collect::<String>());
        let fields: std::collections::HashMap<String, String> = file
            .as_deref()
            .map(|t| {
                let p: Vec<&str> = t.split('\\').skip(1).collect();
                p.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_string(), c[1].to_string())).collect()
            })
            .unwrap_or_default();
        for (k, v) in &w.stats {
            let f = fields.get(*k);
            let same = match (f.and_then(|f| f.trim().parse::<f32>().ok()), v.parse::<f32>().ok()) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-3,
                _ => f.is_some_and(|f| f.eq_ignore_ascii_case(v)),
            };
            let mark = if file.is_none() { "" } else if same { "" } else { "   <-- file: " };
            println!("  {k:24} {v:>12}{mark}{}", if same || file.is_none() { "" } else { f.map(String::as_str).unwrap_or("(missing)") });
        }
    }
}
