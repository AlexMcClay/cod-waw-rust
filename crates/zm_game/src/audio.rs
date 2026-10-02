//! Sound loading and playback.
//!
//! Sounds are mapped to events through `sounds.cfg` (searched in the working
//! directory and next to the executable). Each line is
//! `event = path.wav | other.wav`. A path starting with `iwd/` is read from
//! the World at War install's IWD archives (falling back to the extracted
//! folder from `tools/extract_zombies.py`); other paths are relative to the
//! extracted folder. Anything missing falls back to a procedurally generated
//! sound, so the game always runs.

use bevy::audio::Volume;
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::settings::UserSettings;
use crate::waw::Waw;
use zm_core::wav;
use zm_core::weaponfile::WeaponFile;
use zm_core::weapons::{Kind, WeaponDef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    ShotPistol,
    ShotRifle,
    ShotSmg,
    ShotShotgun,
    ShotLmg,
    ShotWonder,
    Reload,
    DryFire,
    Hit,
    Headshot,
    Knife,
    ZombieGroan,
    ZombieAttack,
    ZombieDeath,
    ZombieSpawn,
    BoardTear,
    BoardRepair,
    Purchase,
    WallBuy,
    Deny,
    DoorOpen,
    CrateOpen,
    CrateReady,
    RoundStart,
    RoundEnd,
    GameStart,
    GameOver,
    PlayerHurt,
    PowerupSpawn,
    PowerupGrab,
    MaxAmmo,
    InstaKill,
    DoublePoints,
    Nuke,
    Carpenter,
}

impl Sfx {
    pub const ALL: [Sfx; 35] = [
        Sfx::ShotPistol, Sfx::ShotRifle, Sfx::ShotSmg, Sfx::ShotShotgun, Sfx::ShotLmg,
        Sfx::ShotWonder, Sfx::Reload, Sfx::DryFire, Sfx::Hit, Sfx::Headshot, Sfx::Knife,
        Sfx::ZombieGroan, Sfx::ZombieAttack, Sfx::ZombieDeath, Sfx::ZombieSpawn, Sfx::BoardTear,
        Sfx::BoardRepair, Sfx::Purchase, Sfx::WallBuy, Sfx::Deny, Sfx::DoorOpen, Sfx::CrateOpen,
        Sfx::CrateReady, Sfx::RoundStart, Sfx::RoundEnd, Sfx::GameStart, Sfx::GameOver,
        Sfx::PlayerHurt, Sfx::PowerupSpawn, Sfx::PowerupGrab, Sfx::MaxAmmo, Sfx::InstaKill,
        Sfx::DoublePoints, Sfx::Nuke, Sfx::Carpenter,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Sfx::ShotPistol => "shot_pistol",
            Sfx::ShotRifle => "shot_rifle",
            Sfx::ShotSmg => "shot_smg",
            Sfx::ShotShotgun => "shot_shotgun",
            Sfx::ShotLmg => "shot_lmg",
            Sfx::ShotWonder => "shot_wonder",
            Sfx::Reload => "reload",
            Sfx::DryFire => "dry_fire",
            Sfx::Hit => "hit",
            Sfx::Headshot => "headshot",
            Sfx::Knife => "knife",
            Sfx::ZombieGroan => "zombie_groan",
            Sfx::ZombieAttack => "zombie_attack",
            Sfx::ZombieDeath => "zombie_death",
            Sfx::ZombieSpawn => "zombie_spawn",
            Sfx::BoardTear => "board_tear",
            Sfx::BoardRepair => "board_repair",
            Sfx::Purchase => "purchase",
            Sfx::WallBuy => "wall_buy",
            Sfx::Deny => "deny",
            Sfx::DoorOpen => "door_open",
            Sfx::CrateOpen => "crate_open",
            Sfx::CrateReady => "crate_ready",
            Sfx::RoundStart => "round_start",
            Sfx::RoundEnd => "round_end",
            Sfx::GameStart => "game_start",
            Sfx::GameOver => "game_over",
            Sfx::PlayerHurt => "player_hurt",
            Sfx::PowerupSpawn => "powerup_spawn",
            Sfx::PowerupGrab => "powerup_grab",
            Sfx::MaxAmmo => "max_ammo",
            Sfx::InstaKill => "insta_kill",
            Sfx::DoublePoints => "double_points",
            Sfx::Nuke => "nuke",
            Sfx::Carpenter => "carpenter",
        }
    }

    pub fn for_weapon(kind: Kind) -> Sfx {
        match kind {
            Kind::Pistol => Sfx::ShotPistol,
            Kind::Rifle => Sfx::ShotRifle,
            Kind::Smg => Sfx::ShotSmg,
            Kind::Shotgun => Sfx::ShotShotgun,
            Kind::Lmg => Sfx::ShotLmg,
            Kind::Wonder => Sfx::ShotWonder,
        }
    }

    /// Procedural stand-in used when no file is mapped.
    fn synth(self, variant: u32) -> Option<wav::Pcm> {
        use wav::{synth_groan, synth_shot, synth_tone};
        let seed = 0x9E37_79B9u32.wrapping_mul(variant + 1);
        Some(match self {
            Sfx::ShotPistol => synth_shot(0.25, seed),
            Sfx::ShotRifle => synth_shot(0.75, seed),
            Sfx::ShotSmg => synth_shot(0.4, seed),
            Sfx::ShotShotgun => synth_shot(1.0, seed),
            Sfx::ShotLmg => synth_shot(0.6, seed),
            Sfx::ShotWonder => synth_tone(1400.0, 300.0, 0.35, true),
            Sfx::Reload => synth_tone(300.0, 260.0, 0.08, true),
            Sfx::DryFire => synth_tone(900.0, 900.0, 0.03, true),
            Sfx::Hit => synth_tone(1800.0, 1600.0, 0.04, false),
            Sfx::Headshot => synth_tone(2400.0, 2000.0, 0.07, false),
            Sfx::Knife => synth_shot(0.05, seed),
            Sfx::ZombieGroan => synth_groan(70.0 + variant as f32 * 9.0, 1.1 + variant as f32 * 0.2, seed),
            Sfx::ZombieAttack => synth_groan(140.0, 0.45, seed),
            Sfx::ZombieDeath => synth_groan(55.0, 0.9, seed),
            Sfx::ZombieSpawn => synth_shot(0.9, seed),
            Sfx::BoardTear => synth_shot(0.55, seed),
            Sfx::BoardRepair => synth_tone(220.0, 180.0, 0.12, true),
            Sfx::Purchase | Sfx::WallBuy => synth_tone(600.0, 1200.0, 0.25, false),
            Sfx::Deny => synth_tone(200.0, 140.0, 0.3, true),
            Sfx::DoorOpen => synth_shot(1.0, seed),
            Sfx::CrateOpen => synth_tone(400.0, 1600.0, 1.2, false),
            Sfx::CrateReady => synth_tone(900.0, 1300.0, 0.4, false),
            Sfx::RoundStart => synth_tone(110.0, 82.0, 2.5, false),
            Sfx::RoundEnd => synth_tone(165.0, 110.0, 2.0, false),
            Sfx::GameStart => synth_tone(82.0, 110.0, 2.0, false),
            Sfx::GameOver => synth_tone(110.0, 55.0, 3.0, false),
            Sfx::PlayerHurt => synth_groan(220.0, 0.25, seed),
            Sfx::PowerupSpawn => synth_tone(500.0, 900.0, 0.5, false),
            Sfx::PowerupGrab => synth_tone(700.0, 1400.0, 0.4, false),
            Sfx::MaxAmmo | Sfx::InstaKill | Sfx::DoublePoints | Sfx::Nuke | Sfx::Carpenter => return None,
        })
    }
}

/// Where the user's extracted files live.
#[derive(Resource, Clone)]
pub struct AssetDir {
    pub root: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub weapon_overrides: usize,
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

impl AssetDir {
    /// `--assets <dir>`, then `UNDEAD_ASSETS`, then common relative spots.
    pub fn locate() -> AssetDir {
        let args: Vec<String> = std::env::args().collect();
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(i) = args.iter().position(|a| a == "--assets") {
            if let Some(p) = args.get(i + 1) {
                candidates.push(PathBuf::from(p));
            }
        }
        if let Ok(p) = std::env::var("UNDEAD_ASSETS") {
            candidates.push(PathBuf::from(p));
        }
        let mut bases = vec![PathBuf::from(".")];
        if let Some(e) = exe_dir() {
            bases.push(e);
        }
        for b in &bases {
            for rel in ["extracted", "../extracted", "../../extracted", "../../../extracted", "../../../../extracted"] {
                candidates.push(b.join(rel));
            }
        }
        let root = candidates.into_iter().find(|p| p.join("iwd").is_dir() || p.join("fastfiles").is_dir());
        let config = bases
            .iter()
            .flat_map(|b| [b.join("sounds.cfg"), b.join("../sounds.cfg"), b.join("../../sounds.cfg"), b.join("../../../sounds.cfg")])
            .find(|p| p.is_file());
        match &root {
            Some(r) => info!("Using extracted assets at {}", r.display()),
            None => warn!("No extracted assets found (pass --assets <dir>); using built-in sounds"),
        }
        AssetDir { root, config, weapon_overrides: 0 }
    }

    pub fn with_override_count(mut self, n: usize) -> Self {
        self.weapon_overrides = n;
        self
    }

    /// Override built-in weapon stats with the game's weapon files: from the
    /// install's IWD archives, else from the extracted folder.
    pub fn apply_weapon_files(&self, waw: &Waw, defs: &mut [WeaponDef]) -> usize {
        let mut n = 0;
        for d in defs.iter_mut() {
            let Some(file) = d.weapon_file else { continue };
            let bytes = waw
                .read(&format!("weapons/sp/{file}"))
                .or_else(|| self.root.as_ref().and_then(|r| std::fs::read(r.join("iwd/weapons/sp").join(file)).ok()));
            if let Some(wf) = bytes.and_then(|b| WeaponFile::parse_bytes(&b)) {
                let applied = d.apply_weapon_file(&wf);
                info!("{}: applied {applied} stats from weapons/sp/{file}", d.name);
                n += 1;
            }
        }
        n
    }
}

#[derive(Resource, Default)]
pub struct SoundBank {
    pub sounds: HashMap<Sfx, Vec<Handle<AudioSource>>>,
    pub loaded_from_disk: usize,
    /// `alias:name` entries from `sounds.cfg`, resolved from the zones.
    pub aliases: HashMap<Sfx, Vec<String>>,
}

impl SoundBank {
    /// Every zone sound alias the config asks for.
    pub fn wanted_aliases(&self) -> Vec<String> {
        let mut v: Vec<String> = self.aliases.values().flatten().cloned().collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Sounds decoded from the install's fastfiles (filled once a zone is read).
#[derive(Resource, Default)]
pub struct ZoneSounds {
    /// Sound alias name -> variants.
    pub aliases: HashMap<String, Vec<Handle<AudioSource>>>,
    /// Our weapon id -> its real fire sound variants.
    pub weapon_fire: HashMap<String, Vec<Handle<AudioSource>>>,
}

/// Request to play a sound. `volume` is linear 0..1.
#[derive(Event, Clone, Copy)]
pub struct PlaySfx {
    pub sfx: Sfx,
    pub volume: f32,
    /// For weapon shots: the weapon id, to use that weapon's own sound.
    pub weapon: Option<&'static str>,
}

impl PlaySfx {
    pub fn new(sfx: Sfx) -> Self {
        PlaySfx { sfx, volume: 1.0, weapon: None }
    }
    pub fn at(sfx: Sfx, volume: f32) -> Self {
        PlaySfx { sfx, volume, weapon: None }
    }
    pub fn weapon(sfx: Sfx, id: &'static str) -> Self {
        PlaySfx { sfx, volume: 1.0, weapon: Some(id) }
    }
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<PlaySfx>()
            .init_resource::<ZoneSounds>()
            .add_systems(PreStartup, load_sounds)
            .add_systems(PostUpdate, play_sounds);
    }
}

fn parse_config(text: &str) -> HashMap<String, Vec<String>> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| {
            (
                k.trim().to_ascii_lowercase(),
                v.split('|').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            )
        })
        .collect()
}

/// Reads a configured sound: IWD archives first for `iwd/...`, then disk.
fn read_sound(dir: &AssetDir, waw: &Waw, rel: &str) -> Result<Vec<u8>, String> {
    if let Some(inner) = rel.strip_prefix("iwd/") {
        if let Some(bytes) = waw.read(inner) {
            return Ok(bytes);
        }
    }
    let root = dir.root.as_ref().ok_or_else(|| "not in the install and no extracted folder".to_string())?;
    std::fs::read(root.join(rel)).map_err(|e| e.to_string())
}

fn load_sounds(dir: Res<AssetDir>, waw: Res<Waw>, mut sources: ResMut<Assets<AudioSource>>, mut commands: Commands) {
    let config = dir
        .config
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| parse_config(&t))
        .unwrap_or_default();
    let mut bank = SoundBank::default();
    for sfx in Sfx::ALL {
        let mut handles = Vec::new();
        if let Some(paths) = config.get(sfx.key()) {
            let aliases: Vec<String> = paths.iter().filter_map(|p| p.strip_prefix("alias:")).map(|a| a.trim().to_string()).collect();
            if !aliases.is_empty() {
                bank.aliases.insert(sfx, aliases);
            }
            for rel in paths.iter().filter(|p| !p.starts_with("alias:")) {
                match read_sound(&dir, &waw, rel).and_then(|b| wav::to_pcm_wav(&b).map_err(|e| e.to_string())) {
                    Ok(bytes) => {
                        handles.push(sources.add(AudioSource { bytes: bytes.into() }));
                        bank.loaded_from_disk += 1;
                    }
                    Err(e) => warn!("sound '{}' -> {rel}: {e}", sfx.key()),
                }
            }
        }
        if handles.is_empty() {
            let variants = if sfx == Sfx::ZombieGroan { 4 } else { 1 };
            for v in 0..variants {
                if let Some(pcm) = sfx.synth(v) {
                    handles.push(sources.add(AudioSource { bytes: pcm.to_wav_bytes().into() }));
                }
            }
        }
        bank.sounds.insert(sfx, handles);
    }
    info!("Loaded {} sounds from disk", bank.loaded_from_disk);
    commands.insert_resource(bank);
}

fn play_sounds(
    mut events: EventReader<PlaySfx>,
    bank: Option<Res<SoundBank>>,
    zone: Res<ZoneSounds>,
    settings: Res<UserSettings>,
    mut commands: Commands,
) {
    let Some(bank) = bank else { return };
    // Collapse duplicates within a frame (e.g. a shotgun hitting 6 zombies).
    let mut played: Vec<(Sfx, Option<&'static str>)> = Vec::new();
    for ev in events.read() {
        if played.contains(&(ev.sfx, ev.weapon)) || ev.volume <= 0.01 {
            continue;
        }
        played.push((ev.sfx, ev.weapon));
        // The weapon's own sound, then a configured zone alias, then files/synth.
        let from_weapon = ev.weapon.and_then(|w| zone.weapon_fire.get(w)).filter(|l| !l.is_empty());
        let from_alias = || {
            let names = bank.aliases.get(&ev.sfx)?;
            let name = &names[fastrand::usize(..names.len())];
            zone.aliases.get(name).filter(|l| !l.is_empty())
        };
        let list = match from_weapon.or_else(from_alias).or_else(|| bank.sounds.get(&ev.sfx)) {
            Some(l) if !l.is_empty() => l,
            _ => continue,
        };
        let h = list[fastrand::usize(..list.len())].clone();
        commands.spawn((
            AudioPlayer::new(h),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(ev.volume * settings.sfx())),
            crate::Dynamic,
        ));
    }
}
