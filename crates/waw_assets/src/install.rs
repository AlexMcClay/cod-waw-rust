//! Finding the World at War install directory.

use std::path::{Path, PathBuf};

/// A validated World at War install.
#[derive(Debug, Clone)]
pub struct Install {
    pub root: PathBuf,
}

/// Folder name of the game inside a Steam library.
const STEAM_DIR: &str = "steamapps/common/Call of Duty World at War";

impl Install {
    /// True if `dir` looks like a WaW install (has the Zombies fastfile).
    pub fn is_install(dir: &Path) -> bool {
        dir.join("zone/english/nazi_zombie_prototype.ff").is_file() && dir.join("main").is_dir()
    }

    /// Checks the explicit candidates first (in order), then Steam libraries
    /// and a few common install locations.
    pub fn locate(explicit: &[PathBuf]) -> Result<Install, String> {
        let mut tried = Vec::new();
        let mut candidates: Vec<PathBuf> = explicit.to_vec();
        for lib in steam_libraries() {
            candidates.push(lib.join(STEAM_DIR));
        }
        for drive in ["C", "D", "E", "F", "G"] {
            candidates.push(PathBuf::from(format!("{drive}:/SteamLibrary/{STEAM_DIR}")));
            candidates.push(PathBuf::from(format!("{drive}:/Program Files (x86)/Steam/{STEAM_DIR}")));
            candidates.push(PathBuf::from(format!("{drive}:/Program Files (x86)/Activision/Call of Duty - World at War")));
            candidates.push(PathBuf::from(format!("{drive}:/Games/Call of Duty World at War")));
        }
        for c in candidates {
            if Self::is_install(&c) {
                return Ok(Install { root: c });
            }
            tried.push(c.display().to_string());
        }
        Err(format!("World at War install not found (tried {} locations)", tried.len()))
    }

    pub fn main_dir(&self) -> PathBuf {
        self.root.join("main")
    }

    /// Path of `zone/english/<name>.ff`.
    pub fn fastfile(&self, name: &str) -> PathBuf {
        self.root.join("zone/english").join(format!("{name}.ff"))
    }
}

/// Every Steam library folder on this machine (best effort, Windows only).
pub fn steam_libraries() -> Vec<PathBuf> {
    let Some(steam) = steam_path() else { return Vec::new() };
    let mut libs = vec![steam.clone()];
    if let Ok(text) = std::fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
        for p in parse_library_folders(&text) {
            if !libs.contains(&p) {
                libs.push(p);
            }
        }
    }
    libs
}

/// Extracts the `"path"` values from Steam's `libraryfolders.vdf`.
pub fn parse_library_folders(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|l| {
            let mut parts = l.split('"').filter(|s| !s.trim().is_empty());
            match (parts.next(), parts.next()) {
                (Some(k), Some(v)) if k.eq_ignore_ascii_case("path") => Some(PathBuf::from(v.replace("\\\\", "\\"))),
                _ => None,
            }
        })
        .collect()
}

#[cfg(windows)]
fn steam_path() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    for key in ["HKCU\\Software\\Valve\\Steam", "HKLM\\SOFTWARE\\WOW6432Node\\Valve\\Steam"] {
        let value = if key.starts_with("HKCU") { "SteamPath" } else { "InstallPath" };
        let out = std::process::Command::new("reg")
            .args(["query", key, "/v", value])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(line) = text.lines().find(|l| l.contains("REG_SZ")) {
            if let Some(p) = line.split("REG_SZ").nth(1) {
                let p = PathBuf::from(p.trim());
                if p.is_dir() {
                    return Some(p);
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn steam_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let p = PathBuf::from(home).join(".steam/steam");
    p.is_dir().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_folders() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}"#;
        let libs = parse_library_folders(vdf);
        assert_eq!(libs, vec![PathBuf::from("C:\\Program Files (x86)\\Steam"), PathBuf::from("D:\\SteamLibrary")]);
    }
}
