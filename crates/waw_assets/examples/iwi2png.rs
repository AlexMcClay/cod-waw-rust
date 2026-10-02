//! Converts an IWI from the install's IWD archives to PNG.
//!
//! `cargo run -p waw_assets --example iwi2png -- <image_name> [out.png]`

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;
use waw_assets::iwi::Iwi;
use waw_assets::{Install, Iwd};

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((w * h * 4 + h) as usize);
    for row in rgba.chunks_exact(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(&raw).unwrap();
    let idat = enc.finish().unwrap();
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |tag: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = tag.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &idat);
    chunk(b"IEND", &[]);
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).expect("image name");
    let out = args.get(2).cloned().unwrap_or_else(|| format!("{name}.png"));
    let install = Install::locate(&[]).expect("install");
    let iwd = Iwd::open(&install.main_dir()).expect("iwd");
    let bytes = iwd.read_image(name).expect("image not found");
    let iwi = Iwi::parse(&bytes).expect("parse");
    println!("{name}: {:?} {}x{} flags=0x{:02x} levels={}", iwi.format, iwi.width, iwi.height, iwi.flags, iwi.levels.len());
    std::fs::write(&out, png(iwi.width, iwi.height, &iwi.to_rgba())).unwrap();
    println!("wrote {out}");
}
