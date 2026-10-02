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
pub mod clipmap;
pub mod decode;
mod walker;
pub mod weapondef;

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
    Font,
    Localize,
    SndDriverGlobals,
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
            20 => Font,
            25 => SndDriverGlobals,
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
    /// File position and count of the `GfxStateBits` table (2 x u32 each).
    pub state_bits: Option<(usize, usize)>,
    /// Technique type -> index into the state bits table (0xff = none).
    pub state_entry: Vec<u8>,
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
    /// Collision surfaces; 0 means the model is not solid.
    pub num_coll_surfs: u32,
    pub contents: i32,
    /// LOD whose geometry the game collides with (-1 = none).
    pub coll_lod: i16,
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
    /// Index into the map's primary lights of the light drawn on top of
    /// the lightmap (0 = none, 1 = the sun).
    pub primary_light: u8,
    pub flags: u8,
}

/// A light the game draws per pixel on top of the baked lighting
/// (`ComPrimaryLight` / `GfxLight`). Game units and axes.
#[derive(Debug, Clone, Default)]
pub struct PrimaryLight {
    /// 1 = sun, 2 = spot, 3 = omni.
    pub kind: u8,
    pub color: [f32; 3],
    /// Sun: towards the sun. Spot: towards the light (minus the cone axis).
    pub dir: [f32; 3],
    pub origin: [f32; 3],
    pub radius: f32,
    pub cos_half_fov_outer: f32,
    pub cos_half_fov_inner: f32,
    pub exponent: u8,
    /// Light definition (falloff curve), e.g. `light_point_linear`.
    pub def_name: String,
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
    /// The sun as the world draws it (`GfxWorld.sunLight`).
    pub sun_light: Option<PrimaryLight>,
    /// Baked light for models (`GfxLightGrid`).
    pub light_grid: Option<LightGridInfo>,
}

/// The light grid: points every 32 x 32 x 64 units, each holding the light
/// arriving from all around as a 4x4x4 block of colours (its 56 surface
/// cells) and the primary light a model there uses. Rows run along
/// `row_axis`, columns along `col_axis`, with run-length encoded columns.
#[derive(Debug, Clone)]
pub struct LightGridInfo {
    pub mins: [u16; 3],
    pub maxs: [u16; 3],
    pub row_axis: usize,
    pub col_axis: usize,
    pub rows: usize,
    pub row_starts: usize,
    pub raw_rows: usize,
    pub raw_rows_len: usize,
    pub entries: usize,
    pub entry_count: usize,
    pub colors: usize,
    pub color_count: usize,
}

/// One light grid point: its colour block and primary light.
#[derive(Debug, Clone, Copy)]
pub struct GridEntry {
    pub colors_index: u16,
    pub primary_light: u8,
}

/// The 56 surface cells of a 4x4x4 cube in storage order (x fastest, then
/// y, then z, skipping the 8 inner cells).
pub fn grid_cube_cells() -> [[u8; 3]; 56] {
    let mut out = [[0u8; 3]; 56];
    let mut n = 0;
    for z in 0..4u8 {
        for y in 0..4u8 {
            for x in 0..4u8 {
                let inner = (1..=2).contains(&x) && (1..=2).contains(&y) && (1..=2).contains(&z);
                if !inner {
                    out[n] = [x, y, z];
                    n += 1;
                }
            }
        }
    }
    out
}

impl LightGridInfo {
    /// Grid coordinates of the cell containing a game-space point.
    pub fn coord(p: [f32; 3]) -> [i64; 3] {
        [(p[0] / 32.0).floor() as i64 + 4096, (p[1] / 32.0).floor() as i64 + 4096, (p[2] / 64.0).floor() as i64 + 2048]
    }

    /// The entry stored at grid coordinates `g`, if any.
    pub fn entry_index(&self, data: &[u8], g: [i64; 3]) -> Option<usize> {
        let (ra, ca) = (self.row_axis, self.col_axis);
        let row = g[ra] - self.mins[ra] as i64;
        if row < 0 || row as usize >= self.rows {
            return None;
        }
        let rs = self.row_starts + 2 * row as usize;
        let start = u16::from_le_bytes([*data.get(rs)?, *data.get(rs + 1)?]);
        if start == 0xffff {
            return None;
        }
        let raw = data.get(self.raw_rows..self.raw_rows + self.raw_rows_len)?;
        let o = 4 * start as usize;
        let rd16 = |i: usize| raw.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let (cs, cc, zs, zc) = (rd16(o)? as i64, rd16(o + 2)? as i64, rd16(o + 4)? as i64, rd16(o + 6)? as i64);
        let mut entry = raw.get(o + 8..o + 12).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))? as i64;
        let mut col = g[ca] - cs;
        let zi = g[2] - zs;
        if !(0..cc).contains(&col) || !(0..zc).contains(&zi) {
            return None;
        }
        let mut p = o + 12;
        loop {
            let (n, k) = (*raw.get(p)? as i64, *raw.get(p + 1)? as i64);
            p += 2;
            let mut zo = 0;
            if k > 0 {
                zo = *raw.get(p)? as i64;
                p += 1;
            }
            if col < n {
                if k == 0 || !(zo..zo + k).contains(&zi) {
                    return None;
                }
                return Some((entry + col * k + (zi - zo)) as usize);
            }
            col -= n;
            entry += n * k;
            if n == 0 && k == 0 {
                return None;
            }
        }
    }

    pub fn entry(&self, data: &[u8], i: usize) -> Option<GridEntry> {
        if i >= self.entry_count {
            return None;
        }
        let b = data.get(self.entries + 4 * i..self.entries + 4 * i + 4)?;
        Some(GridEntry { colors_index: u16::from_le_bytes([b[0], b[1]]), primary_light: b[2] })
    }

    /// The 56 colours (0..1, gamma) of an entry's block, as a 4x4x4 cube
    /// indexed [x][y][z] (inner cells are zero).
    pub fn cube(&self, data: &[u8], e: GridEntry) -> Option<[[[[f32; 3]; 4]; 4]; 4]> {
        let ci = e.colors_index as usize;
        if ci >= self.color_count {
            return None;
        }
        let b = data.get(self.colors + 168 * ci..self.colors + 168 * (ci + 1))?;
        let mut cube = [[[[0.0f32; 3]; 4]; 4]; 4];
        for (i, [x, y, z]) in grid_cube_cells().into_iter().enumerate() {
            cube[x as usize][y as usize][z as usize] = [b[3 * i] as f32 / 255.0, b[3 * i + 1] as f32 / 255.0, b[3 * i + 2] as f32 / 255.0];
        }
        Some(cube)
    }
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
    /// Played together with this one (weapon shots layer their action,
    /// tail and shell sounds this way).
    pub secondary: Option<String>,
    /// Played once this one has finished.
    pub chain: Option<String>,
    /// Seconds to wait before playing.
    pub start_delay: f32,
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
    /// Viewmodel notetrack name -> sound alias it plays.
    pub notetrack_sounds: Vec<(String, String)>,
    /// The model a thrown/fired projectile uses (`projectileModel`).
    pub projectile_model: Option<u32>,
    /// `bounceSound`: an alias per surface type (empty when unset), if any.
    pub bounce_sounds: Vec<String>,
    /// Gameplay numbers (damage, timings, ammo, spread, hit-location
    /// multipliers...) as weapon-file `(key, value)` pairs; see
    /// [`weapondef::stats`].
    pub stats: Vec<(&'static str, String)>,
}

impl WeaponInfo {
    pub fn sound(&self, field: &str) -> Option<&str> {
        self.sounds.iter().find(|(f, _)| *f == field).map(|(_, s)| s.as_str())
    }

    /// One of [`Self::stats`] by its weapon-file key.
    pub fn stat(&self, key: &str) -> Option<&str> {
        self.stats.iter().find(|(f, _)| *f == key).map(|(_, v)| v.as_str())
    }
}

/// `GfxStateBits.loadBits[0]` of a technique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderState(pub u32);

impl RenderState {
    /// Alpha test enabled (GT0, LT128 or GE128).
    pub fn alpha_test(self) -> bool {
        matches!(self.0 & 0x3800, 0x1000 | 0x2000 | 0x3000)
    }
    /// Drawn without face culling.
    pub fn two_sided(self) -> bool {
        self.0 & 0xc000 == 0x4000
    }
    /// Blends with source alpha (src SRC_ALPHA, dst INV_SRC_ALPHA).
    pub fn alpha_blend(self) -> bool {
        self.0 & 0xf == 5 && (self.0 >> 4) & 0xf == 6
    }
}

/// A bitmap font (`Font_s`): glyphs in a shared atlas image.
#[derive(Debug, Clone)]
pub struct FontInfo {
    pub name: String,
    /// Height of the line box in atlas pixels.
    pub pixel_height: i32,
    pub glyph_count: usize,
    pub glyphs_fpos: Option<usize>,
    pub material: Option<u32>,
}

/// One character of a [`FontInfo`]. Positions are in atlas pixels relative
/// to the text origin, which is the bottom of the line box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub letter: u16,
    pub x0: i8,
    pub y0: i8,
    /// Advance to the next character.
    pub dx: u8,
    pub width: u8,
    pub height: u8,
    /// Atlas texture coordinates `s0, t0, s1, t1`.
    pub uv: [f32; 4],
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
    pub fonts: Vec<FontInfo>,
    /// Raw files (scripts, vision sets...): (name, file position, length).
    pub rawfiles: Vec<(String, usize, usize)>,
    /// The map's primary lights (`ComWorld`), indexed by
    /// `WorldSurface::primary_light`.
    pub primary_lights: Vec<PrimaryLight>,
    pub xanims: Vec<XAnimInfo>,
    pub world: Option<WorldInfo>,
    /// The map's collision (brushes, terrain, brush models).
    pub clipmap: Option<clipmap::ClipMapInfo>,
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

    /// The render state (`loadBits[0]`) of the material's main lit
    /// technique (technique types 8..=42 are the lit variants).
    pub fn lit_state(&self, m: &MaterialInfo) -> Option<RenderState> {
        let (p, n) = m.state_bits?;
        let idx = m.state_entry.get(8..=42)?.iter().copied().find(|&i| i != 0xff).or_else(|| m.state_entry.get(4).copied().filter(|&i| i != 0xff))?;
        if idx as usize >= n {
            return None;
        }
        let o = p + 8 * idx as usize;
        let b = self.data.get(o..o + 4)?;
        Some(RenderState(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
    }

    /// A raw file's text (scripts, `.vision` files).
    pub fn rawfile(&self, name: &str) -> Option<String> {
        let (_, p, len) = self.rawfiles.iter().find(|(n, ..)| n.eq_ignore_ascii_case(name))?;
        self.data.get(*p..p + len).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    pub fn font(&self, name: &str) -> Option<&FontInfo> {
        self.fonts.iter().find(|f| f.name.eq_ignore_ascii_case(name))
    }

    /// A font's glyph table.
    pub fn glyphs(&self, font: &FontInfo) -> Vec<Glyph> {
        let Some(p) = font.glyphs_fpos else { return Vec::new() };
        let d = &self.data;
        let f32_at = |o: usize| f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
        (0..font.glyph_count)
            .filter_map(|i| {
                let o = p + 24 * i;
                (o + 24 <= d.len()).then(|| Glyph {
                    letter: u16::from_le_bytes([d[o], d[o + 1]]),
                    x0: d[o + 2] as i8,
                    y0: d[o + 3] as i8,
                    dx: d[o + 4],
                    width: d[o + 5],
                    height: d[o + 6],
                    uv: [f32_at(o + 8), f32_at(o + 12), f32_at(o + 16), f32_at(o + 20)],
                })
            })
            .collect()
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
        // World vertices carry a tangent frame: unit, perpendicular to the
        // normal, with a +-1 binormal sign.
        let (mut ok, mut n) = (0, 0);
        for i in (0..w.vertex_count).step_by(97) {
            let v = decode::world_vertex(&zd, w, i).unwrap();
            let dot: f32 = (0..3).map(|k| v.normal[k] * v.tangent[k]).sum();
            n += 1;
            if dot.abs() < 0.2 && (v.binormal_sign.abs() - 1.0).abs() < 0.01 {
                ok += 1;
            }
        }
        assert!(ok * 10 >= n * 9, "{ok}/{n} vertices with a sane tangent frame");
        // Primary lights: 21 entries (0 = none, 1 = sun, spots, omnis).
        assert_eq!(zd.primary_lights.len(), 21);
        assert_eq!(zd.primary_lights[1].kind, 1);
        let omni20 = &zd.primary_lights[20];
        assert_eq!((omni20.kind, omni20.radius, omni20.def_name.as_str()), (3, 350.0, "tungsten_lamp"));
        let sun = w.sun_light.as_ref().unwrap();
        assert!((sun.color[0] - 0.3536).abs() < 0.001 && (sun.dir[2] - 0.5).abs() < 0.01, "{sun:?}");
        let lit = w.surfaces.iter().filter(|s| s.primary_light == 1).count();
        assert_eq!(lit, 1066);
    }

    /// The zone's WeaponDefs carry the same numbers as the IWD weapon files.
    #[test]
    #[ignore]
    fn reads_weapon_stats() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let ff = std::fs::read(std::path::Path::new(&root).join("zone/english/nazi_zombie_prototype.ff")).unwrap();
        let zd = walk(crate::zone::decompress(&ff).unwrap());
        let k = zd.weapon("kar98k").unwrap();
        assert_eq!(k.stat("damage"), Some("100"));
        assert_eq!(k.stat("fireType"), Some("Single Shot"));
        assert_eq!(k.stat("fireTime"), Some("0.33"));
        assert_eq!(k.stat("rechamberTime"), Some("1"));
        assert_eq!(k.stat("locHead"), Some("3.5"));
        assert_eq!(k.stat("locHelmet"), Some("1"));
        assert_eq!(k.stat("maxDamageRange"), Some("1200"));
        let s = zd.weapon("shotgun").unwrap();
        assert_eq!((s.stat("shotCount"), s.stat("segmentedReload"), s.stat("weaponClass")), (Some("8"), Some("1"), Some("spread")));
        let r = zd.weapon("ray_gun").unwrap();
        assert_eq!((r.stat("weaponType"), r.stat("fireType"), r.stat("explosionInnerDamage")), (Some("projectile"), Some("Full Auto"), Some("1500")));
        assert_eq!(zd.weapon("thompson").unwrap().stat("penetrateType"), Some("medium"));
    }

    /// The game's bitmap fonts come from code_post_gfx.ff.
    #[test]
    #[ignore]
    fn reads_fonts() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let ff = std::fs::read(std::path::Path::new(&root).join("zone/english/code_post_gfx.ff")).unwrap();
        let zd = walk(crate::zone::decompress(&ff).unwrap());
        assert_eq!(zd.unresolved, 0);
        let names: Vec<&str> = zd.fonts.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names.len(), 9, "{names:?} stopped {:?} at {}", zd.stopped, zd.end_pos);
        let f = zd.font("fonts/objectiveFont").unwrap();
        assert_eq!((f.pixel_height, f.glyph_count), (28, 191));
        let g = zd.glyphs(f);
        assert_eq!(g[0].letter, 32);
        assert_eq!(g[(b'0' - 32) as usize].dx, 13);
        let m = &zd.materials[f.material.unwrap() as usize];
        assert_eq!(m.textures.first().and_then(|t| t.image).map(|i| zd.image_name(i)), Some("gamefonts_pc"));
    }

    #[test]
    fn sampler_hashes() {
        assert_eq!(sampler_hash("colorMap"), COLOR_MAP);
        assert_eq!(sampler_hash("normalMap"), NORMAL_MAP);
        assert_eq!(sampler_hash("specularMap"), 0x34ec_ccb3);
    }
}
