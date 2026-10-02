//! A read-only virtual file system over the game's `.iwd` archives.
//!
//! IWDs are ordinary zip files. Archives are indexed in name order and a later
//! archive overrides an earlier one with the same path, which matches the
//! game's own priority (`localized_*` patches `iw_*`, `iw_27` beats `iw_00`).
//! Lookups are case-insensitive and accept either slash direction.

use flate2::read::DeflateDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy)]
struct Entry {
    archive: u16,
    method: u16,
    local_offset: u32,
    comp_size: u32,
    size: u32,
}

struct Archive {
    path: PathBuf,
    file: Mutex<File>,
}

/// All IWD archives of an install, merged into one path index.
pub struct Iwd {
    archives: Vec<Archive>,
    index: HashMap<String, Entry>,
    /// Original-case path for each key, for listings.
    names: HashMap<String, String>,
}

/// Normalises a path to the index key form: lowercase, forward slashes.
pub fn key(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Reads a zip central directory: `(name, entry-without-archive-index)`.
fn read_central_directory(file: &mut File) -> io::Result<Vec<(String, Entry)>> {
    let len = file.seek(SeekFrom::End(0))?;
    let tail_len = len.min(65_557) as usize;
    let mut tail = vec![0u8; tail_len];
    file.seek(SeekFrom::Start(len - tail_len as u64))?;
    file.read_exact(&mut tail)?;
    let eocd = (0..=tail_len.saturating_sub(22))
        .rev()
        .find(|&i| u32_at(&tail, i) == 0x0605_4b50)
        .ok_or_else(|| bad("no end-of-central-directory record"))?;
    let count = u16_at(&tail, eocd + 10) as usize;
    let cd_size = u32_at(&tail, eocd + 12) as usize;
    let cd_offset = u32_at(&tail, eocd + 16) as u64;
    let mut cd = vec![0u8; cd_size];
    file.seek(SeekFrom::Start(cd_offset))?;
    file.read_exact(&mut cd)?;

    let mut out = Vec::with_capacity(count);
    let mut o = 0;
    while o + 46 <= cd.len() && u32_at(&cd, o) == 0x0201_4b50 {
        let method = u16_at(&cd, o + 10);
        let comp_size = u32_at(&cd, o + 20);
        let size = u32_at(&cd, o + 24);
        let name_len = u16_at(&cd, o + 28) as usize;
        let extra_len = u16_at(&cd, o + 30) as usize;
        let comment_len = u16_at(&cd, o + 32) as usize;
        let local_offset = u32_at(&cd, o + 42);
        let name = String::from_utf8_lossy(&cd[o + 46..o + 46 + name_len]).into_owned();
        if !name.ends_with('/') {
            out.push((name, Entry { archive: 0, method, local_offset, comp_size, size }));
        }
        o += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

impl Iwd {
    /// Indexes every `*.iwd` in `main_dir`.
    pub fn open(main_dir: &Path) -> io::Result<Iwd> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(main_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iwd")))
            .collect();
        paths.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
        Self::from_paths(&paths)
    }

    /// Indexes the given archives; later ones override earlier ones.
    pub fn from_paths(paths: &[PathBuf]) -> io::Result<Iwd> {
        let mut iwd = Iwd { archives: Vec::new(), index: HashMap::new(), names: HashMap::new() };
        for p in paths {
            let mut file = File::open(p)?;
            let entries = match read_central_directory(&mut file) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let archive = iwd.archives.len() as u16;
            for (name, mut e) in entries {
                e.archive = archive;
                let k = key(&name);
                iwd.names.insert(k.clone(), name);
                iwd.index.insert(k, e);
            }
            iwd.archives.push(Archive { path: p.clone(), file: Mutex::new(file) });
        }
        Ok(iwd)
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.index.contains_key(&key(path))
    }

    /// Which archive a file comes from (after overrides).
    pub fn archive_of(&self, path: &str) -> Option<&Path> {
        self.index.get(&key(path)).map(|e| self.archives[e.archive as usize].path.as_path())
    }

    /// Original-case paths of every file whose key starts with `prefix`.
    pub fn list(&self, prefix: &str) -> Vec<&str> {
        let p = key(prefix);
        let mut v: Vec<&str> = self.names.iter().filter(|(k, _)| k.starts_with(&p)).map(|(_, n)| n.as_str()).collect();
        v.sort_unstable();
        v
    }

    /// Reads and decompresses one file.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let e = *self.index.get(&key(path))?;
        self.read_entry(e).ok()
    }

    /// `images/<name>.iwi`.
    pub fn read_image(&self, name: &str) -> Option<Vec<u8>> {
        self.read(&format!("images/{name}.iwi"))
    }

    fn read_entry(&self, e: Entry) -> io::Result<Vec<u8>> {
        let archive = &self.archives[e.archive as usize];
        let comp = {
            let mut f = archive.file.lock().map_err(|_| bad("poisoned"))?;
            let mut header = [0u8; 30];
            f.seek(SeekFrom::Start(e.local_offset as u64))?;
            f.read_exact(&mut header)?;
            if u32_at(&header, 0) != 0x0403_4b50 {
                return Err(bad("bad local header"));
            }
            let skip = u16_at(&header, 26) as i64 + u16_at(&header, 28) as i64;
            f.seek(SeekFrom::Current(skip))?;
            let mut comp = vec![0u8; e.comp_size as usize];
            f.read_exact(&mut comp)?;
            comp
        };
        match e.method {
            0 => Ok(comp),
            8 => {
                let mut out = Vec::with_capacity(e.size as usize);
                DeflateDecoder::new(&comp[..]).read_to_end(&mut out)?;
                Ok(out)
            }
            m => Err(bad(&format!("unsupported zip method {m}"))),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    /// Builds a minimal zip with the given (name, data, deflate?) entries.
    pub fn make_zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for (name, data, deflate) in files {
            let comp = if *deflate {
                let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
                enc.write_all(data).unwrap();
                enc.finish().unwrap()
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 8]); // time, date, crc
            out.extend_from_slice(&(comp.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&comp);

            cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            cd.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&[0; 8]);
            cd.extend_from_slice(&(comp.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0; 12]); // extra, comment, disk, int attr, ext attr
            cd.extend_from_slice(&offset.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("waw_assets_test_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_and_overrides() {
        let dir = temp_dir("iwd");
        std::fs::write(dir.join("iw_00.iwd"), make_zip(&[("images/A.iwi", b"old", false), ("sound/x.wav", b"stays", true)])).unwrap();
        let big = vec![7u8; 5000];
        std::fs::write(dir.join("iw_01.iwd"), make_zip(&[("images/a.iwi", &big, true)])).unwrap();
        std::fs::write(dir.join("readme.txt"), b"not an archive").unwrap();
        let iwd = Iwd::open(&dir).unwrap();
        assert_eq!(iwd.len(), 2);
        assert_eq!(iwd.read("IMAGES\\a.iwi").unwrap(), big);
        assert_eq!(iwd.read_image("A").unwrap(), big);
        assert_eq!(iwd.read("sound/X.wav").unwrap(), b"stays");
        assert!(iwd.archive_of("images/a.iwi").unwrap().ends_with("iw_01.iwd"));
        assert_eq!(iwd.list("sound/"), vec!["sound/x.wav"]);
        assert!(iwd.read("missing").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
