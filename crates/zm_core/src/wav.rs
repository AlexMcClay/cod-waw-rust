//! Minimal RIFF/WAVE decoder.
//!
//! Bevy's WAV support only understands PCM. The `.wav` files inside the game's
//! `.iwd` archives are mostly Microsoft ADPCM (format tag 2), so this module
//! decodes MS-ADPCM and IMA-ADPCM into 16-bit PCM and re-wraps the result in a
//! canonical PCM WAV container that any player can read.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WavError {
    NotRiff,
    MissingChunk(&'static str),
    Truncated,
    Unsupported(u16, u16),
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WavError::NotRiff => write!(f, "not a RIFF/WAVE file"),
            WavError::MissingChunk(c) => write!(f, "missing '{c}' chunk"),
            WavError::Truncated => write!(f, "file is truncated"),
            WavError::Unsupported(tag, bits) => {
                write!(f, "unsupported WAV format tag {tag} ({bits} bits)")
            }
        }
    }
}

impl std::error::Error for WavError {}

#[derive(Debug, Clone)]
pub struct Format {
    pub tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub block_align: u16,
    pub bits: u16,
    pub extra: Vec<u8>,
}

/// Decoded PCM16 audio, interleaved.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

impl Pcm {
    pub fn duration_secs(&self) -> f32 {
        if self.channels == 0 || self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / self.channels as f32 / self.sample_rate as f32
    }

    /// Serialise as a canonical 44-byte-header PCM16 WAV file.
    pub fn to_wav_bytes(&self) -> Vec<u8> {
        let data_len = (self.samples.len() * 2) as u32;
        let block_align = self.channels * 2;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&self.channels.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&(self.sample_rate * block_align as u32).to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in &self.samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }
}

fn u16_at(b: &[u8], o: usize) -> Result<u16, WavError> {
    b.get(o..o + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(WavError::Truncated)
}
fn u32_at(b: &[u8], o: usize) -> Result<u32, WavError> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(WavError::Truncated)
}
fn i16_at(b: &[u8], o: usize) -> Result<i16, WavError> {
    u16_at(b, o).map(|v| v as i16)
}

/// Split a RIFF/WAVE file into its format description and raw `data` payload.
pub fn parse(bytes: &[u8]) -> Result<(Format, &[u8]), WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotRiff);
    }
    let mut fmt: Option<Format> = None;
    let mut data: Option<&[u8]> = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32_at(bytes, pos + 4)? as usize;
        let body_start = pos + 8;
        // Some writers lie about the final chunk length; clamp to file size.
        let body_end = (body_start + len).min(bytes.len());
        let body = &bytes[body_start..body_end];
        match id {
            b"fmt " => {
                if body.len() < 16 {
                    return Err(WavError::Truncated);
                }
                let extra = if body.len() > 18 {
                    let cb = u16_at(body, 16)? as usize;
                    body[18..(18 + cb).min(body.len())].to_vec()
                } else {
                    Vec::new()
                };
                fmt = Some(Format {
                    tag: u16_at(body, 0)?,
                    channels: u16_at(body, 2)?,
                    sample_rate: u32_at(body, 4)?,
                    block_align: u16_at(body, 12)?,
                    bits: u16_at(body, 14)?,
                    extra,
                });
            }
            b"data" => data = Some(body),
            _ => {}
        }
        pos = body_start + len + (len & 1);
    }
    Ok((
        fmt.ok_or(WavError::MissingChunk("fmt "))?,
        data.ok_or(WavError::MissingChunk("data"))?,
    ))
}

/// Decode any supported WAV (PCM8/16, MS-ADPCM, IMA-ADPCM) to PCM16.
pub fn decode(bytes: &[u8]) -> Result<Pcm, WavError> {
    let (fmt, data) = parse(bytes)?;
    if fmt.channels == 0 || fmt.channels > 2 {
        return Err(WavError::Unsupported(fmt.tag, fmt.bits));
    }
    let samples = match (fmt.tag, fmt.bits) {
        (1, 16) => data
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect(),
        (1, 8) => data.iter().map(|&b| ((b as i16) - 128) << 8).collect(),
        (2, _) => decode_ms_adpcm(&fmt, data)?,
        (0x11, _) => decode_ima_adpcm(&fmt, data)?,
        (tag, bits) => return Err(WavError::Unsupported(tag, bits)),
    };
    Ok(Pcm {
        channels: fmt.channels,
        sample_rate: fmt.sample_rate,
        samples,
    })
}

/// Convenience: decode and re-encode as PCM16 WAV bytes.
pub fn to_pcm_wav(bytes: &[u8]) -> Result<Vec<u8>, WavError> {
    decode(bytes).map(|p| p.to_wav_bytes())
}

// ---------------------------------------------------------------------------
// Microsoft ADPCM
// ---------------------------------------------------------------------------

const MS_ADAPT: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];
const MS_DEFAULT_COEFS: [(i32, i32); 7] = [
    (256, 0),
    (512, -256),
    (0, 0),
    (192, 64),
    (240, 0),
    (460, -208),
    (392, -232),
];

#[derive(Clone, Copy, Default)]
struct MsChannel {
    c1: i32,
    c2: i32,
    delta: i32,
    s1: i32,
    s2: i32,
}

impl MsChannel {
    fn step(&mut self, nibble: u8) -> i16 {
        let signed = if nibble >= 8 { nibble as i32 - 16 } else { nibble as i32 };
        let predicted = (self.s1 * self.c1 + self.s2 * self.c2) >> 8;
        let sample = (predicted + signed * self.delta).clamp(-32768, 32767);
        self.s2 = self.s1;
        self.s1 = sample;
        self.delta = ((MS_ADAPT[nibble as usize] * self.delta) >> 8).max(16);
        sample as i16
    }
}

fn decode_ms_adpcm(fmt: &Format, data: &[u8]) -> Result<Vec<i16>, WavError> {
    let ch = fmt.channels as usize;
    let block_align = fmt.block_align as usize;
    if block_align < 7 * ch {
        return Err(WavError::Unsupported(fmt.tag, fmt.bits));
    }
    // Coefficient table from the fmt extension, falling back to the standard 7.
    let mut coefs: Vec<(i32, i32)> = MS_DEFAULT_COEFS.to_vec();
    if fmt.extra.len() >= 4 {
        let n = u16_at(&fmt.extra, 2)? as usize;
        if n > 0 && fmt.extra.len() >= 4 + n * 4 {
            coefs = (0..n)
                .map(|i| {
                    let o = 4 + i * 4;
                    (
                        i16_at(&fmt.extra, o).unwrap_or(0) as i32,
                        i16_at(&fmt.extra, o + 2).unwrap_or(0) as i32,
                    )
                })
                .collect();
        }
    }

    let mut out = Vec::with_capacity(data.len() * 2);
    for block in data.chunks(block_align) {
        if block.len() < 7 * ch {
            break;
        }
        let mut st = [MsChannel::default(); 2];
        for c in 0..ch {
            let pred = block[c] as usize;
            let (c1, c2) = *coefs.get(pred).unwrap_or(&coefs[0]);
            st[c].c1 = c1;
            st[c].c2 = c2;
            st[c].delta = i16_at(block, ch + c * 2)? as i32;
            st[c].s1 = i16_at(block, 3 * ch + c * 2)? as i32;
            st[c].s2 = i16_at(block, 5 * ch + c * 2)? as i32;
        }
        // The header carries the first two samples, oldest (s2) first.
        for c in 0..ch {
            out.push(st[c].s2 as i16);
        }
        for c in 0..ch {
            out.push(st[c].s1 as i16);
        }
        let mut c = 0usize;
        for &byte in &block[7 * ch..] {
            for nib in [byte >> 4, byte & 0x0f] {
                out.push(st[c].step(nib));
                c = (c + 1) % ch;
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// IMA ADPCM (DVI)
// ---------------------------------------------------------------------------

const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];
const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408,
    449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066,
    2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630,
    9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];

#[derive(Clone, Copy, Default)]
struct ImaChannel {
    pred: i32,
    index: i32,
}

impl ImaChannel {
    fn step(&mut self, nibble: u8) -> i16 {
        let step = IMA_STEPS[self.index as usize];
        let mut diff = step >> 3;
        if nibble & 1 != 0 {
            diff += step >> 2;
        }
        if nibble & 2 != 0 {
            diff += step >> 1;
        }
        if nibble & 4 != 0 {
            diff += step;
        }
        if nibble & 8 != 0 {
            diff = -diff;
        }
        self.pred = (self.pred + diff).clamp(-32768, 32767);
        self.index = (self.index + IMA_INDEX[nibble as usize]).clamp(0, 88);
        self.pred as i16
    }
}

fn decode_ima_adpcm(fmt: &Format, data: &[u8]) -> Result<Vec<i16>, WavError> {
    let ch = fmt.channels as usize;
    let block_align = fmt.block_align as usize;
    if block_align < 4 * ch {
        return Err(WavError::Unsupported(fmt.tag, fmt.bits));
    }
    let mut out = Vec::with_capacity(data.len() * 2);
    for block in data.chunks(block_align) {
        if block.len() < 4 * ch {
            break;
        }
        let mut st = [ImaChannel::default(); 2];
        for c in 0..ch {
            st[c].pred = i16_at(block, c * 4)? as i32;
            st[c].index = (block[c * 4 + 2] as i32).clamp(0, 88);
            out.push(st[c].pred as i16);
        }
        let body = &block[4 * ch..];
        if ch == 1 {
            for &b in body {
                out.push(st[0].step(b & 0x0f));
                out.push(st[0].step(b >> 4));
            }
        } else {
            // Stereo: 4 bytes (8 samples) of left, then 4 bytes of right, repeating.
            for group in body.chunks(8) {
                if group.len() < 8 {
                    break;
                }
                let mut l = [0i16; 8];
                let mut r = [0i16; 8];
                for i in 0..4 {
                    l[i * 2] = st[0].step(group[i] & 0x0f);
                    l[i * 2 + 1] = st[0].step(group[i] >> 4);
                    r[i * 2] = st[1].step(group[4 + i] & 0x0f);
                    r[i * 2 + 1] = st[1].step(group[4 + i] >> 4);
                }
                for i in 0..8 {
                    out.push(l[i]);
                    out.push(r[i]);
                }
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Procedural sounds (used when the user's install has no matching asset)
// ---------------------------------------------------------------------------

/// Small deterministic xorshift so synthesis needs no RNG crate.
fn noise(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// A gunshot-like burst: filtered noise with a fast attack, exponential decay
/// and a low "thump". `weight` 0..1 controls how heavy it sounds.
pub fn synth_shot(weight: f32, seed: u32) -> Pcm {
    let rate = 22050u32;
    let len = (rate as f32 * (0.18 + 0.35 * weight)) as usize;
    let mut s = seed | 1;
    let mut lp = 0.0f32;
    let cutoff = 0.25 + 0.5 * (1.0 - weight);
    let samples = (0..len)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let env = (-t * (28.0 - 16.0 * weight)).exp();
            lp += cutoff * (noise(&mut s) - lp);
            let thump = (t * (90.0 - 40.0 * weight) * std::f32::consts::TAU).sin()
                * (-t * 18.0).exp()
                * 0.6;
            ((lp * 0.9 + thump) * env * 30000.0).clamp(-32767.0, 32767.0) as i16
        })
        .collect();
    Pcm { channels: 1, sample_rate: rate, samples }
}

/// A sweeping tone, used for pickups, purchases and UI blips.
pub fn synth_tone(f0: f32, f1: f32, secs: f32, square: bool) -> Pcm {
    let rate = 22050u32;
    let len = (rate as f32 * secs) as usize;
    let mut phase = 0.0f32;
    let samples = (0..len)
        .map(|i| {
            let x = i as f32 / len as f32;
            let f = f0 + (f1 - f0) * x;
            phase += f / rate as f32;
            let w = (phase * std::f32::consts::TAU).sin();
            let w = if square { w.signum() * 0.5 } else { w };
            let env = (x * 40.0).min(1.0) * (1.0 - x).powf(1.5);
            (w * env * 14000.0) as i16
        })
        .collect();
    Pcm { channels: 1, sample_rate: rate, samples }
}

/// A low, wet groan for the zombies (formant-ish filtered saw + noise).
pub fn synth_groan(pitch: f32, secs: f32, seed: u32) -> Pcm {
    let rate = 22050u32;
    let len = (rate as f32 * secs) as usize;
    let mut s = seed | 1;
    let mut phase = 0.0f32;
    let mut lp = 0.0f32;
    let samples = (0..len)
        .map(|i| {
            let x = i as f32 / len as f32;
            let wobble = 1.0 + 0.08 * (x * 23.0).sin() + 0.03 * noise(&mut s);
            phase = (phase + pitch * wobble / rate as f32).fract();
            let saw = phase * 2.0 - 1.0;
            lp += 0.08 * (saw + 0.4 * noise(&mut s) - lp);
            let env = (x * 8.0).min(1.0) * (1.0 - x).powf(0.8);
            (lp * env * 26000.0).clamp(-32767.0, 32767.0) as i16
        })
        .collect();
    Pcm { channels: 1, sample_rate: rate, samples }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_with(fmt_body: &[u8], data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&((4 + 8 + fmt_body.len() + 8 + data.len()) as u32).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&(fmt_body.len() as u32).to_le_bytes());
        v.extend_from_slice(fmt_body);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    #[test]
    fn pcm16_roundtrip() {
        let pcm = Pcm { channels: 1, sample_rate: 8000, samples: vec![0, 100, -100, 32767] };
        let back = decode(&pcm.to_wav_bytes()).unwrap();
        assert_eq!(back, pcm);
    }

    #[test]
    fn ms_adpcm_constant_block() {
        // Mono MS-ADPCM, predictor 0 (c1=256,c2=0), delta 16, s1=100, s2=50,
        // all-zero nibbles -> output 50, 100, then 100 forever.
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&2u16.to_le_bytes()); // tag
        fmt.extend_from_slice(&1u16.to_le_bytes()); // channels
        fmt.extend_from_slice(&22050u32.to_le_bytes());
        fmt.extend_from_slice(&11025u32.to_le_bytes());
        fmt.extend_from_slice(&11u16.to_le_bytes()); // block align: 7 + 4
        fmt.extend_from_slice(&4u16.to_le_bytes()); // bits
        fmt.extend_from_slice(&2u16.to_le_bytes()); // cbSize
        fmt.extend_from_slice(&10u16.to_le_bytes()); // samples per block
        let mut block = vec![0u8];
        block.extend_from_slice(&16i16.to_le_bytes());
        block.extend_from_slice(&100i16.to_le_bytes());
        block.extend_from_slice(&50i16.to_le_bytes());
        block.extend_from_slice(&[0, 0, 0, 0]);
        let pcm = decode(&wav_with(&fmt, &block)).unwrap();
        assert_eq!(pcm.samples, vec![50, 100, 100, 100, 100, 100, 100, 100, 100, 100]);
    }

    #[test]
    fn ms_adpcm_step_moves_toward_sign() {
        let mut ch = MsChannel { c1: 256, c2: 0, delta: 16, s1: 0, s2: 0 };
        assert_eq!(ch.step(0x7), 112); // +7 * 16
        assert!(ch.step(0x9) < 112); // negative nibble pulls down
    }

    #[test]
    fn ima_adpcm_mono_block() {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&0x11u16.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&8000u32.to_le_bytes());
        fmt.extend_from_slice(&4000u32.to_le_bytes());
        fmt.extend_from_slice(&8u16.to_le_bytes());
        fmt.extend_from_slice(&4u16.to_le_bytes());
        let block = [0x10, 0x00, 0x00, 0x00, 0x77, 0x77, 0x00, 0x00];
        let pcm = decode(&wav_with(&fmt, &block)).unwrap();
        assert_eq!(pcm.samples.len(), 1 + 8);
        assert_eq!(pcm.samples[0], 16);
        assert!(pcm.samples[1] > 16 && pcm.samples[4] > pcm.samples[1]);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(decode(b"hello").unwrap_err(), WavError::NotRiff);
    }

    #[test]
    fn synth_lengths() {
        assert!(synth_shot(0.5, 7).duration_secs() > 0.2);
        assert!((synth_tone(440.0, 880.0, 0.5, false).duration_secs() - 0.5).abs() < 0.01);
        assert!(synth_groan(80.0, 1.0, 3).samples.iter().any(|&s| s != 0));
    }
}
