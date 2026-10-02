//! Fastfile (`.ff`) containers and the zone they decompress to.
//!
//! A PC World at War fastfile is `"IWffu100"`, a u32 version (387) and one
//! zlib stream. The zone inside starts with a 36-byte header (size, external
//! size, seven block sizes) followed by the asset list: script strings, then
//! one `{type, pointer}` pair per asset, then the asset data itself.

use flate2::read::ZlibDecoder;
use std::io::Read;
use std::path::Path;

pub const FF_VERSION: u32 = 387;
pub const BLOCK_COUNT: usize = 7;
/// Pointer value meaning "the data follows inline in the stream".
pub const INLINE: u32 = 0xFFFF_FFFF;

/// World at War asset types (as stored in the asset list).
pub mod asset_type {
    pub const PHYSPRESET: u32 = 1;
    pub const DESTRUCTIBLEDEF: u32 = 3;
    pub const XANIMPARTS: u32 = 4;
    pub const XMODEL: u32 = 5;
    pub const MATERIAL: u32 = 6;
    pub const TECHNIQUE_SET: u32 = 7;
    pub const SOUND: u32 = 9;
    pub const CLIPMAP: u32 = 11;
    pub const COMWORLD: u32 = 13;
    pub const GAMEWORLD_SP: u32 = 14;
    pub const GFXWORLD: u32 = 17;
    pub const LIGHT_DEF: u32 = 18;
    pub const LOCALIZE_ENTRY: u32 = 23;
    pub const WEAPON: u32 = 24;
    pub const FX: u32 = 26;
    pub const RAWFILE: u32 = 32;

    pub fn name(t: u32) -> &'static str {
        match t {
            0 => "xmodelpieces",
            1 => "physpreset",
            2 => "physconstraints",
            3 => "destructibledef",
            4 => "xanimparts",
            5 => "xmodel",
            6 => "material",
            7 => "techniqueset",
            8 => "image",
            9 => "sound",
            10 => "loaded_sound",
            11 => "clipmap",
            12 => "clipmap_pvs",
            13 => "comworld",
            14 => "gameworld_sp",
            15 => "gameworld_mp",
            16 => "map_ents",
            17 => "gfxworld",
            18 => "lightdef",
            19 => "ui_map",
            20 => "font",
            21 => "menulist",
            22 => "menu",
            23 => "localize",
            24 => "weapon",
            25 => "snddriverglobals",
            26 => "fx",
            27 => "impactfx",
            28 => "aitype",
            29 => "mptype",
            30 => "character",
            31 => "xmodelalias",
            32 => "rawfile",
            33 => "stringtable",
            _ => "unknown",
        }
    }
}

#[derive(Debug)]
pub enum ZoneError {
    Io(std::io::Error),
    BadMagic,
    BadVersion(u32),
    Truncated(&'static str),
}

impl std::fmt::Display for ZoneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZoneError::Io(e) => write!(f, "{e}"),
            ZoneError::BadMagic => write!(f, "not a fastfile"),
            ZoneError::BadVersion(v) => write!(f, "unsupported fastfile version {v}"),
            ZoneError::Truncated(what) => write!(f, "zone truncated reading {what}"),
        }
    }
}

impl std::error::Error for ZoneError {}

impl From<std::io::Error> for ZoneError {
    fn from(e: std::io::Error) -> Self {
        ZoneError::Io(e)
    }
}

/// Decompresses a fastfile into its zone bytes.
pub fn decompress(ff: &[u8]) -> Result<Vec<u8>, ZoneError> {
    if ff.len() < 12 || !(ff.starts_with(b"IWffu100") || ff.starts_with(b"IWff0100")) {
        return Err(ZoneError::BadMagic);
    }
    let version = u32::from_le_bytes([ff[8], ff[9], ff[10], ff[11]]);
    if version != FF_VERSION {
        return Err(ZoneError::BadVersion(version));
    }
    let mut out = Vec::with_capacity(ff.len() * 2);
    ZlibDecoder::new(&ff[12..]).read_to_end(&mut out)?;
    Ok(out)
}

/// A decompressed zone with its asset list parsed.
pub struct Zone {
    pub data: Vec<u8>,
    pub block_sizes: [u32; BLOCK_COUNT],
    /// Script strings (index 0 is the null string).
    pub script_strings: Vec<String>,
    /// Asset type per entry, in load order.
    pub asset_types: Vec<u32>,
    /// Offset where the first asset's data starts.
    pub data_start: usize,
}

impl Zone {
    pub fn load(path: &Path) -> Result<Zone, ZoneError> {
        Zone::parse(decompress(&std::fs::read(path)?)?)
    }

    pub fn parse(data: Vec<u8>) -> Result<Zone, ZoneError> {
        let mut r = Reader::new(&data, 0);
        let _size = r.u32().ok_or(ZoneError::Truncated("header"))?;
        let _external = r.u32().ok_or(ZoneError::Truncated("header"))?;
        let mut block_sizes = [0u32; BLOCK_COUNT];
        for b in &mut block_sizes {
            *b = r.u32().ok_or(ZoneError::Truncated("header"))?;
        }
        let string_count = r.u32().ok_or(ZoneError::Truncated("asset list"))? as usize;
        let _strings_ptr = r.u32();
        let asset_count = r.u32().ok_or(ZoneError::Truncated("asset list"))? as usize;
        let _assets_ptr = r.u32();
        let mut inline = Vec::with_capacity(string_count);
        for _ in 0..string_count {
            inline.push(r.u32().ok_or(ZoneError::Truncated("script string table"))? == INLINE);
        }
        let mut script_strings = Vec::with_capacity(string_count);
        for is_inline in inline {
            script_strings.push(if is_inline {
                r.cstr().ok_or(ZoneError::Truncated("script strings"))?.to_string()
            } else {
                String::new()
            });
        }
        let mut asset_types = Vec::with_capacity(asset_count);
        for _ in 0..asset_count {
            let t = r.u32().ok_or(ZoneError::Truncated("asset table"))?;
            let _ptr = r.u32();
            asset_types.push(t);
        }
        let data_start = r.pos;
        Ok(Zone { data, block_sizes, script_strings, asset_types, data_start })
    }

    pub fn reader(&self, pos: usize) -> Reader<'_> {
        Reader::new(&self.data, pos)
    }

    /// Script string by index (empty for 0 or out of range).
    pub fn script_string(&self, i: usize) -> &str {
        self.script_strings.get(i).map(String::as_str).unwrap_or("")
    }

    /// Offsets of every occurrence of `needle`.
    pub fn find_all(&self, needle: &[u8]) -> Vec<usize> {
        find_all(&self.data, needle)
    }
}

pub fn find_all(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return Vec::new();
    }
    let first = needle[0];
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        match hay[i..hay.len() - needle.len() + 1].iter().position(|&b| b == first) {
            Some(p) => {
                i += p;
                if &hay[i..i + needle.len()] == needle {
                    out.push(i);
                }
                i += 1;
            }
            None => break,
        }
    }
    out
}

/// Little-endian cursor over zone bytes.
#[derive(Clone)]
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], pos: usize) -> Self {
        Reader { data, pos }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.data.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }

    pub fn skip(&mut self, n: usize) -> Option<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn align(&mut self, a: usize) {
        self.pos = (self.pos + a - 1) / a * a;
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.bytes(1).map(|b| b[0])
    }
    pub fn u16(&mut self) -> Option<u16> {
        self.bytes(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }
    pub fn u32(&mut self) -> Option<u32> {
        self.bytes(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    pub fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
    pub fn vec3(&mut self) -> Option<[f32; 3]> {
        Some([self.f32()?, self.f32()?, self.f32()?])
    }

    /// Null-terminated string (Latin-1 bytes outside ASCII are replaced).
    pub fn cstr(&mut self) -> Option<&'a str> {
        let rest = self.data.get(self.pos..)?;
        let end = rest.iter().position(|&b| b == 0)?;
        self.pos += end + 1;
        std::str::from_utf8(&rest[..end]).ok().or(Some(""))
    }

    pub fn peek_u32(&self) -> Option<u32> {
        self.clone().u32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn synthetic_zone() -> Vec<u8> {
        let mut z = Vec::new();
        let push = |z: &mut Vec<u8>, v: u32| z.extend_from_slice(&v.to_le_bytes());
        push(&mut z, 0);
        push(&mut z, 0);
        for b in 0..7 {
            push(&mut z, b * 16);
        }
        push(&mut z, 3); // script strings
        push(&mut z, INLINE);
        push(&mut z, 2); // assets
        push(&mut z, INLINE);
        push(&mut z, 0); // string 0 is null
        push(&mut z, INLINE);
        push(&mut z, INLINE);
        z.extend_from_slice(b"tag_view\0j_head\0");
        push(&mut z, asset_type::RAWFILE);
        push(&mut z, INLINE);
        push(&mut z, asset_type::XMODEL);
        push(&mut z, INLINE);
        z.extend_from_slice(b"DATA");
        z
    }

    #[test]
    fn parses_asset_list() {
        let zone = Zone::parse(synthetic_zone()).unwrap();
        assert_eq!(zone.block_sizes[2], 32);
        assert_eq!(zone.script_strings, vec!["", "tag_view", "j_head"]);
        assert_eq!(zone.asset_types, vec![asset_type::RAWFILE, asset_type::XMODEL]);
        assert_eq!(&zone.data[zone.data_start..], b"DATA");
    }

    #[test]
    fn decompresses_fastfile() {
        let zone = synthetic_zone();
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&zone).unwrap();
        let mut ff = b"IWffu100".to_vec();
        ff.extend_from_slice(&FF_VERSION.to_le_bytes());
        ff.extend_from_slice(&enc.finish().unwrap());
        assert_eq!(decompress(&ff).unwrap(), zone);
        assert!(matches!(decompress(b"PK\x03\x04garbage"), Err(ZoneError::BadMagic)));
    }

    #[test]
    fn finds_patterns() {
        assert_eq!(find_all(b"abcabcab", b"abc"), vec![0, 3]);
        assert_eq!(find_all(b"aaa", b"aa"), vec![0, 1]);
        assert!(find_all(b"ab", b"abc").is_empty());
    }

    /// Real-data check: `UNDEAD_WAW=<install> cargo test -p waw_assets -- --ignored`.
    #[test]
    #[ignore]
    fn real_nacht_zone() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let zone = Zone::load(&Path::new(&root).join("zone/english/nazi_zombie_prototype.ff")).unwrap();
        assert_eq!(zone.asset_types.len(), 3140);
        assert_eq!(zone.script_strings.len(), 552);
        assert_eq!(zone.script_string(1), "tag_view");
        assert_eq!(zone.asset_types.iter().filter(|&&t| t == asset_type::XMODEL).count(), 288);
        assert_eq!(zone.asset_types.iter().filter(|&&t| t == asset_type::GFXWORLD).count(), 1);
    }
}
