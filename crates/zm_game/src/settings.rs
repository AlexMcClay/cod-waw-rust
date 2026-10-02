//! Launch configuration (`undead.cfg` next to the executable) and the user's
//! options, persisted to `%LOCALAPPDATA%\UndeadRounds\settings.cfg`.

use bevy::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

/// Per-user data folder (settings, logs).
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("UndeadRounds");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Finds a config file in the working directory, next to the executable, or
/// up to three folders above it (for `cargo run` from `target/`).
pub fn find_config(name: &str) -> Option<PathBuf> {
    let mut bases = vec![PathBuf::from(".")];
    if let Some(e) = exe_dir() {
        bases.extend([e.clone(), e.join(".."), e.join("../.."), e.join("../../..")]);
    }
    bases.into_iter().map(|b| b.join(name)).find(|p| p.is_file())
}

/// `key = value` lines, `#` comments.
pub fn parse_kv(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().trim_matches('"').to_string()))
        .collect()
}

/// Options read once at startup from `undead.cfg`.
#[derive(Resource, Debug, Clone, Default)]
pub struct LaunchConfig {
    pub waw_path: Option<PathBuf>,
}

impl LaunchConfig {
    pub fn load() -> LaunchConfig {
        let mut bases = vec![PathBuf::from(".")];
        if let Some(e) = exe_dir() {
            bases.push(e.clone());
            bases.push(e.join(".."));
            bases.push(e.join("../.."));
        }
        let Some(path) = bases.iter().map(|b| b.join("undead.cfg")).find(|p| p.is_file()) else {
            return LaunchConfig::default();
        };
        let kv = std::fs::read_to_string(&path).map(|t| parse_kv(&t)).unwrap_or_default();
        LaunchConfig {
            waw_path: kv.get("waw_path").filter(|s| !s.is_empty()).map(PathBuf::from),
        }
    }
}

/// Options the player can change in the menu.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct UserSettings {
    pub sensitivity: f32,
    pub fov: f32,
    pub master_volume: f32,
    pub music_volume: f32,
    pub sfx_volume: f32,
    pub exposure_ev: f32,
    pub fullscreen: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        UserSettings {
            sensitivity: 0.0022,
            fov: 72.0,
            master_volume: 0.8,
            music_volume: 0.6,
            sfx_volume: 1.0,
            exposure_ev: 7.5,
            fullscreen: false,
        }
    }
}

impl UserSettings {
    fn path() -> PathBuf {
        data_dir().join("settings.cfg")
    }

    pub fn load() -> UserSettings {
        let mut s = UserSettings::default();
        let Ok(text) = std::fs::read_to_string(Self::path()) else { return s };
        let kv = parse_kv(&text);
        let f = |k: &str, d: f32| kv.get(k).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(d);
        s.sensitivity = f("sensitivity", s.sensitivity).clamp(0.0003, 0.02);
        s.fov = f("fov", s.fov).clamp(55.0, 100.0);
        s.master_volume = f("master_volume", s.master_volume).clamp(0.0, 1.0);
        s.music_volume = f("music_volume", s.music_volume).clamp(0.0, 1.0);
        s.sfx_volume = f("sfx_volume", s.sfx_volume).clamp(0.0, 1.0);
        s.exposure_ev = f("exposure_ev", s.exposure_ev).clamp(3.0, 14.0);
        s.fullscreen = kv.get("fullscreen").is_some_and(|v| v == "1" || v == "true");
        s
    }

    pub fn save(&self) {
        let text = format!(
            "# Undead Rounds settings\nsensitivity = {}\nfov = {}\nmaster_volume = {}\nmusic_volume = {}\nsfx_volume = {}\nexposure_ev = {}\nfullscreen = {}\n",
            self.sensitivity, self.fov, self.master_volume, self.music_volume, self.sfx_volume, self.exposure_ev, self.fullscreen
        );
        if let Err(e) = std::fs::write(Self::path(), text) {
            warn!("could not save settings: {e}");
        }
    }

    /// Effective volume for a sound effect.
    pub fn sfx(&self) -> f32 {
        self.master_volume * self.sfx_volume
    }

    pub fn music(&self) -> f32 {
        self.master_volume * self.music_volume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_parsing() {
        let kv = parse_kv("# c\nwaw_path = \"D:\\Games\\WaW\" # note\n\nFOV=80\nbad line\n");
        assert_eq!(kv.get("waw_path").unwrap(), "D:\\Games\\WaW");
        assert_eq!(kv.get("fov").unwrap(), "80");
        assert_eq!(kv.len(), 2);
    }
}
