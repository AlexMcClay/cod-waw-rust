//! Prints a raw file (script, vision set...) from a zone.
//!
//! `cargo run --release -p waw_assets --example rawfile -- <zone> <name>`

use waw_assets::t4;
use waw_assets::zone::decompress;
use waw_assets::Install;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(zone), Some(name)) = (args.next(), args.next()) else {
        eprintln!("usage: rawfile <zone> <name>");
        return;
    };
    let install = Install::locate(&[]).expect("install");
    let zd = t4::walk(decompress(&std::fs::read(install.fastfile(&zone)).expect("read")).expect("decompress"));
    match zd.rawfile(&name) {
        Some(text) => print!("{text}"),
        None => eprintln!("{name} not in {zone}"),
    }
}
