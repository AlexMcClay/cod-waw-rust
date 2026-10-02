//! Full sequential reader for World at War (T4, PC) fastfile zones.
//!
//! A zone is a depth-first serialisation of every asset. To resolve the
//! back-references that let assets share data (a material used by both a
//! model and the map, vertex arrays shared between LODs, ...) every asset has
//! to be walked in order while emulating the game's memory blocks exactly, so
//! this module knows the load order of every asset type that occurs before the
//! data we want. The layouts and load order follow the zone definitions of
//! OpenAssetTools (GPL-3.0, https://github.com/Laupetin/OpenAssetTools).
//!
//! The walk produces a [`ZoneData`] with the things the game uses: images,
//! materials, models, the map (GfxWorld), sound aliases, loaded sounds,
//! weapons, map entities and localized strings. Large arrays (vertices,
//! indices, sound data) are kept as offsets into the zone bytes and decoded on
//! demand by [`decode`].

pub mod anim;
pub mod decode;
mod walker;

pub use walker::walk;

/// Asset types (the numeric ids used in the zone's asset table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetType {
    PhysPreset,
    PhysConstraints,
    DestructibleDef,
    XAnim,
    XModel,
    Material,
    TechniqueSet,
    Image,
    Sound,
    LoadedSound,
    ClipMap,
    ComWorld,
    GameWorldSp,
    MapEnts,
    GfxWorld,
    LightDef,
    Localize,
    Weapon,
    Fx,
    ImpactFx,
    RawFile,
    StringTable,
}

impl AssetType {
    pub fn from_id(id: u32) -> Option<AssetType> {
        use AssetType::*;
        Some(match id {
            1 => PhysPreset,
            2 => PhysConstraints,
            3 => DestructibleDef,
            4 => XAnim,
            5 => XModel,
            6 => Material,
            7 => TechniqueSet,
            8 => Image,
            9 => Sound,
            10 => LoadedSound,
            11 | 12 => ClipMap,
            13 => ComWorld,
            14 => GameWorldSp,
            16 => MapEnts,
            17 => GfxWorld,
            18 => LightDef,
            23 => Localize,
            24 => Weapon,
            26 => Fx,
            27 => ImpactFx,
            32 => RawFile,
            33 => StringTable,
            _ => return None,
        })
    }
}

/// A loaded asset, by kind and index into the matching [`ZoneData`] list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetRef {
    Image(u32),
    Material(u32),
    XModel(u32),
    Sound(u32),
    LoadedSound(u32),
    Weapon(u32),
    TechSet(u32),
    Other(AssetType),
    /// An image's pixel-data header (target of -2 pointer slots).
    LoadDef(u32),
}

/// Pixels stored in the zone itself (lightmaps, probes); most images are
/// streamed from `images/<name>.iwi` in the IWDs instead.
#[derive(Debug, Clone)]
pub struct InlinePixels {
    pub fpos: usize,
    pub len: usize,
    /// D3D format or FourCC (`'DXT1'` = 0x31545844, ...).
    pub format: u32,
    pub levels: u8,
    pub dims: [u16; 3],
}

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub name: String,
    pub map_type: u32,
    pub width: u16,
    pub height: u16,
    pub inline: Option<InlinePixels>,
}

/// Sampler-name hash of the diffuse texture slot (`"colorMap"`).
pub const COLOR_MAP: u32 = 0xa0ab_1041;
pub const NORMAL_MAP: u32 = 0x59d3_0d0f;

/// The game's sampler-name hash.
pub fn sampler_hash(name: &str) -> u32 {
    name.bytes().fold(0u32, |h, c| (c as u32 | 0x20) ^ h.wrapping_mul(33))
}

#[derive(Debug, Clone, Copy)]
pub struct TexDef {
    pub name_hash: u32,
    pub semantic: u8,
    pub image: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct MaterialInfo {
    pub name: String,
    pub techset: Option<String>,
    pub textures: Vec<TexDef>,
    pub sort_key: u8,
}

impl MaterialInfo {
    pub fn texture(&self, hash: u32) -> Option<u32> {
        self.textures.iter().find(|t| t.name_hash == hash).and_then(|t| t.image)
    }
    pub fn color_map(&self) -> Option<u32> {
        self.texture(COLOR_MAP)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RigidVertList {
    pub bone_offset: u16,
    pub vert_count: u16,
    pub tri_offset: u16,
    pub tri_count: u16,
}

#[derive(Debug, Clone)]
pub struct XSurfInfo {
    pub vert_count: u16,
    pub tri_count: u16,
    /// Vertices with 1, 2, 3 and 4 bone influences.
    pub blend_counts: [i16; 4],
    pub blend_fpos: Option<usize>,
    pub verts_fpos: Option<usize>,
    pub tris_fpos: Option<usize>,
    pub rigid: Vec<RigidVertList>,
}

#[derive(Debug, Clone, Copy)]
pub struct LodInfo {
    pub dist: f32,
    pub num_surfs: u16,
    pub surf_index: u16,
}

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    /// Index of the parent bone (`None` for roots).
    pub parent: Option<usize>,
    /// Local rotation (x, y, z, w) and translation relative to the parent.
    pub local_quat: [f32; 4],
    pub local_trans: [f32; 3],
    /// Bind pose in model space.
    pub base_quat: [f32; 4],
    pub base_trans: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct XModelInfo {
    pub name: String,
    pub bones: Vec<Bone>,
    pub surfs: Vec<XSurfInfo>,
    pub materials: Vec<Option<u32>>,
    pub lods: Vec<LodInfo>,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

impl XModelInfo {
    /// Surface indices of a LOD.
    pub fn lod_surfs(&self, lod: usize) -> std::ops::Range<usize> {
        match self.lods.get(lod) {
            Some(l) => l.surf_index as usize..(l.surf_index + l.num_surfs) as usize,
            None => 0..0,
        }
    }
}

/// A raw data stream of an animation: file position and element count.
#[derive(Debug, Clone, Copy, Default)]
pub struct Stream {
    pub fpos: Option<usize>,
    pub count: usize,
}

/// Root-motion translation of an animation (`XAnimPartTrans`).
#[derive(Debug, Clone)]
pub struct DeltaTrans {
    /// Number of keys minus one (0 = a single constant `frame0`).
    pub size: u16,
    pub small: bool,
    /// File position of the whole variable-size struct (header first).
    pub fpos: usize,
    /// File position of the key values (when `size > 0`).
    pub frames_fpos: Option<usize>,
}

/// Root-motion rotation of an animation (`XAnimDeltaPartQuat`).
#[derive(Debug, Clone)]
pub struct DeltaQuat {
    pub size: u16,
    pub fpos: usize,
    pub frames_fpos: Option<usize>,
}

/// An animation's header and the positions of its data streams; the
/// keyframes are decoded on demand.
#[derive(Debug, Clone)]
pub struct XAnimInfo {
    pub name: String,
    pub numframes: u16,
    pub framerate: f32,
    pub frequency: f32,
    pub looping: bool,
    pub delta: bool,
    /// Cumulative bone counts per part type (PART_TYPE_COUNT = 10).
    pub bone_counts: [u8; 10],
    pub bones: Vec<String>,
    pub data_byte: Stream,
    pub data_short: Stream,
    pub data_int: Stream,
    pub random_data_short: Stream,
    pub random_data_byte: Stream,
    pub random_data_int: Stream,
    /// Frame indices (u8 when `numframes < 256`, else u16).
    pub indices: Stream,
    pub notify: Vec<(String, f32)>,
    pub delta_trans: Option<DeltaTrans>,
    pub delta_quat: Option<DeltaQuat>,
}

#[derive(Debug, Clone)]
pub struct WorldSurface {
    pub first_vertex: u32,
    pub vertex_count: u16,
    pub tri_count: u16,
    pub base_index: u32,
    pub vertex_layer_data: u32,
    pub material: Option<u32>,
    pub lightmap: u8,
    pub flags: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct BrushModel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub surface_count: u32,
    pub start_surface: u32,
}

#[derive(Debug, Clone)]
pub struct StaticModel {
    pub origin: [f32; 3],
    /// Rows are the model's local X, Y and Z axes in world space.
    pub axis: [[f32; 3]; 3],
    pub scale: f32,
    pub model: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct WorldInfo {
    pub name: String,
    pub vertex_count: u32,
    pub vertices_fpos: Option<usize>,
    pub index_count: u32,
    pub indices_fpos: Option<usize>,
    pub layer_data_fpos: Option<usize>,
    pub surfaces: Vec<WorldSurface>,
    pub static_surface_count: u32,
    /// Surface index ranges: lit (opaque), decal (blended), emissive.
    pub lit_range: std::ops::Range<u32>,
    pub decal_range: std::ops::Range<u32>,
    pub emissive_range: std::ops::Range<u32>,
    pub models: Vec<BrushModel>,
    pub smodels: Vec<StaticModel>,
    pub sky_image: Option<u32>,
    pub sky_box_model: Option<String>,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub sun_color: [f32; 3],
}

#[derive(Debug, Clone)]
pub enum SoundFile {
    None,
    /// Inline RIFF data (index into [`ZoneData::loaded_sounds`]).
    Loaded(u32),
    /// Streamed from `sound/<dir>/<name>` in the IWDs.
    Streamed { dir: String, name: String },
}

#[derive(Debug, Clone)]
pub struct SoundAlias {
    pub name: String,
    pub file: SoundFile,
    pub vol_min: f32,
    pub vol_max: f32,
    pub pitch_min: f32,
    pub pitch_max: f32,
    pub dist_min: f32,
    pub dist_max: f32,
    pub flags: i32,
}

/// One alias list: the variants the game picks between.
#[derive(Debug, Clone)]
pub struct SoundList {
    pub name: String,
    pub aliases: Vec<SoundAlias>,
}

#[derive(Debug, Clone)]
pub struct LoadedSoundInfo {
    pub name: String,
    pub fpos: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Default)]
pub struct WeaponInfo {
    pub name: String,
    pub display_name: String,
    pub view_model: Option<u32>,
    pub hand_model: Option<u32>,
    pub world_model: Option<u32>,
    /// `(field, alias name)` for every sound the weapon references.
    pub sounds: Vec<(&'static str, String)>,
    pub xanims: Vec<String>,
}

impl WeaponInfo {
    pub fn sound(&self, field: &str) -> Option<&str> {
        self.sounds.iter().find(|(f, _)| *f == field).map(|(_, s)| s.as_str())
    }
}

/// Everything the walk extracted from one zone.
#[derive(Debug, Default)]
pub struct ZoneData {
    pub data: Vec<u8>,
    pub script_strings: Vec<String>,
    /// `(type, name)` of every asset in load order (including inline ones).
    pub assets: Vec<(AssetType, String)>,
    pub images: Vec<ImageInfo>,
    pub materials: Vec<MaterialInfo>,
    pub xmodels: Vec<XModelInfo>,
    pub sounds: Vec<SoundList>,
    pub loaded_sounds: Vec<LoadedSoundInfo>,
    pub weapons: Vec<WeaponInfo>,
    pub xanims: Vec<XAnimInfo>,
    pub world: Option<WorldInfo>,
    pub map_ents: Option<String>,
    pub localize: Vec<(String, String)>,
    /// Back-references that could not be resolved (0 for a correct walk).
    pub unresolved: usize,
    /// Final stream position and VIRTUAL block size (for verification).
    pub end_pos: usize,
    pub virtual_end: u32,
    pub virtual_expected: u32,
    /// Why the walk stopped early, if it did (e.g. an unsupported asset type).
    pub stopped: Option<String>,
}

impl ZoneData {
    pub fn complete(&self) -> bool {
        self.stopped.is_none() && self.end_pos == self.data.len() && self.virtual_end == self.virtual_expected
    }

    pub fn xmodel(&self, name: &str) -> Option<&XModelInfo> {
        self.xmodels.iter().find(|m| m.name == name)
    }

    pub fn material(&self, name: &str) -> Option<&MaterialInfo> {
        self.materials.iter().find(|m| m.name == name)
    }

    pub fn sound(&self, name: &str) -> Option<&SoundList> {
        self.sounds.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn xanim(&self, name: &str) -> Option<&XAnimInfo> {
        self.xanims.iter().find(|a| a.name == name)
    }

    pub fn weapon(&self, name: &str) -> Option<&WeaponInfo> {
        self.weapons.iter().find(|w| w.name.eq_ignore_ascii_case(name))
    }

    /// Image name with any leading `,` (cross-zone reference marker) removed.
    pub fn image_name(&self, i: u32) -> &str {
        self.images[i as usize].name.trim_start_matches(',')
    }

    /// Raw bytes of an inline loaded sound (a complete RIFF file).
    pub fn loaded_sound_bytes(&self, i: u32) -> &[u8] {
        let s = &self.loaded_sounds[i as usize];
        &self.data[s.fpos..s.fpos + s.len]
    }

    pub fn localized(&self, key: &str) -> Option<&str> {
        self.localize.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real-data check: `UNDEAD_WAW=<install> cargo test -p waw_assets -- --ignored`.
    #[test]
    #[ignore]
    fn walks_nacht_completely() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let ff = std::fs::read(std::path::Path::new(&root).join("zone/english/nazi_zombie_prototype.ff")).unwrap();
        let zd = walk(crate::zone::decompress(&ff).unwrap());
        assert!(zd.complete(), "stopped {:?} at {} of {}", zd.stopped, zd.end_pos, zd.data.len());
        assert_eq!(zd.unresolved, 0);
        assert_eq!(zd.assets.len(), 5649);
        let w = zd.world.as_ref().unwrap();
        assert_eq!((w.vertex_count, w.index_count, w.surfaces.len(), w.smodels.len()), (91002, 203895, 3741, 1506));
        assert!(zd.map_ents.as_ref().unwrap().contains("worldspawn"));
        let m = zd.xmodel("char_ger_honorgd_zomb_behead").unwrap();
        let lod0: usize = m.surfs[m.lod_surfs(0)].iter().map(|s| s.tri_count as usize).sum();
        assert_eq!((m.bones.len(), lod0), (28, 668));
        assert_eq!(zd.weapon("kar98k").and_then(|w| w.sound("fireSound")), Some("weap_kar98k_fire"));
    }

    #[test]
    fn sampler_hashes() {
        assert_eq!(sampler_hash("colorMap"), COLOR_MAP);
        assert_eq!(sampler_hash("normalMap"), NORMAL_MAP);
        assert_eq!(sampler_hash("specularMap"), 0x34ec_ccb3);
    }
}
