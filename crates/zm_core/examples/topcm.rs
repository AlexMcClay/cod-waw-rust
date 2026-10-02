//! Converts a WAV (PCM or IMA/MS ADPCM) to 16-bit PCM: `topcm <in.wav> <out.wav>`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: topcm <in.wav> <out.wav>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(input).expect("read input");
    let pcm = zm_core::wav::to_pcm_wav(&bytes).expect("decode");
    std::fs::write(output, pcm).expect("write output");
}
