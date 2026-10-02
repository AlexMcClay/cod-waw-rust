//! `.iwi` textures (version 6, as used by World at War).
//!
//! Header (28 bytes): `"IWi"`, version u8, format u8, flags u8, width u16,
//! height u16, depth u16, then four u32 picmip file offsets. Mip levels follow
//! smallest first, so the full-size image is at the end of the file.
//!
//! Formats 6-10 are the engine's wavelet codecs, used by a handful of UI
//! images; they are reported as unsupported and callers fall back.

pub const HEADER_LEN: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Argb32,
    Rgb24,
    LumAlpha16,
    Luminance8,
    Alpha8,
    Dxt1,
    Dxt3,
    Dxt5,
}

impl Format {
    fn from_byte(b: u8) -> Option<Format> {
        Some(match b {
            0x01 => Format::Argb32,
            0x02 => Format::Rgb24,
            0x03 => Format::LumAlpha16,
            0x04 => Format::Luminance8,
            0x05 => Format::Alpha8,
            0x0B => Format::Dxt1,
            0x0C => Format::Dxt3,
            0x0D => Format::Dxt5,
            _ => return None,
        })
    }

    pub fn is_compressed(self) -> bool {
        matches!(self, Format::Dxt1 | Format::Dxt3 | Format::Dxt5)
    }

    /// Byte size of one mip level.
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let (w, h) = (w.max(1) as usize, h.max(1) as usize);
        let blocks = w.div_ceil(4) * h.div_ceil(4);
        match self {
            Format::Dxt1 => blocks * 8,
            Format::Dxt3 | Format::Dxt5 => blocks * 16,
            Format::Argb32 => w * h * 4,
            Format::Rgb24 => w * h * 3,
            Format::LumAlpha16 => w * h * 2,
            Format::Luminance8 | Format::Alpha8 => w * h,
        }
    }
}

pub mod flags {
    pub const NO_PICMIP: u8 = 0x01;
    pub const NO_MIPMAPS: u8 = 0x02;
    pub const CUBEMAP: u8 = 0x04;
    pub const VOLUME: u8 = 0x08;
}

#[derive(Debug, PartialEq, Eq)]
pub enum IwiError {
    NotIwi,
    UnsupportedVersion(u8),
    UnsupportedFormat(u8),
    Cubemap,
    Truncated,
}

impl std::fmt::Display for IwiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IwiError::NotIwi => write!(f, "not an IWI file"),
            IwiError::UnsupportedVersion(v) => write!(f, "unsupported IWI version {v}"),
            IwiError::UnsupportedFormat(v) => write!(f, "unsupported IWI format 0x{v:02x}"),
            IwiError::Cubemap => write!(f, "cubemaps are not supported"),
            IwiError::Truncated => write!(f, "IWI data truncated"),
        }
    }
}

impl std::error::Error for IwiError {}

/// A parsed IWI with its mip chain, largest level first.
#[derive(Debug, Clone)]
pub struct Iwi {
    pub format: Format,
    pub flags: u8,
    pub width: u32,
    pub height: u32,
    /// Raw level data in the file's format, `levels[0]` is full size.
    pub levels: Vec<Vec<u8>>,
}

impl Iwi {
    pub fn parse(bytes: &[u8]) -> Result<Iwi, IwiError> {
        if bytes.len() < HEADER_LEN || &bytes[0..3] != b"IWi" {
            return Err(IwiError::NotIwi);
        }
        if bytes[3] != 6 {
            return Err(IwiError::UnsupportedVersion(bytes[3]));
        }
        let format = Format::from_byte(bytes[4]).ok_or(IwiError::UnsupportedFormat(bytes[4]))?;
        let flags = bytes[5];
        if flags & flags::CUBEMAP != 0 {
            return Err(IwiError::Cubemap);
        }
        let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        let data = &bytes[HEADER_LEN..];

        let full_chain = if flags & flags::NO_MIPMAPS != 0 { 1 } else { 32 - width.max(height).max(1).leading_zeros() };
        // Use as many levels as the file actually holds (smallest are first).
        let sizes: Vec<usize> = (0..full_chain).map(|l| format.level_size(width >> l, height >> l)).collect();
        let total: usize = sizes.iter().sum();
        let count = if data.len() >= total {
            full_chain as usize
        } else {
            // Fall back to the largest level only.
            if data.len() < sizes[0] {
                return Err(IwiError::Truncated);
            }
            1
        };
        let mut levels = Vec::with_capacity(count);
        let mut end = data.len();
        for size in sizes.iter().take(count) {
            levels.push(data[end - size..end].to_vec());
            end -= size;
        }
        Ok(Iwi { format, flags, width, height, levels })
    }

    /// Decodes the full-size level to RGBA8.
    pub fn to_rgba(&self) -> Vec<u8> {
        decode_level(self.format, &self.levels[0], self.width, self.height)
    }

    /// Decodes level `l` to RGBA8 with its dimensions.
    pub fn level_rgba(&self, l: usize) -> (u32, u32, Vec<u8>) {
        let (w, h) = ((self.width >> l).max(1), (self.height >> l).max(1));
        (w, h, decode_level(self.format, &self.levels[l], w, h))
    }
}

/// Decodes one level of `format` data to RGBA8.
pub fn decode_level(format: Format, data: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    match format {
        Format::Argb32 => {
            for (o, i) in out.chunks_exact_mut(4).zip(data.chunks_exact(4)) {
                o.copy_from_slice(&[i[2], i[1], i[0], i[3]]);
            }
        }
        Format::Rgb24 => {
            for (o, i) in out.chunks_exact_mut(4).zip(data.chunks_exact(3)) {
                o.copy_from_slice(&[i[2], i[1], i[0], 255]);
            }
        }
        Format::LumAlpha16 => {
            for (o, i) in out.chunks_exact_mut(4).zip(data.chunks_exact(2)) {
                o.copy_from_slice(&[i[0], i[0], i[0], i[1]]);
            }
        }
        Format::Luminance8 => {
            for (o, &l) in out.chunks_exact_mut(4).zip(data) {
                o.copy_from_slice(&[l, l, l, 255]);
            }
        }
        Format::Alpha8 => {
            for (o, &a) in out.chunks_exact_mut(4).zip(data) {
                o.copy_from_slice(&[255, 255, 255, a]);
            }
        }
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            let block_bytes = if format == Format::Dxt1 { 8 } else { 16 };
            let bw = w.div_ceil(4);
            for (bi, block) in data.chunks_exact(block_bytes).enumerate() {
                let (bx, by) = (bi % bw, bi / bw);
                if by * 4 >= h {
                    break;
                }
                let px = decode_block(format, block);
                for y in 0..4 {
                    for x in 0..4 {
                        let (ix, iy) = (bx * 4 + x, by * 4 + y);
                        if ix < w && iy < h {
                            let o = (iy * w + ix) * 4;
                            out[o..o + 4].copy_from_slice(&px[y * 4 + x]);
                        }
                    }
                }
            }
        }
    }
    out
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [((r * 527 + 23) >> 6) as u8, ((g * 259 + 33) >> 6) as u8, ((b * 527 + 23) >> 6) as u8]
}

fn color_block(b: &[u8], allow_punchthrough: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u32, wb: u32| ((a as u32 * wa + b as u32 * wb) / (wa + wb)) as u8;
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || !allow_punchthrough {
        pal[2] = [mix(p0[0], p1[0], 2, 1), mix(p0[1], p1[1], 2, 1), mix(p0[2], p1[2], 2, 1), 255];
        pal[3] = [mix(p0[0], p1[0], 1, 2), mix(p0[1], p1[1], 1, 2), mix(p0[2], p1[2], 1, 2), 255];
    } else {
        pal[2] = [mix(p0[0], p1[0], 1, 1), mix(p0[1], p1[1], 1, 1), mix(p0[2], p1[2], 1, 1), 255];
        pal[3] = [0, 0, 0, 0];
    }
    let idx = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let mut out = [[0u8; 4]; 16];
    for (i, px) in out.iter_mut().enumerate() {
        *px = pal[((idx >> (i * 2)) & 3) as usize];
    }
    out
}

fn decode_block(format: Format, b: &[u8]) -> [[u8; 4]; 16] {
    match format {
        Format::Dxt1 => color_block(b, true),
        Format::Dxt3 => {
            let mut px = color_block(&b[8..16], false);
            for (i, p) in px.iter_mut().enumerate() {
                let nib = (b[i / 2] >> ((i % 2) * 4)) & 15;
                p[3] = nib * 17;
            }
            px
        }
        Format::Dxt5 => {
            let mut px = color_block(&b[8..16], false);
            let (a0, a1) = (b[0] as u32, b[1] as u32);
            let mut alpha = [0u8; 8];
            alpha[0] = a0 as u8;
            alpha[1] = a1 as u8;
            if a0 > a1 {
                for i in 1..7u32 {
                    alpha[i as usize + 1] = (((7 - i) * a0 + i * a1) / 7) as u8;
                }
            } else {
                for i in 1..5u32 {
                    alpha[i as usize + 1] = (((5 - i) * a0 + i * a1) / 5) as u8;
                }
                alpha[6] = 0;
                alpha[7] = 255;
            }
            let bits = b[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | x as u64);
            for (i, p) in px.iter_mut().enumerate() {
                p[3] = alpha[((bits >> (i * 3)) & 7) as usize];
            }
            px
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(format: u8, flags: u8, w: u16, h: u16) -> Vec<u8> {
        let mut v = b"IWi".to_vec();
        v.extend_from_slice(&[6, format, flags]);
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&[0; 16]);
        v
    }

    #[test]
    fn dxt1_solid_and_punchthrough() {
        // c0 = pure red (0xF800) > c1 = pure blue (0x001F): all indices 0 -> red.
        let block = [0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0];
        let px = decode_block(Format::Dxt1, &block);
        assert!(px.iter().all(|p| *p == [255, 0, 0, 255]));
        // c0 <= c1 and index 3 -> transparent black.
        let block = [0x1F, 0x00, 0x00, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF];
        let px = decode_block(Format::Dxt1, &block);
        assert!(px.iter().all(|p| *p == [0, 0, 0, 0]));
    }

    #[test]
    fn dxt5_alpha_ramp() {
        // a0 = 255, a1 = 0, index 1 everywhere -> alpha 0; colour white.
        let mut block = [0u8; 16];
        block[0] = 255;
        block[1] = 0;
        // 16 x 3-bit index 1 = 0b001 repeated.
        let bits: u64 = (0..16).fold(0, |acc, i| acc | (1u64 << (i * 3)));
        block[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
        block[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
        block[10..12].copy_from_slice(&0xFFFFu16.to_le_bytes());
        let px = decode_block(Format::Dxt5, &block);
        assert!(px.iter().all(|p| *p == [255, 255, 255, 0]));
    }

    #[test]
    fn dxt3_explicit_alpha() {
        let mut block = [0u8; 16];
        block[0] = 0xF0; // pixel 0 alpha 0, pixel 1 alpha 15
        block[8..10].copy_from_slice(&0x07E0u16.to_le_bytes()); // green
        let px = decode_block(Format::Dxt3, &block);
        assert_eq!(px[0], [0, 255, 0, 0]);
        assert_eq!(px[1], [0, 255, 0, 255]);
    }

    #[test]
    fn parses_mip_chain_largest_first() {
        // 4x4 ARGB with mips 4x4, 2x2, 1x1 stored smallest first.
        let mut v = header(0x01, 0, 4, 4);
        v.extend_from_slice(&[1, 1, 1, 1]); // 1x1
        v.extend_from_slice(&[2; 16]); // 2x2
        v.extend_from_slice(&[0x10, 0x20, 0x30, 0x40].repeat(16)); // 4x4 BGRA
        let iwi = Iwi::parse(&v).unwrap();
        assert_eq!(iwi.levels.len(), 3);
        assert_eq!(iwi.levels[0].len(), 64);
        assert_eq!(iwi.levels[2], vec![1, 1, 1, 1]);
        assert_eq!(&iwi.to_rgba()[..4], &[0x30, 0x20, 0x10, 0x40]);
    }

    #[test]
    fn single_level_and_errors() {
        let mut v = header(0x0B, flags::NO_MIPMAPS, 8, 8);
        v.extend_from_slice(&[0u8; 32]);
        let iwi = Iwi::parse(&v).unwrap();
        assert_eq!(iwi.levels.len(), 1);
        assert_eq!(iwi.to_rgba().len(), 8 * 8 * 4);
        assert_eq!(Iwi::parse(b"nope").unwrap_err(), IwiError::NotIwi);
        assert_eq!(Iwi::parse(&header(0x06, 0, 8, 8)).unwrap_err(), IwiError::UnsupportedFormat(6));
        assert_eq!(Iwi::parse(&header(0x0D, flags::NO_MIPMAPS, 8, 8)).unwrap_err(), IwiError::Truncated);
    }
}
