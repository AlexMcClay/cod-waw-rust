//! Effects (`FxEffectDef`, asset type 26) and impact tables (`FxImpactTable`,
//! type 27) as the zone stores them. See `research/vfx/WAW_FX.md`.
//!
//! An effect is a list of element definitions, split into three groups by
//! position: looping elements first, then one-shot elements, then emitted
//! ones (spawned by other elements as they move). Each element spawns
//! particles of one type (sprite, tail, cloud, model, light, runner...)
//! with random ranges for its delay, lifetime, origin, angles and gravity,
//! and samples over normalised life for velocity (`vel`) and look (`vis`).

/// `FxFloatRange`: `base + amplitude * random(0..1)`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Range {
    pub base: f32,
    pub amp: f32,
}

impl Range {
    pub fn at(&self, r: f32) -> f32 {
        self.base + self.amp * r
    }
    /// The largest value the range can produce.
    pub fn max(&self) -> f32 {
        self.base + self.amp.max(0.0)
    }
}

/// `FxIntRange`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IntRange {
    pub base: i32,
    pub amp: i32,
}

impl IntRange {
    pub fn at(&self, r: f32) -> i32 {
        self.base + (self.amp as f32 * r).round() as i32
    }
    pub fn max(&self) -> i32 {
        self.base + self.amp.max(0)
    }
}

/// Element types (`FxElemType`, World at War numbering).
pub mod elem_type {
    pub const SPRITE_BILLBOARD: u8 = 0;
    pub const SPRITE_ORIENTED: u8 = 1;
    pub const SPRITE_ROTATED: u8 = 2;
    pub const TAIL: u8 = 3;
    pub const TRAIL: u8 = 4;
    pub const CLOUD: u8 = 5;
    pub const MODEL: u8 = 6;
    pub const OMNI_LIGHT: u8 = 7;
    pub const SPOT_LIGHT: u8 = 8;
    pub const SOUND: u8 = 9;
    pub const DECAL: u8 = 10;
    pub const RUNNER: u8 = 11;

    pub fn name(t: u8) -> &'static str {
        match t {
            0 => "billboard",
            1 => "oriented",
            2 => "rotated",
            3 => "tail",
            4 => "trail",
            5 => "cloud",
            6 => "model",
            7 => "omni_light",
            8 => "spot_light",
            9 => "sound",
            10 => "decal",
            11 => "runner",
            _ => "?",
        }
    }
}

/// `FxElemDef.flags` bits.
pub mod elem_flags {
    pub const SPAWN_RELATIVE_TO_EFFECT: u32 = 0x2;
    pub const SPAWN_FRUSTUM_CULL: u32 = 0x4;
    pub const RUNNER_USES_RAND_ROT: u32 = 0x8;
    pub const SPAWN_OFFSET_SPHERE: u32 = 0x10;
    pub const SPAWN_OFFSET_CYLINDER: u32 = 0x20;
    pub const SPAWN_OFFSET_MASK: u32 = 0x30;
    pub const RUN_RELATIVE_TO_SPAWN: u32 = 0x40;
    pub const RUN_RELATIVE_TO_EFFECT: u32 = 0x80;
    pub const RUN_RELATIVE_TO_OFFSET: u32 = 0xC0;
    pub const RUN_MASK: u32 = 0xC0;
    pub const USE_COLLISION: u32 = 0x100;
    pub const DIE_ON_TOUCH: u32 = 0x200;
    pub const DRAW_PAST_FOG: u32 = 0x400;
    pub const DRAW_WITH_VIEWMODEL: u32 = 0x800;
    pub const BLOCK_SIGHT: u32 = 0x1000;
    pub const HAS_VELOCITY_GRAPH_LOCAL: u32 = 0x0100_0000;
    pub const HAS_VELOCITY_GRAPH_WORLD: u32 = 0x0200_0000;
    pub const HAS_GRAVITY: u32 = 0x0400_0000;
    pub const USE_MODEL_PHYSICS: u32 = 0x0800_0000;
    pub const NONUNIFORM_SCALE: u32 = 0x1000_0000;
}

/// `FxElemAtlas`: which frame of a texture atlas a particle shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Atlas {
    /// Bits 0-1: start frame (0 fixed `index`, 1 random, 2 indexed by
    /// spawn order); 4: play the frames over the particle's life; 8: loop
    /// only `loop_count` times.
    pub behavior: u8,
    pub index: u8,
    /// Frames per second (0 = static, unless played over life).
    pub fps: u8,
    pub loop_count: u8,
    pub col_index_bits: u8,
    pub row_index_bits: u8,
    pub entry_count: i16,
}

impl Atlas {
    pub const START_FIXED: u8 = 0;
    pub const START_RANDOM: u8 = 1;
    pub const START_INDEXED: u8 = 2;
    pub const START_MASK: u8 = 3;
    pub const PLAY_OVER_LIFE: u8 = 4;
    pub const LOOP_ONLY_N_TIMES: u8 = 8;
}

/// `FxElemVec3Range`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3Range {
    pub base: [f32; 3],
    pub amp: [f32; 3],
}

/// One velocity sample (`FxElemVelStateSample`): in the element's own
/// frame (`local`) and in world axes (`world`). `velocity` is in units per
/// millisecond; `total_delta` is the distance travelled since spawn by this
/// sample, in units per millisecond of life (multiply by the lifetime).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VelSample {
    pub local_velocity: Vec3Range,
    pub local_delta: Vec3Range,
    pub world_velocity: Vec3Range,
    pub world_delta: Vec3Range,
}

/// `FxElemVisualState`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VisState {
    /// RGBA, 0..255.
    pub color: [u8; 4],
    pub rotation_delta: f32,
    pub rotation_total: f32,
    pub size: [f32; 2],
    pub scale: f32,
}

/// One look sample (`FxElemVisStateSample`): `base + amplitude * random`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VisSample {
    pub base: VisState,
    pub amp: VisState,
}

/// What an element draws (`FxElemVisuals`).
#[derive(Debug, Clone, PartialEq)]
pub enum Visual {
    /// Index into the zone's materials (sprites, tails, trails, clouds).
    Material(u32),
    /// Index into the zone's xmodels.
    Model(u32),
    /// Another effect (runners), by name.
    Effect(String),
    /// A sound alias, by name.
    Sound(String),
    /// Decal materials (`[normal, ...]`).
    Mark([Option<u32>; 2]),
}

/// `FxTrailDef`: the cross-section of a trail element.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trail {
    pub scroll_time_msec: i32,
    pub repeat_dist: i32,
    pub split_dist: i32,
    /// `(pos, normal, texCoord)` per vertex.
    pub verts: Vec<([f32; 2], [f32; 2], f32)>,
    pub inds: Vec<u16>,
}

/// One `FxElemDef`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Elem {
    pub flags: u32,
    /// Looping elements: `(interval msec, count)`; one-shot: count range
    /// `(base, amplitude)`.
    pub spawn: [i32; 2],
    pub spawn_range: Range,
    pub fade_in_range: Range,
    pub fade_out_range: Range,
    pub spawn_frustum_cull_radius: f32,
    pub spawn_delay_msec: IntRange,
    pub life_span_msec: IntRange,
    pub spawn_origin: [Range; 3],
    pub spawn_offset_radius: Range,
    pub spawn_offset_height: Range,
    /// Degrees.
    pub spawn_angles: [Range; 3],
    /// Degrees per millisecond.
    pub angular_velocity: [Range; 3],
    /// Radians.
    pub initial_rotation: Range,
    pub gravity: Range,
    pub reflection_factor: Range,
    pub atlas: Atlas,
    pub wind_influence: f32,
    pub elem_type: u8,
    pub visuals: Vec<Visual>,
    pub vel: Vec<VelSample>,
    pub vis: Vec<VisSample>,
    pub coll_mins: [f32; 3],
    pub coll_maxs: [f32; 3],
    pub effect_on_impact: Option<String>,
    pub effect_on_death: Option<String>,
    pub effect_emitted: Option<String>,
    pub emit_dist: Range,
    pub emit_dist_variance: Range,
    pub trail: Option<Trail>,
    pub sort_order: u8,
    /// How much scene lighting affects the particle (0..255).
    pub lighting_frac: u8,
    pub use_item_clip: u8,
}

impl Elem {
    pub fn looping_interval(&self) -> i32 {
        self.spawn[0]
    }
    pub fn looping_count(&self) -> i32 {
        self.spawn[1]
    }
    pub fn oneshot_count(&self) -> IntRange {
        IntRange { base: self.spawn[0], amp: self.spawn[1] }
    }
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

/// One `FxEffectDef`. A name starting with `,` is a reference to an effect
/// defined in another zone (no elements).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Effect {
    pub name: String,
    pub flags: i32,
    pub total_size: i32,
    pub msec_looping_life: i32,
    pub looping: usize,
    pub oneshot: usize,
    pub emission: usize,
    pub priority: u8,
    pub elems: Vec<Elem>,
}

impl Effect {
    pub fn is_stub(&self) -> bool {
        self.name.starts_with(',')
    }
    pub fn looping_elems(&self) -> &[Elem] {
        &self.elems[..self.looping.min(self.elems.len())]
    }
    pub fn oneshot_elems(&self) -> &[Elem] {
        let a = self.looping.min(self.elems.len());
        &self.elems[a..(a + self.oneshot).min(self.elems.len())]
    }
    pub fn emission_elems(&self) -> &[Elem] {
        let a = (self.looping + self.oneshot).min(self.elems.len());
        &self.elems[a..]
    }
}

/// Surface types (`surfaceNames`), indexing `FxImpactTable` entries, a
/// grenade's bounce sounds and `MaterialInfo::surface_type_bits`.
pub const SURFACE_TYPES: [&str; 31] = [
    "default", "bark", "brick", "carpet", "cloth", "concrete", "dirt", "flesh", "foliage", "glass", "grass",
    "gravel", "ice", "metal", "mud", "paper", "plaster", "rock", "sand", "snow", "water", "wood", "asphalt",
    "ceramic", "plastic", "rubber", "cushion", "fruit", "paintedmetal", "player", "tallgrass",
];

/// One row of an impact table: an effect per surface type, and four for
/// flesh hits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImpactEntry {
    /// Effect names (indices into [`super::ZoneData::fx`] resolved to names).
    pub nonflesh: Vec<Option<String>>,
    pub flesh: Vec<Option<String>>,
}

/// `FxImpactTable`: rows indexed by a weapon's `impactType`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImpactTable {
    pub name: String,
    pub entries: Vec<ImpactEntry>,
}

/// Effects a weapon plays (names; empty when unset).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeaponFx {
    pub view_flash: Option<String>,
    pub world_flash: Option<String>,
    pub view_shell_eject: Option<String>,
    pub world_shell_eject: Option<String>,
    pub view_last_shot_eject: Option<String>,
    pub world_last_shot_eject: Option<String>,
    pub proj_explosion: Option<String>,
    pub proj_dud: Option<String>,
    pub proj_trail: Option<String>,
    pub proj_ignition: Option<String>,
    /// `impactType`: row of the impact table for this weapon's hits.
    pub impact_type: i32,
}

// ------------------------------------------------------------- raw parsing

fn f32_at(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn i32_at(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn range_at(d: &[u8], o: usize) -> Range {
    Range { base: f32_at(d, o), amp: f32_at(d, o + 4) }
}
fn vec3_at(d: &[u8], o: usize) -> [f32; 3] {
    [f32_at(d, o), f32_at(d, o + 4), f32_at(d, o + 8)]
}
fn vec3range_at(d: &[u8], o: usize) -> Vec3Range {
    Vec3Range { base: vec3_at(d, o), amp: vec3_at(d, o + 12) }
}
fn vis_state_at(d: &[u8], o: usize) -> VisState {
    VisState {
        color: [d[o], d[o + 1], d[o + 2], d[o + 3]],
        rotation_delta: f32_at(d, o + 4),
        rotation_total: f32_at(d, o + 8),
        size: [f32_at(d, o + 12), f32_at(d, o + 16)],
        scale: f32_at(d, o + 20),
    }
}

/// The scalar fields of a 256-byte `FxElemDef` (pointers are filled in by
/// the walker).
pub(crate) fn elem_from_bytes(d: &[u8]) -> Elem {
    let r3 = |o: usize| [range_at(d, o), range_at(d, o + 8), range_at(d, o + 16)];
    Elem {
        flags: i32_at(d, 0) as u32,
        spawn: [i32_at(d, 4), i32_at(d, 8)],
        spawn_range: range_at(d, 0xc),
        fade_in_range: range_at(d, 0x14),
        fade_out_range: range_at(d, 0x1c),
        spawn_frustum_cull_radius: f32_at(d, 0x24),
        spawn_delay_msec: IntRange { base: i32_at(d, 0x28), amp: i32_at(d, 0x2c) },
        life_span_msec: IntRange { base: i32_at(d, 0x30), amp: i32_at(d, 0x34) },
        spawn_origin: r3(0x38),
        spawn_offset_radius: range_at(d, 0x50),
        spawn_offset_height: range_at(d, 0x58),
        spawn_angles: r3(0x60),
        angular_velocity: r3(0x78),
        initial_rotation: range_at(d, 0x90),
        gravity: range_at(d, 0x98),
        reflection_factor: range_at(d, 0xa0),
        atlas: Atlas {
            behavior: d[0xa8],
            index: d[0xa9],
            fps: d[0xaa],
            loop_count: d[0xab],
            col_index_bits: d[0xac],
            row_index_bits: d[0xad],
            entry_count: i16::from_le_bytes([d[0xae], d[0xaf]]),
        },
        wind_influence: f32_at(d, 0xb0),
        elem_type: d[0xb4],
        coll_mins: vec3_at(d, 0xc4),
        coll_maxs: vec3_at(d, 0xd0),
        emit_dist: range_at(d, 0xe8),
        emit_dist_variance: range_at(d, 0xf0),
        sort_order: d[0xfc],
        lighting_frac: d[0xfd],
        use_item_clip: d[0xfe],
        ..Default::default()
    }
}

/// `n` velocity samples (96 bytes each).
pub(crate) fn vel_samples(d: &[u8], n: usize) -> Vec<VelSample> {
    (0..n)
        .map(|i| {
            let o = 96 * i;
            VelSample {
                local_velocity: vec3range_at(d, o),
                local_delta: vec3range_at(d, o + 24),
                world_velocity: vec3range_at(d, o + 48),
                world_delta: vec3range_at(d, o + 72),
            }
        })
        .collect()
}

/// `n` look samples (48 bytes each).
pub(crate) fn vis_samples(d: &[u8], n: usize) -> Vec<VisSample> {
    (0..n).map(|i| VisSample { base: vis_state_at(d, 48 * i), amp: vis_state_at(d, 48 * i + 24) }).collect()
}

pub(crate) fn trail_verts(d: &[u8], n: usize) -> Vec<([f32; 2], [f32; 2], f32)> {
    (0..n)
        .map(|i| {
            let o = 20 * i;
            ([f32_at(d, o), f32_at(d, o + 4)], [f32_at(d, o + 8), f32_at(d, o + 12)], f32_at(d, o + 16))
        })
        .collect()
}
