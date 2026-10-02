//! `cargo run -p zm_core --example wavinfo -- file.wav [out.wav]`
//! Prints format info and optionally writes a PCM16 copy.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: wavinfo <in.wav> [out.wav]");
        std::process::exit(2);
    };
    let bytes = std::fs::read(path).expect("read");
    let (fmt, data) = zm_core::wav::parse(&bytes).expect("parse");
    println!("tag={} ch={} rate={} align={} bits={} data={}B", fmt.tag, fmt.channels, fmt.sample_rate, fmt.block_align, fmt.bits, data.len());
    let pcm = zm_core::wav::decode(&bytes).expect("decode");
    let peak = pcm.samples.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0);
    let rms = (pcm.samples.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / pcm.samples.len().max(1) as f64).sqrt();
    println!("decoded {} samples, {:.2}s, peak={} rms={:.0}", pcm.samples.len(), pcm.duration_secs(), peak, rms);
    if let Some(out) = args.get(1) {
        std::fs::write(out, pcm.to_wav_bytes()).expect("write");
    }
}
