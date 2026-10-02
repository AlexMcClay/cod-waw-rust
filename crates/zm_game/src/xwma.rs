//! xWMA (WMA v2 in a RIFF container) decoding. About a tenth of World at
//! War's in-zone sounds (chalk, cha-ching, the box jingle, board repairs,
//! ambience) use it. There is no WMA decoder in the Rust ecosystem we ship
//! with, so on Windows the system's own Media Foundation WMA decoder does
//! the work; elsewhere these sounds are simply unavailable.

/// The pieces of an xWMA file.
struct Xwma<'a> {
    channels: u16,
    rate: u32,
    avg_bytes: u32,
    block_align: u16,
    bits: u16,
    /// Decoded size in bytes after each packet (the last is the total).
    total_pcm_bytes: Option<u32>,
    data: &'a [u8],
}

fn parse(bytes: &[u8]) -> Option<Xwma<'_>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"XWMA" {
        return None;
    }
    let rd16 = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let rd32 = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let mut fmt = None;
    let mut dpds = None;
    let mut data = None;
    let mut o = 12;
    while o + 8 <= bytes.len() {
        let id = &bytes[o..o + 4];
        let len = rd32(o + 4) as usize;
        let body = o + 8;
        let end = (body + len).min(bytes.len());
        match id {
            b"fmt " if len >= 16 => fmt = Some(body),
            b"dpds" if len >= 4 => dpds = Some(rd32(body + (len / 4 - 1) * 4)),
            b"data" => data = Some(&bytes[body..end]),
            _ => {}
        }
        o = body + len + (len & 1);
    }
    let f = fmt?;
    if rd16(f) != 0x161 {
        return None;
    }
    Some(Xwma {
        channels: rd16(f + 2),
        rate: rd32(f + 4),
        avg_bytes: rd32(f + 8),
        block_align: rd16(f + 12),
        bits: rd16(f + 14),
        total_pcm_bytes: dpds,
        data: data?,
    })
}

/// The stream's real bitrate in bytes per second. The decoder derives its
/// bitstream layout from it, and the header is not trustworthy: the game's
/// mono sounds say 12000 but are encoded at 6000. The decoded size table
/// tells the truth; snap that to the nearest bitrate xWMA allows.
fn bitrate(x: &Xwma) -> u32 {
    const RATES: [u32; 11] = [2000, 4000, 6000, 8000, 10000, 12000, 16000, 20000, 24000, 32000, 40000];
    let Some(total) = x.total_pcm_bytes.filter(|t| *t > 0) else { return x.avg_bytes };
    let pcm_per_sec = 2.0 * x.channels.max(1) as f64 * x.rate as f64;
    let est = x.data.len() as f64 * pcm_per_sec / total as f64;
    RATES.into_iter().min_by(|a, b| (*a as f64 - est).abs().total_cmp(&(*b as f64 - est).abs())).unwrap_or(x.avg_bytes)
}

/// True if `bytes` is an xWMA file.
pub fn is_xwma(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"XWMA"
}

/// Decodes an xWMA file to 16-bit PCM.
#[cfg(windows)]
pub fn decode(bytes: &[u8]) -> Option<zm_core::wav::Pcm> {
    let x = parse(bytes)?;
    mf::decode(&x)
}

#[cfg(not(windows))]
pub fn decode(_bytes: &[u8]) -> Option<zm_core::wav::Pcm> {
    None
}

#[cfg(windows)]
mod mf {
    use super::Xwma;
    use windows::Win32::Media::Audio::WAVEFORMATEX;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

    fn startup() -> bool {
        static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *OK.get_or_init(|| unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE).is_ok() })
    }

    /// WAVEFORMATEX for WMA v2 plus its 10 codec bytes (samples per block,
    /// encode options, super block align). xWMA leaves them out; these are
    /// the values its encoder always uses.
    fn wave_format(x: &Xwma) -> Vec<u8> {
        let samples_per_block: u32 = if x.rate <= 16000 {
            512
        } else if x.rate <= 22050 {
            1024
        } else {
            2048
        };
        let mut v = Vec::with_capacity(28);
        v.extend_from_slice(&0x161u16.to_le_bytes());
        v.extend_from_slice(&x.channels.to_le_bytes());
        v.extend_from_slice(&x.rate.to_le_bytes());
        v.extend_from_slice(&super::bitrate(x).to_le_bytes());
        v.extend_from_slice(&x.block_align.to_le_bytes());
        v.extend_from_slice(&x.bits.to_le_bytes());
        v.extend_from_slice(&10u16.to_le_bytes());
        v.extend_from_slice(&samples_per_block.to_le_bytes());
        v.extend_from_slice(&0x1fu16.to_le_bytes());
        v.extend_from_slice(&(x.block_align as u32).to_le_bytes());
        v
    }

    unsafe fn sample_from(bytes: &[u8]) -> windows::core::Result<IMFSample> {
        let buf = MFCreateMemoryBuffer(bytes.len() as u32)?;
        let mut p: *mut u8 = std::ptr::null_mut();
        buf.Lock(&mut p, None, None)?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        buf.Unlock()?;
        buf.SetCurrentLength(bytes.len() as u32)?;
        let s = MFCreateSample()?;
        s.AddBuffer(&buf)?;
        Ok(s)
    }

    /// Pulls every ready output sample into `out`.
    unsafe fn drain(t: &IMFTransform, out_size: u32, provides: bool, out: &mut Vec<u8>) -> windows::core::Result<()> {
        loop {
            let sample = if provides {
                None
            } else {
                let s = MFCreateSample()?;
                s.AddBuffer(&MFCreateMemoryBuffer(out_size.max(16384))?)?;
                Some(s)
            };
            let mut buffers = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: std::mem::ManuallyDrop::new(sample), dwStatus: 0, pEvents: std::mem::ManuallyDrop::new(None) }];
            let mut status = 0u32;
            let r = t.ProcessOutput(0, &mut buffers, &mut status);
            let sample = std::mem::ManuallyDrop::take(&mut buffers[0].pSample);
            let _events = std::mem::ManuallyDrop::take(&mut buffers[0].pEvents);
            match r {
                Ok(()) => {
                    if let Some(s) = sample {
                        let b = s.ConvertToContiguousBuffer()?;
                        let mut p: *mut u8 = std::ptr::null_mut();
                        let mut len = 0u32;
                        b.Lock(&mut p, None, Some(&mut len))?;
                        out.extend_from_slice(std::slice::from_raw_parts(p, len as usize));
                        b.Unlock()?;
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) => return Err(e),
            }
        }
    }

    pub(super) fn decode(x: &Xwma) -> Option<zm_core::wav::Pcm> {
        unsafe {
            // Each loader thread needs COM; "already initialised" is fine.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            if !startup() {
                return None;
            }
            let t: IMFTransform = CoCreateInstance(&CWMADecMediaObject, None, CLSCTX_INPROC_SERVER).ok()?;
            let fmt = wave_format(x);
            let input = MFCreateMediaType().ok()?;
            MFInitMediaTypeFromWaveFormatEx(&input, fmt.as_ptr() as *const WAVEFORMATEX, fmt.len() as u32).ok()?;
            t.SetInputType(0, &input, 0).ok()?;
            // 16-bit PCM out.
            let mut chosen = None;
            for i in 0.. {
                let Ok(ty) = t.GetOutputAvailableType(0, i) else { break };
                if ty.GetGUID(&MF_MT_SUBTYPE).ok() == Some(MFAudioFormat_PCM) && ty.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE).ok() == Some(16) {
                    chosen = Some(ty);
                    break;
                }
            }
            let out_ty = chosen?;
            t.SetOutputType(0, &out_ty, 0).ok()?;
            let channels = out_ty.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).ok()? as u16;
            let rate = out_ty.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).ok()?;
            let info = t.GetOutputStreamInfo(0).ok()?;
            let provides = info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32) != 0;
            let _ = t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
            let mut pcm = Vec::new();
            let packet = x.block_align.max(1) as usize;
            for chunk in x.data.chunks(packet) {
                let s = sample_from(chunk).ok()?;
                if t.ProcessInput(0, &s, 0).is_err() {
                    drain(&t, info.cbSize, provides, &mut pcm).ok()?;
                    t.ProcessInput(0, &s, 0).ok()?;
                }
                drain(&t, info.cbSize, provides, &mut pcm).ok()?;
            }
            let _ = t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = t.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0);
            drain(&t, info.cbSize, provides, &mut pcm).ok()?;
            if let Some(total) = x.total_pcm_bytes {
                pcm.truncate(total as usize);
            }
            let samples = pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
            Some(zm_core::wav::Pcm { channels, sample_rate: rate, samples })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn decode_file() {
        let p = std::env::var("XW_FILE").unwrap();
        let b = std::fs::read(p).unwrap();
        let pcm = decode(&b).expect("decode");
        let rms = (pcm.samples.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / pcm.samples.len().max(1) as f64).sqrt();
        println!("XWRESULT ch={} rate={} secs={:.2} rms={rms:.0}", pcm.channels, pcm.sample_rate, pcm.duration_secs());
    }

    #[test]
    fn rejects_non_xwma() {
        assert!(!is_xwma(b"RIFF\0\0\0\0WAVEfmt "));
        assert!(parse(b"nothing").is_none());
    }
}
