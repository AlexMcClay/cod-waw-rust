//! Builds Nacht der Untoten from the user's install, off the main thread:
//! map geometry grouped by material, brush submodels, static and script
//! models, textures, the baked lightmap and a collision mesh.
//!
//! Game space is inches with Z up; Bevy space is metres with Y up:
//! `(x, y, z) -> (x, z, -y) * 0.0254`, which keeps handedness.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::HashMap;
use waw_assets::iwi::Iwi;
use waw_assets::mapents;
use waw_assets::t4::{self, decode, XModelInfo, ZoneData};
use waw_assets::zombiemap::ZombieMap;
use waw_assets::{Install, Iwd};
use zm_core::geom::V3;
use zm_core::trimesh::{blocks, Tri, TriMesh};

pub const INCH: f32 = 0.0254;

/// Game-space point (inches, Z up) to Bevy space.
pub fn to_bevy(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1]) * INCH
}

/// Game-space direction to Bevy space (no scaling).
pub fn dir_to_bevy(n: [f32; 3]) -> Vec3 {
    Vec3::new(n[0], n[2], -n[1])
}

/// Rotation for game angles (pitch, yaw, roll in degrees).
pub fn angles_to_quat(a: [f32; 3]) -> Quat {
    let (pitch, yaw, roll) = (a[0].to_radians(), a[1].to_radians(), a[2].to_radians());
    // Game: yaw about Z(up), pitch about Y(left), roll about X(forward).
    // In Bevy space Z(up) -> Y, Y(left) -> -Z, X(forward) -> X.
    Quat::from_rotation_y(yaw) * Quat::from_rotation_z(-pitch) * Quat::from_rotation_x(roll)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blend {
    Opaque,
    Mask,
    Blend,
    /// Additive (sky glow layers).
    Add,
}

#[derive(Debug, Clone)]
pub struct SceneMaterial {
    pub color: Option<String>,
    pub blend: Blend,
    pub unlit: bool,
    pub lightmapped: bool,
    /// Drawn without back-face culling.
    pub two_sided: bool,
    /// Normal map (loaded raw, not sRGB).
    pub normal: Option<String>,
    /// Lit shaders tint by vertex colour; layered ones use it as a blend
    /// weight instead.
    pub vertex_tint: bool,
}

/// A lit world surface group: one material lit by one primary light.
pub struct SceneWorldMesh {
    pub mesh: Mesh,
    pub material: usize,
    pub primary_light: u8,
    /// Which lightmap the surfaces use.
    pub lightmap: u8,
}

/// A primary light in Bevy space (metres).
#[derive(Clone, Copy, Default)]
pub struct SceneLight {
    /// 0 none, 1 sun, 2 spot, 3 omni.
    pub kind: u8,
    pub color: Vec3,
    pub position: Vec3,
    pub radius: f32,
    pub dir: Vec3,
    pub cos_outer: f32,
    pub cos_inner: f32,
    pub exponent: f32,
    /// 0 linear, 1 tungsten.
    pub falloff: u8,
}

pub struct SceneMesh {
    pub mesh: Mesh,
    pub material: usize,
}

/// A bone of a skinned character, in Bevy space.
pub struct SceneBone {
    pub name: String,
    pub parent: Option<usize>,
    /// Bind pose relative to the parent (or the model root).
    pub local: Transform,
    pub inv_bind: Mat4,
}

/// A model skinned to its own skeleton.
pub struct SkinnedPart {
    pub bones: Vec<SceneBone>,
    pub meshes: Vec<SceneMesh>,
}

/// A zombie: body plus interchangeable heads (merged by bone name).
pub struct SceneCharacter {
    pub body: SkinnedPart,
    pub heads: Vec<SkinnedPart>,
}

/// First-person rig: arms plus a gun per weapon (gun root `j_gun` hangs off
/// the arms' `tag_weapon`), and the weapon's animations by slot.
pub struct SceneViewRig {
    pub arms: SkinnedPart,
    pub guns: HashMap<String, SkinnedPart>,
    /// weapon id -> (slot name, clip)
    pub anims: HashMap<String, Vec<(&'static str, AnimClip)>>,
}

/// An animation in Bevy space: absolute local rotations and translation
/// offsets from the bind pose, per bone, keyed by frame.
#[derive(Clone)]
pub struct AnimClip {
    pub name: String,
    pub numframes: f32,
    pub duration: f32,
    pub looping: bool,
    pub tracks: Vec<AnimTrack>,
    /// Root-motion speed (metres per second) and total rise (metres).
    pub root_speed: f32,
    #[allow(dead_code)] // for root-motion window climbs
    pub root_rise: f32,
    /// Notetracks (name, frame fraction); not used by the game yet.
    #[allow(dead_code)]
    pub notify: Vec<(String, f32)>,
}

#[derive(Clone)]
pub struct AnimTrack {
    pub bone: String,
    pub rot: Vec<(f32, Quat)>,
    pub trans: Vec<(f32, Vec3)>,
}

impl AnimClip {
    fn from_clip(c: &waw_assets::t4::anim::Clip) -> AnimClip {
        let end = c.root_trans(c.numframes as f32);
        AnimClip {
            name: c.name.clone(),
            numframes: c.numframes as f32,
            duration: c.duration(),
            looping: c.looping,
            tracks: c
                .tracks
                .iter()
                .map(|t| AnimTrack {
                    bone: t.bone.to_ascii_lowercase(),
                    // Bones without rotation data are explicitly identity.
                    rot: if t.rot.is_empty() { vec![(0.0, Quat::IDENTITY)] } else { t.rot.iter().map(|(f, q)| (*f as f32, rot_to_bevy(*q))).collect() },
                    trans: t.trans.iter().map(|(f, v)| (*f as f32, to_bevy(*v))).collect(),
                })
                .collect(),
            root_speed: c.root_speed() * INCH,
            root_rise: end[2] * INCH,
            notify: c.notify.clone(),
        }
    }

    /// Frame for a time in seconds (wrapping when looping).
    pub fn frame_at(&self, t: f32) -> f32 {
        if self.duration <= 0.0 {
            return 0.0;
        }
        let k = if self.looping { (t / self.duration).rem_euclid(1.0) } else { (t / self.duration).clamp(0.0, 1.0) };
        k * self.numframes
    }
}

impl AnimTrack {
    pub fn rotation(&self, frame: f32) -> Quat {
        let k = &self.rot;
        let i = k.partition_point(|(f, _)| *f <= frame);
        match (i.checked_sub(1).map(|j| k[j]), k.get(i)) {
            (Some((fa, a)), Some((fb, b))) if *fb > fa => a.slerp(*b, ((frame - fa) / (fb - fa)).clamp(0.0, 1.0)),
            (Some((_, a)), _) => a,
            (None, Some((_, b))) => *b,
            _ => Quat::IDENTITY,
        }
    }

    pub fn offset(&self, frame: f32) -> Vec3 {
        let k = &self.trans;
        let i = k.partition_point(|(f, _)| *f <= frame);
        match (i.checked_sub(1).map(|j| k[j]), k.get(i)) {
            (Some((fa, a)), Some((fb, b))) if *fb > fa => a.lerp(*b, ((frame - fa) / (fb - fa)).clamp(0.0, 1.0)),
            (Some((_, a)), _) => a,
            (None, Some((_, b))) => *b,
            _ => Vec3::ZERO,
        }
    }
}

/// Zombie animations used by the game (all from Nacht's zone).
pub const ZOMBIE_ANIMS: &[&str] = &[
    "ai_zombie_walk_v1",
    "ai_zombie_walk_v2",
    "ai_zombie_walk_v3",
    "ai_zombie_walk_v4",
    "ai_zombie_walk_fast_v1",
    "ai_zombie_walk_fast_v2",
    "ai_zombie_walk_fast_v3",
    "ai_zombie_sprint_v1",
    "ai_zombie_sprint_v2",
    "ai_zombie_attack_v1",
    "ai_zombie_attack_forward_v1",
    "ai_zombie_door_tear_high",
    "ai_zombie_door_tear_left",
    "ai_zombie_door_tear_right",
    "ai_zombie_door_tear_low",
    "ai_zombie_door_pound_v1",
    "ai_zombie_traverse_v1",
    "ai_zombie_traverse_v2",
    "ai_zombie_death_v1",
    "ai_zombie_death_v2",
    "ai_zombie_idle_v1_delta",
];

/// Weapon animation slots used (index into the weapon's anim list).
pub const VIEW_ANIM_SLOTS: &[(usize, &str)] = &[
    (1, "idle"),
    (2, "empty_idle"),
    (3, "fire"),
    (4, "hold_fire"),
    (5, "last_shot"),
    (6, "rechamber"),
    (7, "melee"),
    (9, "reload"),
    (10, "reload_empty"),
    (11, "reload_start"),
    (12, "reload_end"),
    (13, "raise"),
    (14, "first_raise"),
    (15, "drop"),
    (22, "sprint_in"),
    (23, "sprint_loop"),
    (24, "sprint_out"),
    (30, "ads_fire"),
    (33, "ads_up"),
    (34, "ads_down"),
];

/// Game-space rotation (x, y, z, w) to Bevy space.
fn rot_to_bevy(q: [f32; 4]) -> Quat {
    let g = Mat3::from_quat(Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize());
    let c = Mat3::from_cols(Vec3::X, Vec3::NEG_Z, Vec3::Y);
    Quat::from_mat3(&(c * g * c.transpose())).normalize()
}

/// The zombie body and head models (from the map's character scripts).
pub const ZOMBIE_BODIES: &[&str] = &["char_ger_honorgd_body1_1", "char_ger_honorgd_body2_1", "char_ger_honorgd_body1_2", "char_ger_honorgd_body2_2"];
pub const ZOMBIE_HEADS: &[&str] = &[
    "char_ger_honorgd_zombiehead1_1",
    "char_ger_honorgd_zombiehead2_1",
    "char_ger_honorgd_zombiehead3_1",
    "char_ger_honorgd_zombiehead4_1",
    "char_ger_honorgd_zombiehead1_2",
    "char_ger_honorgd_zombiehead2_3",
];

/// One model (LOD 0) as meshes in Bevy space, plus its bind-pose bones.
/// The mystery box: its static base, the lid script model the trigger
/// targets, and the point the weapon rises from.
#[derive(Clone)]
pub struct SceneChest {
    /// World bounds of the box model (Bevy space).
    pub bounds: (Vec3, Vec3),
    pub lid_model: String,
    pub lid: Transform,
    /// Where the weapon appears, facing as the script turns it.
    pub weapon: Transform,
}

pub struct SceneModel {
    pub surfaces: Vec<SceneMesh>,
}

pub struct NachtScene {
    pub images: HashMap<String, Image>,
    pub materials: Vec<SceneMaterial>,
    pub world: Vec<SceneWorldMesh>,
    /// Each lightmap's secondary (two halves, RGBA) and primary (shadow of
    /// each surface's primary light) pages, raw, by lightmap index.
    pub lightmap_pages: Vec<Option<(Image, Image)>>,
    /// The light grid as an irradiance volume for models, and where it sits.
    pub irradiance: Option<(Image, Transform)>,
    /// Fog and film grade from the map's own art script and vision set.
    pub fog: Option<waw_assets::look::Fog>,
    pub film: Option<waw_assets::look::Film>,
    /// Primary lights by index (0 = none, 1 = the sun).
    pub lights: Vec<SceneLight>,
    /// Brush submodels by number (`"*N"` in the entities), in local space.
    pub submodels: HashMap<usize, Vec<SceneMesh>>,
    /// Local-space bounds of each submodel (Bevy space, metres).
    pub submodel_bounds: HashMap<usize, (Vec3, Vec3)>,
    pub models: HashMap<String, SceneModel>,
    pub static_models: Vec<(String, Transform)>,
    pub sky_model: Option<String>,
    /// Scale that puts the sky model behind everything (the game draws its
    /// sky at infinity).
    pub sky_scale: f32,
    pub collision: TriMesh,
    pub map: ZombieMap,
    pub entities: Vec<mapents::Entity>,
    /// Sound aliases from the zones: name -> variants.
    pub sounds: HashMap<String, Vec<SceneSound>>,
    /// Our weapon id -> its sound fields and notetrack sounds.
    pub weapon_sounds: HashMap<String, WeaponSounds>,
    /// Our weapon id -> the game's display name ("Colt M1911").
    pub weapon_names: HashMap<String, String>,
    /// Our weapon id -> its third-person (world) model, shown by the box.
    pub weapon_world_models: HashMap<String, String>,
    /// Our weapon id -> the gameplay numbers of the zone's WeaponDef (what
    /// the game itself uses), as weapon-file key/value pairs.
    pub weapon_stats: HashMap<String, Vec<(&'static str, String)>>,
    /// Flesh penetration depths from common.ff's `info/bullet_penetration_sp`.
    pub flesh_penetration: Option<[f32; 4]>,
    /// The mystery box as the map builds it.
    pub chest: Option<SceneChest>,
    pub characters: Vec<SceneCharacter>,
    pub zombie_anims: Vec<AnimClip>,
    pub view_rig: Option<SceneViewRig>,
    /// First-person gun model per weapon id (bind pose, grip at the origin).
    pub view_models: HashMap<String, Vec<SceneMesh>>,
    /// Round, zombie and points rules from the map's scripts.
    pub rules: zm_core::rules::ZombieRules,
    pub load_secs: f32,
    /// Effects the map and its weapons use.
    pub fx: crate::fx::data::FxData,
}

/// Which zones' assets a material/model index refers to.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ZoneMat(usize, u32);

struct Builder<'a> {
    zones: Vec<&'a ZoneData>,
    iwd: &'a Iwd,
    bc: bool,
    materials: Vec<SceneMaterial>,
    mat_index: HashMap<ZoneMat, usize>,
    images: HashMap<String, Image>,
}

/// Blend mode from a technique-set name such as `wc_l_sm_t0c0n0s0`.
fn classify(techset: &str) -> (Blend, bool, bool) {
    let t = techset.trim_start_matches(',');
    // Sky-box model layers: drawn unlit and without depth (so the fog pass
    // leaves the sky alone, as the game's sky shaders have no fog).
    if t.starts_with("mc_sky") {
        return (if t.contains("_add") { Blend::Add } else { Blend::Blend }, true, false);
    }
    if t.contains("sky") || t.contains("tools") || t.contains("shadowcaster") {
        return (Blend::Opaque, true, false); // caller skips these
    }
    if t.contains("unlit") {
        return (Blend::Mask, true, false);
    }
    if t.contains("water") || t.contains("distortion") || t.contains("add") {
        return (Blend::Blend, false, false);
    }
    let first = t.split("sm_").nth(1).or_else(|| t.split("l_").nth(1)).and_then(|s| s.chars().next());
    let blend = match first {
        Some('b') => Blend::Blend,
        Some('t') => Blend::Mask,
        _ => Blend::Opaque,
    };
    (blend, false, true)
}

pub fn skip_material(techset: &str, name: &str) -> bool {
    let t = techset.trim_start_matches(',');
    // Tool textures by name: `caulk`, `clip`, `clip_player`, ... (not any
    // name containing "clip": the MG42's drum is `mtl_drumclip_mg42`).
    let base = name.rsplit('/').next().unwrap_or(name);
    (t.contains("sky") && !t.starts_with("mc_sky")) || t.contains("tools") || t.contains("shadowcaster") || t.contains("water") || base.starts_with("caulk") || base.starts_with("clip")
}

fn sampler(repeat: bool, mips: bool) -> ImageSampler {
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: if mips { 8 } else { 1 },
        ..default()
    })
}

impl<'a> Builder<'a> {
    /// An image loaded without sRGB decoding (normal maps), keyed `name#raw`.
    fn image_raw(&mut self, name: &str) -> Option<String> {
        let name = name.trim_start_matches(',').to_ascii_lowercase();
        let key = format!("{name}#raw");
        if self.images.contains_key(&key) {
            return Some(key);
        }
        let bytes = self.iwd.read_image(&name)?;
        let iwi = Iwi::parse(&bytes).ok()?;
        self.images.insert(key.clone(), crate::waw::iwi_to_image(&iwi, false, true, self.bc));
        Some(key)
    }

    fn image(&mut self, name: &str) -> Option<String> {
        let name = name.trim_start_matches(',').to_ascii_lowercase();
        if self.images.contains_key(&name) {
            return Some(name);
        }
        let bytes = self.iwd.read_image(&name)?;
        let iwi = Iwi::parse(&bytes).ok()?;
        self.images.insert(name.clone(), crate::waw::iwi_to_image(&iwi, true, true, self.bc));
        Some(name)
    }

    /// Scene material for material `idx` of zone `z`, or `None` to skip it.
    fn material(&mut self, z: usize, idx: u32) -> Option<usize> {
        let key = ZoneMat(z, idx);
        if let Some(&m) = self.mat_index.get(&key) {
            return (m != usize::MAX).then_some(m);
        }
        let zone = self.zones[z];
        let mut info = &zone.materials[idx as usize];
        // Cross-zone stubs (",name") carry no data: find the real one.
        if info.name.starts_with(',') {
            let real = info.name.trim_start_matches(',');
            if let Some(m) = self.zones.iter().flat_map(|zd| zd.materials.iter()).find(|m| m.name == real) {
                info = m;
            }
        }
        let techset = info.techset.clone().unwrap_or_default();
        if skip_material(&techset, &info.name) {
            self.mat_index.insert(key, usize::MAX);
            return None;
        }
        let (mut blend, unlit, lightmapped) = classify(&techset);
        // The material's data belongs to whichever zone it came from.
        let zi = self.zones.iter().position(|zd| zd.materials.iter().any(|m| std::ptr::eq(m, info))).unwrap_or(z);
        // Cut-outs: alpha-tested lit states, and the foliage shaders, which
        // discard by texture alpha themselves (grass, hedges, tree cards).
        let state = self.zones[zi].lit_state(info);
        let t = techset.trim_start_matches(',');
        let foliage = t.contains("foliage") || t.contains("treecanopy") || t.contains("ambient_t");
        if blend == Blend::Opaque && (foliage || state.is_some_and(|s| s.alpha_test())) {
            blend = Blend::Mask;
        }
        let two_sided = state.is_some_and(|s| s.two_sided()) || blend != Blend::Opaque;
        let color_name = info.color_map().map(|i| self.zones[zi].image_name(i).to_string());
        let color = color_name.and_then(|n| self.image(&n));
        let normal_name = info.texture(t4::NORMAL_MAP).map(|i| self.zones[zi].image_name(i).to_string());
        let normal = normal_name.and_then(|n| self.image_raw(&n));
        // Layered techsets ("..._b1c1...") blend a second layer by vertex
        // colour rather than tinting.
        let vertex_tint = !t.split('_').any(|part| part.len() >= 2 && part.as_bytes()[1] == b'1' && part.as_bytes()[0].is_ascii_lowercase());
        let m = self.materials.len();
        self.materials.push(SceneMaterial { color, blend, unlit, lightmapped, two_sided, normal, vertex_tint });
        self.mat_index.insert(key, m);
        Some(m)
    }

    fn find_model(&self, name: &str) -> Option<(usize, &'a XModelInfo)> {
        let name = name.trim_start_matches(',');
        self.zones
            .iter()
            .copied()
            .enumerate()
            .find_map(|(i, zd)| zd.xmodels.iter().find(|m| m.name == name && !m.surfs.is_empty()).map(|m| (i, m)))
    }

    /// LOD-0 surfaces of a model with skin weights; `remap` maps the
    /// model's bone indices onto the target skeleton.
    fn skinned_parts(&mut self, zi: usize, info: &XModelInfo, remap: &[u16]) -> Vec<SceneMesh> {
        let zone = self.zones[zi];
        let mut out = Vec::new();
        for s in info.lod_surfs(0) {
            let Some(mat) = info.materials.get(s).copied().flatten().and_then(|m| self.material(zi, m)) else { continue };
            let dm = decode::model_surface(zone, &info.surfs[s]);
            if dm.vertices.is_empty() || dm.triangles.is_empty() {
                continue;
            }
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, dm.vertices.iter().map(|v| to_bevy(v.pos).to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, dm.vertices.iter().map(|v| dir_to_bevy(v.normal).normalize_or_zero().to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, dm.vertices.iter().map(|v| v.uv).collect::<Vec<_>>());
            let joints: Vec<[u16; 4]> = dm.skin.iter().map(|w| w.map(|(b, _)| remap.get(b as usize).copied().unwrap_or(0))).collect();
            let weights: Vec<[f32; 4]> = dm
                .skin
                .iter()
                .map(|w| {
                    let sum: f32 = w.iter().map(|x| x.1).sum::<f32>().max(1e-4);
                    w.map(|(_, x)| x / sum)
                })
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, bevy::render::mesh::VertexAttributeValues::Uint16x4(joints));
            mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
            mesh.insert_indices(Indices::U32(dm.triangles.iter().flat_map(|t| [t[0] as u32, t[2] as u32, t[1] as u32]).collect()));
            out.push(SceneMesh { mesh, material: mat });
        }
        out
    }

    /// A model's skeleton in Bevy space (bind locals and inverse binds).
    /// Locals come from the model's own local data, not from the base
    /// pose: the viewmodel arms store zero local translations because their
    /// animations carry the full bone offsets.
    fn skeleton(info: &XModelInfo) -> Vec<SceneBone> {
        let globals: Vec<Transform> = info
            .bones
            .iter()
            .map(|b| Transform { translation: to_bevy(b.base_trans), rotation: rot_to_bevy(b.base_quat), scale: Vec3::ONE })
            .collect();
        info.bones
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let g = globals[i].compute_matrix();
                let local = match b.parent {
                    Some(p) if p < i => Transform { translation: to_bevy(b.local_trans), rotation: rot_to_bevy(b.local_quat).normalize(), scale: Vec3::ONE },
                    _ => globals[i],
                };
                SceneBone { name: b.name.to_ascii_lowercase(), parent: b.parent.filter(|&p| p < i), local, inv_bind: g.inverse() }
            })
            .collect()
    }

    /// A model skinned to its own skeleton (bones are shared by name with
    /// whatever it is attached to when spawned).
    fn skinned(&mut self, name: &str) -> Option<SkinnedPart> {
        let (zi, info) = self.find_model(name)?;
        let bones = Self::skeleton(info);
        let identity: Vec<u16> = (0..bones.len() as u16).collect();
        let meshes = self.skinned_parts(zi, info, &identity);
        (!meshes.is_empty()).then_some(SkinnedPart { bones, meshes })
    }

    /// A body model and the heads that can be attached to it.
    fn character(&mut self, body: &str, heads: &[&str]) -> Option<SceneCharacter> {
        let body = self.skinned(body)?;
        let heads = heads.iter().filter_map(|h| self.skinned(h)).collect();
        Some(SceneCharacter { body, heads })
    }

    /// A weapon's first-person model. Parts parked far from the gun in the
    /// bind pose (spare clips, rounds shown only during reloads) are dropped.
    fn view_model(&mut self, name: &str) -> Option<Vec<SceneMesh>> {
        let (zi, info) = self.find_model(name)?;
        let zone = self.zones[zi];
        let mut out = Vec::new();
        for s in info.lod_surfs(0) {
            let Some(mat) = info.materials.get(s).copied().flatten().and_then(|m| self.material(zi, m)) else { continue };
            let dm = decode::model_surface(zone, &info.surfs[s]);
            let near = |i: u16| dm.vertices.get(i as usize).is_some_and(|v| v.pos[1].abs() < 12.0 && v.pos[0] > -20.0);
            let tris: Vec<u32> = dm.triangles.iter().filter(|t| t.iter().all(|&i| near(i))).flat_map(|t| [t[0] as u32, t[2] as u32, t[1] as u32]).collect();
            if tris.is_empty() {
                continue;
            }
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, dm.vertices.iter().map(|v| to_bevy(v.pos).to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, dm.vertices.iter().map(|v| dir_to_bevy(v.normal).normalize_or_zero().to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, dm.vertices.iter().map(|v| v.uv).collect::<Vec<_>>());
            mesh.insert_indices(Indices::U32(tris));
            out.push(SceneMesh { mesh, material: mat });
        }
        (!out.is_empty()).then_some(out)
    }

    /// LOD-0 meshes of a model (searched across zones by name).
    fn model(&mut self, name: &str) -> Option<SceneModel> {
        let name = name.trim_start_matches(',');
        let (zi, info) = self
            .zones
            .iter()
            .enumerate()
            .find_map(|(i, zd)| zd.xmodels.iter().find(|m| m.name == name && !m.surfs.is_empty()).map(|m| (i, m)))?;
        let zone = self.zones[zi];
        let mut surfaces = Vec::new();
        for s in info.lod_surfs(0) {
            let Some(mat) = info.materials.get(s).copied().flatten().and_then(|m| self.material(zi, m)) else { continue };
            let dm = decode::model_surface(zone, &info.surfs[s]);
            if dm.vertices.is_empty() || dm.triangles.is_empty() {
                continue;
            }
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, dm.vertices.iter().map(|v| to_bevy(v.pos).to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, dm.vertices.iter().map(|v| dir_to_bevy(v.normal).normalize_or_zero().to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, dm.vertices.iter().map(|v| v.uv).collect::<Vec<_>>());
            mesh.insert_indices(Indices::U32(dm.triangles.iter().flat_map(|t| [t[0] as u32, t[2] as u32, t[1] as u32]).collect()));
            surfaces.push(SceneMesh { mesh, material: mat });
        }
        Some(SceneModel { surfaces })
    }
}

/// Bevy mesh from map surfaces (global vertex indices remapped per mesh).
fn world_mesh(zone: &ZoneData, w: &t4::WorldInfo, surfs: &[usize], lightmap: bool) -> Option<Mesh> {
    let mut pos = Vec::new();
    let mut nor = Vec::new();
    let mut uv0 = Vec::new();
    let mut uv1 = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut tan: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut remap: HashMap<u32, u32> = HashMap::new();
    for &si in surfs {
        let s = &w.surfaces[si];
        remap.clear();
        for tri in decode::world_triangles(zone, w, s) {
            for &v in &[tri[0], tri[2], tri[1]] {
                let i = *remap.entry(v).or_insert_with(|| {
                    let vx = decode::world_vertex(zone, w, v).unwrap_or(decode::Vertex { pos: [0.0; 3], normal: [0.0, 0.0, 1.0], tangent: [1.0, 0.0, 0.0], binormal_sign: 1.0, uv: [0.0; 2], color: [255; 4] });
                    pos.push(to_bevy(vx.pos).to_array());
                    nor.push(dir_to_bevy(vx.normal).normalize_or_zero().to_array());
                    uv0.push(vx.uv);
                    uv1.push(decode::world_lightmap_uv(zone, w, v).unwrap_or([0.0; 2]));
                    // D3DCOLOR: bytes B, G, R, A (kept as raw gamma values).
                    col.push([vx.color[2] as f32 / 255.0, vx.color[1] as f32 / 255.0, vx.color[0] as f32 / 255.0, vx.color[3] as f32 / 255.0]);
                    let t = dir_to_bevy(vx.tangent).normalize_or_zero();
                    tan.push([t.x, t.y, t.z, if vx.binormal_sign < 0.0 { -1.0 } else { 1.0 }]);
                    pos.len() as u32 - 1
                });
                idx.push(i);
            }
        }
    }
    if idx.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv0);
    if lightmap {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tan);
    }
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}


/// The map's fog and vision set, from its own data: the art script
/// `maps/createart/<map>_art.gsc` names the fog and the vision set, which
/// is `vision/<name>.vision` in any loaded zone or the IWDs.
fn map_look(zones: &[&ZoneData], iwd: &Iwd, world: &t4::WorldInfo) -> (Option<waw_assets::look::Fog>, Option<waw_assets::look::Film>) {
    use waw_assets::look;
    let base = world.name.rsplit('/').next().unwrap_or(&world.name).trim_end_matches(".d3dbsp").to_string();
    let raw = |name: &str| zones.iter().find_map(|z| z.rawfile(name)).or_else(|| iwd.read(name).map(|b| String::from_utf8_lossy(&b).into_owned()));
    let Some(script) = raw(&format!("maps/createart/{base}_art.gsc")) else {
        warn!("No art script for {base}: no fog or vision set");
        return (None, None);
    };
    let fog = look::fog_from_script(&script);
    let vision = look::vision_name_from_script(&script).and_then(|n| raw(&format!("vision/{n}.vision")).map(|t| (n, look::parse_vision(&t))));
    info!("Map look for {base}: fog {fog:?}, vision {:?}", vision.as_ref().map(|v| &v.0));
    (fog, vision.map(|v| v.1.film))
}

/// RGB9E5 shared-exponent packing (for the irradiance volume).
fn rgb9e5(c: [f32; 3]) -> u32 {
    let clamp = |v: f32| if v.is_finite() { v.clamp(0.0, 65408.0) } else { 0.0 };
    let (r, g, b) = (clamp(c[0]), clamp(c[1]), clamp(c[2]));
    let max = r.max(g).max(b);
    if max <= 0.0 {
        return 0;
    }
    let mut exp = (max.log2().floor() as i32).max(-16) + 1 + 15;
    let mut denom = 2f32.powi(exp - 15 - 9);
    if (max / denom + 0.5).floor() >= 512.0 {
        denom *= 2.0;
        exp += 1;
    }
    let q = |v: f32| ((v / denom + 0.5).floor() as u32).min(511);
    q(r) | (q(g) << 9) | (q(b) << 18) | ((exp.clamp(0, 31) as u32) << 27)
}

/// The light grid as a Bevy irradiance volume: each grid point's 4x4x4
/// block of incoming light becomes an ambient cube (the centre of each
/// face, which is what the game's model shader reads for that direction),
/// with the game's x2 model overbright, converted from gamma to linear.
/// Empty points borrow from their neighbours so models near walls don't go
/// black.
fn irradiance_volume(zone: &ZoneData, world: &t4::WorldInfo, lights: &[SceneLight], collision: &TriMesh) -> Option<(Image, Transform)> {
    let grid = world.light_grid.as_ref()?;
    let d = &zone.data;
    let curves = FalloffCurves::read(zone);
    let (mn, mx) = (grid.mins, grid.maxs);
    // Bevy axes: X = game X, Y = game Z (up), Z = -game Y.
    let (rx, ry, rz) = ((mx[0] - mn[0] + 1) as usize, (mx[2] - mn[2] + 1) as usize, (mx[1] - mn[1] + 1) as usize);
    // Per voxel: +X, -X, +Y(up), -Y, +Z(-game Y), -Z(+game Y).
    let mut faces: Vec<Option<[[f32; 3]; 6]>> = vec![None; rx * ry * rz];
    let face = |cube: &[[[[f32; 3]; 4]; 4]; 4], axis: usize, hi: bool| -> [f32; 3] {
        let k = if hi { 3 } else { 0 };
        let mut acc = [0.0f32; 3];
        for a in 1..3 {
            for b in 1..3 {
                let c = match axis {
                    0 => cube[k][a][b],
                    1 => cube[a][k][b],
                    _ => cube[a][b][k],
                };
                for i in 0..3 {
                    acc[i] += c[i] * 0.25;
                }
            }
        }
        // The game's x2 model overbright (gamma; linearised after adding the
        // primary light).
        acc.map(|v| 2.0 * v)
    };
    for z in 0..rz {
        for y in 0..ry {
            for x in 0..rx {
                let g = [mn[0] as i64 + x as i64, mx[1] as i64 - z as i64, mn[2] as i64 + y as i64];
                let Some(entry) = grid.entry_index(d, g).and_then(|i| grid.entry(d, i)) else { continue };
                let Some(cube) = grid.cube(d, entry) else { continue };
                // Game faces: +X, -X, +Y, -Y, +Z, -Z.
                let (px, nx) = (face(&cube, 0, true), face(&cube, 0, false));
                let (py, ny) = (face(&cube, 1, true), face(&cube, 1, false));
                let (pz, nz) = (face(&cube, 2, true), face(&cube, 2, false));
                // Bevy order: +X, -X, +Y, -Y, +Z, -Z (still gamma, x2 applied).
                let mut f = [px, nx, pz, nz, ny, py];
                // The point's primary light, as the game adds it per pixel to
                // models (with its falloff, cone and visibility).
                let p = to_bevy([(g[0] - 4096) as f32 * 32.0, (g[1] - 4096) as f32 * 32.0, (g[2] - 2048) as f32 * 64.0]);
                if let Some(light) = lights.get(entry.primary_light as usize) {
                    let normals = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
                    if let Some((l, color)) = primary_at(light, p, &curves, collision) {
                        for (face, n) in f.iter_mut().zip(normals) {
                            let k = n.dot(l).max(0.0);
                            for c in 0..3 {
                                face[c] += color[c] * k;
                            }
                        }
                    }
                }
                faces[(z * ry + y) * rx + x] = Some(f.map(|c| c.map(|v| v.max(0.0).powf(2.2))));
            }
        }
    }
    // Fill empty points from filled neighbours, a few layers deep.
    for _ in 0..6 {
        let prev = faces.clone();
        for z in 0..rz {
            for y in 0..ry {
                for x in 0..rx {
                    let i = (z * ry + y) * rx + x;
                    if prev[i].is_some() {
                        continue;
                    }
                    let mut acc = [[0.0f32; 3]; 6];
                    let mut n = 0.0;
                    for (dx, dy, dz) in [(-1i64, 0i64, 0i64), (1, 0, 0), (0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1)] {
                        let (nx, ny, nz) = (x as i64 + dx, y as i64 + dy, z as i64 + dz);
                        if nx < 0 || ny < 0 || nz < 0 || nx >= rx as i64 || ny >= ry as i64 || nz >= rz as i64 {
                            continue;
                        }
                        if let Some(f) = prev[(nz as usize * ry + ny as usize) * rx + nx as usize] {
                            for s in 0..6 {
                                for c in 0..3 {
                                    acc[s][c] += f[s][c];
                                }
                            }
                            n += 1.0;
                        }
                    }
                    if n > 0.0 {
                        faces[i] = Some(acc.map(|f| f.map(|v| v / n)));
                    }
                }
            }
        }
    }
    // Pack (Rx, 2Ry, 3Rz): positive sides in the first Ry rows, negative in
    // the second; X sides in the first Rz layers, then Y, then Z.
    let (w, h, depth) = (rx, 2 * ry, 3 * rz);
    let mut texels = vec![0u32; w * h * depth];
    for z in 0..rz {
        for y in 0..ry {
            for x in 0..rx {
                let f = faces[(z * ry + y) * rx + x].unwrap_or([[0.0; 3]; 6]);
                for (axis, (pos, neg)) in [(0usize, (f[0], f[1])), (1, (f[2], f[3])), (2, (f[4], f[5]))] {
                    let layer = axis * rz + z;
                    texels[(layer * h + y) * w + x] = rgb9e5(pos);
                    texels[(layer * h + ry + y) * w + x] = rgb9e5(neg);
                }
            }
        }
    }
    let bytes: Vec<u8> = texels.iter().flat_map(|t| t.to_le_bytes()).collect();
    let mut image = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: depth as u32 },
        TextureDimension::D3,
        bytes,
        TextureFormat::Rgb9e5Ufloat,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler(false, false);
    // Voxel centres on the grid points: points every 32 x 64 x 32 (Bevy x y z).
    let cell = Vec3::new(32.0, 64.0, 32.0) * INCH;
    let size = Vec3::new(rx as f32, ry as f32, rz as f32) * cell;
    let base = Vec3::new((mn[0] as f32 - 4096.0) * 32.0, (mn[2] as f32 - 2048.0) * 64.0, (4096.0 - mx[1] as f32) * 32.0) * INCH;
    let transform = Transform::from_translation(base + size * 0.5 - cell * 0.5).with_scale(size);
    info!("Light grid: {rx}x{ry}x{rz} points as an irradiance volume");
    Some((image, transform))
}

/// The light falloff curves the game keeps in row 0 of the first lightmap's
/// secondary page (linear and tungsten), read raw.
struct FalloffCurves {
    linear: Vec<[f32; 3]>,
    tungsten: Vec<[f32; 3]>,
}

impl FalloffCurves {
    fn read(zone: &ZoneData) -> FalloffCurves {
        let row = zone
            .images
            .iter()
            .find(|i| i.name.ends_with("lightmap0_secondary"))
            .and_then(|i| i.inline.clone())
            .and_then(|i| zone.data.get(i.fpos..i.fpos + 4 * i.dims[0] as usize).map(|b| b.to_vec()))
            .unwrap_or_default();
        let curve = |start: usize, width: usize| -> Vec<[f32; 3]> {
            (0..width)
                .map(|i| {
                    let o = 4 * (start + i);
                    match row.get(o..o + 4) {
                        // BGRA bytes.
                        Some(b) => [b[2] as f32 / 255.0, b[1] as f32 / 255.0, b[0] as f32 / 255.0],
                        None => [1.0 - i as f32 / (width - 1) as f32; 3],
                    }
                })
                .collect()
        };
        FalloffCurves { linear: curve(1, 16), tungsten: curve(19, 32) }
    }

    fn sample(&self, tungsten: bool, t: f32) -> [f32; 3] {
        let c = if tungsten { &self.tungsten } else { &self.linear };
        let x = t.clamp(0.0, 1.0) * (c.len() - 1) as f32;
        let (i, f) = (x.floor() as usize, x.fract());
        let (a, b) = (c[i.min(c.len() - 1)], c[(i + 1).min(c.len() - 1)]);
        [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
    }
}

/// A primary light's direction and colour (gamma, before N.L) at a point,
/// if it reaches it: the falloff curve, the spot cone, and visibility.
fn primary_at(l: &SceneLight, p: Vec3, curves: &FalloffCurves, collision: &TriMesh) -> Option<(Vec3, [f32; 3])> {
    let v = |a: Vec3| V3::new(a.x, a.y, a.z);
    match l.kind {
        1 => {
            // The sun: blocked by anything along its direction.
            let o = p + l.dir * 0.1;
            collision.raycast(v(o), v(l.dir), 400.0).is_none().then_some((l.dir, l.color.to_array()))
        }
        2 | 3 => {
            let to = l.position - p;
            let dist = to.length();
            let t = dist / l.radius;
            if t >= 1.0 || dist < 1e-3 {
                return None;
            }
            let dir = to / dist;
            let mut k = 1.0;
            if l.kind == 2 {
                let x = 1.0 / (l.cos_inner - l.cos_outer).max(1e-4);
                let s = (dir.dot(l.dir) * x - l.cos_outer * x).clamp(0.0, 1.0);
                if s <= 0.0 {
                    return None;
                }
                k = s.powf(l.exponent.max(1e-4));
            }
            if !collision.line_clear(v(p), v(l.position - dir * 0.15)) {
                return None;
            }
            let f = curves.sample(l.falloff == 1, t);
            Some((dir, [l.color.x * f[0] * k, l.color.y * f[1] * k, l.color.z * f[2] * k]))
        }
        _ => None,
    }
}

/// The primary lights in Bevy space. Index 1 is the sun as the world draws
/// it; spots and omnis come from the map's light table.
fn scene_lights(zone: &ZoneData, world: &t4::WorldInfo) -> Vec<SceneLight> {
    zone.primary_lights
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let src = if i == 1 { world.sun_light.as_ref().unwrap_or(l) } else { l };
            SceneLight {
                kind: if i == 0 { 0 } else { l.kind },
                color: Vec3::from(src.color),
                position: to_bevy(l.origin),
                radius: (l.radius * INCH).max(0.01),
                dir: dir_to_bevy(src.dir).normalize_or_zero(),
                cos_outer: l.cos_half_fov_outer,
                cos_inner: l.cos_half_fov_inner,
                exponent: l.exponent.max(1) as f32,
                falloff: if l.def_name.contains("tungsten") { 1 } else { 0 },
            }
        })
        .collect()
}

/// The lightmap pages as the shaders sample them: the secondary page
/// (BGRA bytes -> RGBA) and the primary L8 page, unfiltered by sRGB.
fn lightmap_pages(zone: &ZoneData, index: usize) -> Option<(Image, Image)> {
    let find = |suffix: &str| zone.images.iter().find(|i| i.name.ends_with(suffix)).and_then(|i| i.inline.clone());
    let primary = find(&format!("lightmap{index}_primary"))?;
    let secondary = find(&format!("lightmap{index}_secondary"))?;
    let (pw, ph) = (primary.dims[0] as usize, primary.dims[1] as usize);
    let (sw, sh) = (secondary.dims[0] as usize, secondary.dims[1] as usize);
    if primary.len < pw * ph || secondary.len < sw * sh * 4 {
        return None;
    }
    let s = &zone.data[secondary.fpos..secondary.fpos + sw * sh * 4];
    let rgba: Vec<u8> = s.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    let mut sec = Image::new(
        Extent3d { width: sw as u32, height: sh as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    sec.sampler = sampler(false, false);
    let mut pri = Image::new(
        Extent3d { width: pw as u32, height: ph as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        zone.data[primary.fpos..primary.fpos + pw * ph].to_vec(),
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    pri.sampler = sampler(false, false);
    Some((sec, pri))
}


/// One variant of a sound alias, decoded, with its authored properties.
pub struct SceneSound {
    pub wav: Vec<u8>,
    pub volume: (f32, f32),
    pub pitch: (f32, f32),
    /// Positional (3D) in the game.
    pub spatial: bool,
    /// Full volume up to `.0`, silent beyond `.1` (metres).
    pub distance: (f32, f32),
    pub secondary: Option<String>,
    pub chain: Option<String>,
    /// Length in seconds.
    pub duration: f32,
    pub start_delay: f32,
}

/// The sounds a weapon definition names.
#[derive(Clone, Default)]
pub struct WeaponSounds {
    /// WeaponDef field (`fireSoundPlayer`, `raiseSoundPlayer`, ...) -> alias.
    pub fields: HashMap<String, String>,
    /// Viewmodel notetrack -> alias.
    pub notetracks: HashMap<String, String>,
}

/// Sounds the game plays by alias name besides the configured ones.
pub const GAME_ALIASES: &[&str] = &[
    "mx_splash_screen",
    "mx_zombie_wave_1",
    "mx_game_over",
    "chalk",
    "round_over",
    "cha_ching",
    "no_cha_ching",
    "weap_wall",
    "lid_open",
    "music_box",
    "lid_close",
    "couch_slam",
    "break_boards",
    "break_stone",
    "repair_boards",
    "boards_float",
    "board_slam",
    "spawn_powerup",
    "spawn_powerup_loop",
    "powerup_grabbed",
    "full_ammo",
    "insta_kill",
    "insta_kill_loop",
    "double_point_loop",
    "points_loop_off",
    "nuke_flash",
    "nuked",
    "zombie_head_gib",
    "death_gibs",
    "heart_beat",
    "breathing_hurt",
    "breathing_better",
    "player_pain_small",
    "melee_hit",
    "amb_spooky_2d",
];

/// Weapon fields the first-person game uses.
const WEAPON_SOUND_FIELDS: &[&str] = &[
    "fireSoundPlayer",
    "fireSound",
    "fireLastSoundPlayer",
    "emptyFireSoundPlayer",
    "raiseSoundPlayer",
    "firstRaiseSoundPlayer",
    "putawaySoundPlayer",
    "reloadSoundPlayer",
    "reloadEmptySoundPlayer",
    "meleeSwipeSoundPlayer",
    "meleeHitSound",
    "pullbackSoundPlayer",
];

/// A model's collision triangles in its own (Bevy) space, if the game
/// treats it as solid. Uses the model's collision LOD (else its lowest).
fn model_collision(zones: &[&ZoneData], name: &str) -> Option<Vec<[Vec3; 3]>> {
    let name = name.trim_start_matches(',');
    let (zd, info) = zones.iter().find_map(|zd| zd.xmodels.iter().find(|m| m.name == name && !m.surfs.is_empty()).map(|m| (*zd, m)))?;
    if info.num_coll_surfs == 0 || info.lods.is_empty() {
        return None;
    }
    let lod = if info.coll_lod >= 0 { (info.coll_lod as usize).min(info.lods.len() - 1) } else { info.lods.len() - 1 };
    let mut out = Vec::new();
    for s in info.lod_surfs(lod) {
        let dm = decode::model_surface(zd, &info.surfs[s]);
        for t in &dm.triangles {
            let v = |i: u16| dm.vertices.get(i as usize).map(|v| to_bevy(v.pos));
            if let (Some(a), Some(b), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) {
                out.push([a, b, c]);
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// What a clipMap brush or terrain triangle with these contents blocks.
fn clip_blocks(contents: u32) -> u8 {
    use waw_assets::t4::clipmap::contents as c;
    let mut b = 0;
    if contents & c::PLAYER_SOLID != 0 {
        b |= blocks::PLAYER;
    }
    if contents & c::MONSTER_SOLID != 0 {
        b |= blocks::AI;
    }
    b
}

/// The map's collision as triangles in Bevy space: the static world's
/// brushes and terrain, and the brushes of scenery brush models (entities
/// with a `*N` model, placed by their origin and angles). `skip` lists the
/// brush models gameplay moves or removes (boards, doors, debris).
/// Triangles only block the player and/or AI ([`clip_blocks`]), never the
/// plain solid queries.
pub fn clip_collision(cm: &t4::clipmap::ClipMapInfo, entities: &[mapents::Entity], skip: &std::collections::HashSet<usize>) -> Vec<Tri> {
    let mut out = Vec::new();
    let v3 = |p: Vec3| V3::new(p.x, p.y, p.z);
    let add = |poly: &[Vec3], b: u8, out: &mut Vec<Tri>| {
        for k in 1..poly.len().saturating_sub(1) {
            if let Some(t) = Tri::new(v3(poly[0]), v3(poly[k]), v3(poly[k + 1])) {
                out.push(t.blocking(b));
            }
        }
    };
    let by_model = cm.model_brushes();
    let terrain = cm.model_terrain();
    let brushes = |list: &[u16], t: &Transform, out: &mut Vec<Tri>| {
        for &bi in list {
            let Some(brush) = cm.brushes.get(bi as usize) else { continue };
            let b = clip_blocks(brush.contents);
            if b == 0 {
                continue;
            }
            for f in t4::clipmap::brush_polygons(brush) {
                let poly: Vec<Vec3> = f.points.iter().map(|&p| t.transform_point(to_bevy(p))).collect();
                add(&poly, b, out);
            }
        }
    };
    brushes(&by_model[0], &Transform::IDENTITY, &mut out);
    for e in entities {
        let Some(n) = e.submodel() else { continue };
        if n == 0 || skip.contains(&n) || e.classname().starts_with("trigger") {
            continue;
        }
        let Some(list) = by_model.get(n) else { continue };
        let t = Transform::from_translation(to_bevy(e.origin())).with_rotation(angles_to_quat(e.angles()));
        brushes(list, &t, &mut out);
        // Its patches too (model space, like its brushes).
        for (tri, m) in terrain.get(n).into_iter().flatten() {
            let b = clip_blocks(cm.material_contents(*m as i64));
            if b != 0 {
                add(&tri.map(|p| t.transform_point(to_bevy(p))), b, &mut out);
            }
        }
    }
    for (tri, m) in &terrain[0] {
        let b = clip_blocks(cm.material_contents(*m as i64));
        if b != 0 {
            add(&tri.map(to_bevy), b, &mut out);
        }
    }
    out
}

/// The zone's weapon name for one of our weapon ids.
fn zone_weapon_name(id: &str) -> &str {
    match id {
        "m1911" => "zombie_colt",
        "raypistol" => "ray_gun",
        "trenchgun" => "shotgun",
        other => other,
    }
}

/// Length of a 16-bit PCM WAV in seconds.
fn wav_seconds(wav: &[u8]) -> f32 {
    let rd = |o: usize| wav.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0);
    let (channels, rate) = (wav.get(22).copied().unwrap_or(1).max(1) as f32, rd(24).max(1) as f32);
    wav.len().saturating_sub(44) as f32 / (2.0 * channels * rate)
}

/// The sound fields and notetrack map of each of our weapons.
fn weapon_sounds(zones: &[&ZoneData], ids: &[&str]) -> HashMap<String, WeaponSounds> {
    let mut out = HashMap::new();
    for &id in ids {
        let Some(w) = zones.iter().find_map(|zd| zd.weapon(zone_weapon_name(id))) else { continue };
        let fields = WEAPON_SOUND_FIELDS.iter().filter_map(|f| w.sound(f).map(|a| (f.to_string(), a.to_string()))).collect();
        let notetracks = w.notetrack_sounds.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.clone())).collect();
        out.insert(id.to_string(), WeaponSounds { fields, notetracks });
    }
    out
}

/// Turns zone sound aliases into playable PCM WAVs with their properties,
/// following secondary and chained aliases.
fn collect_sounds(zones: &[&ZoneData], iwd: &Iwd, wanted: &[String]) -> HashMap<String, Vec<SceneSound>> {
    let mut out: HashMap<String, Vec<SceneSound>> = HashMap::new();
    let mut todo: Vec<String> = wanted.to_vec();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(name) = todo.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let name = &name;
        let name = name.as_str();
        if out.contains_key(name) {
            continue;
        }
        let Some((zd, list)) = zones.iter().find_map(|zd| zd.sound(name).map(|l| (*zd, l))) else { continue };
        let mut variants = Vec::new();
        for a in &list.aliases {
            let bytes = match &a.file {
                t4::SoundFile::Loaded(i) => {
                    let own = zd.loaded_sound_bytes(*i);
                    if !own.is_empty() {
                        Some(own.to_vec())
                    } else {
                        // A reference to a sound loaded by another zone
                        // (common.ff): find it by name.
                        let wanted = zd.loaded_sounds.get(*i as usize).map(|l| l.name.trim_start_matches(',').to_string()).unwrap_or_default();
                        zones.iter().find_map(|z| {
                            z.loaded_sounds.iter().enumerate().find(|(_, l)| l.len > 0 && l.name.trim_start_matches(',') == wanted).map(|(j, _)| z.loaded_sound_bytes(j as u32).to_vec())
                        })
                    }
                }
                t4::SoundFile::Streamed { dir, name } => {
                    let path = if dir.is_empty() { format!("sound/{name}") } else { format!("sound/{}/{name}", dir.replace('\\', "/")) };
                    iwd.read(&path)
                }
                t4::SoundFile::None => None,
            };
            let decoded = bytes.and_then(|b| {
                if crate::xwma::is_xwma(&b) {
                    crate::xwma::decode(&b).map(|p| p.to_wav_bytes())
                } else {
                    zm_core::wav::to_pcm_wav(&b).ok()
                }
            });
            // Developer aid: `UNDEAD_DUMP_SOUNDS=<dir>` writes every decoded sound.
            if let (Some(w), Ok(dir)) = (&decoded, std::env::var("UNDEAD_DUMP_SOUNDS")) {
                let _ = std::fs::write(format!("{dir}/{name}_{}.wav", variants.len()), w);
            }
            if let Some(wav) = decoded {
                todo.extend(a.secondary.iter().chain(a.chain.iter()).cloned());
                let duration = wav_seconds(&wav);
                variants.push(SceneSound {
                    wav,
                    volume: (a.vol_min, a.vol_max.max(a.vol_min)),
                    pitch: (a.pitch_min.max(0.1), a.pitch_max.max(a.pitch_min).max(0.1)),
                    spatial: a.flags & 0x40 != 0,
                    distance: (a.dist_min * INCH, a.dist_max.max(a.dist_min) * INCH),
                    secondary: a.secondary.clone(),
                    chain: a.chain.clone(),
                    duration,
                    start_delay: a.start_delay,
                });
            }
        }
        if !variants.is_empty() {
            out.insert(name.to_string(), variants);
        }
    }
    out
}

/// What the game wants from the zones besides the map itself.
pub struct Wanted {
    pub aliases: Vec<String>,
    pub weapon_ids: Vec<&'static str>,
}

pub fn build(install: &Install, iwd: &Iwd, bc: bool, wanted: Wanted) -> Result<NachtScene, String> {
    let t0 = std::time::Instant::now();
    let read = |name: &str| -> Result<ZoneData, String> {
        let raw = std::fs::read(install.fastfile(name)).map_err(|e| format!("{name}.ff: {e}"))?;
        let data = waw_assets::zone::decompress(&raw).map_err(|e| format!("{name}.ff: {e}"))?;
        Ok(t4::walk(data))
    };
    let nacht = read("nazi_zombie_prototype")?;
    if !nacht.complete() {
        return Err(format!("could not read nazi_zombie_prototype.ff ({:?}, unresolved {})", nacht.stopped, nacht.unresolved));
    }
    // Shared weapons, hands and knife live in common.ff (read up to its menus).
    let common = read("common").ok();
    let mut zones: Vec<&ZoneData> = vec![&nacht];
    if let Some(c) = &common {
        zones.push(c);
    }
    // The round/zombie rules: patch.ff's scripts override the map's.
    let rules = {
        let patch = read("patch").ok();
        let mut script_zones: Vec<&ZoneData> = patch.iter().collect();
        script_zones.push(&nacht);
        zombie_rules("nazi_zombie_prototype", &script_zones, common.as_ref())
    };
    let world = nacht.world.as_ref().ok_or("map has no world geometry")?;
    let text = nacht.map_ents.as_deref().ok_or("map has no entities")?;
    let entities = mapents::parse(text);
    let map = ZombieMap::from_entities(&entities);

    let mut b = Builder { zones: zones.clone(), iwd, bc, materials: Vec::new(), mat_index: HashMap::new(), images: HashMap::new() };

    // Static world, grouped by material.
    let mut groups: HashMap<(usize, u8, u8), Vec<usize>> = HashMap::new();
    let mut collide: Vec<usize> = Vec::new();
    for i in 0..world.static_surface_count.min(world.surfaces.len() as u32) as usize {
        let s = &world.surfaces[i];
        let Some(m) = s.material.and_then(|m| b.material(0, m)) else { continue };
        groups.entry((m, s.primary_light, s.lightmap)).or_default().push(i);
        let mat = &b.materials[m];
        if mat.blend != Blend::Blend && !world.decal_range.contains(&(i as u32)) && !mat.unlit {
            collide.push(i);
        }
    }
    let mut world_meshes = Vec::new();
    for ((m, light, lmap), surfs) in &groups {
        let lit = b.materials[*m].lightmapped;
        if let Some(mesh) = world_mesh(&nacht, world, surfs, lit) {
            world_meshes.push(SceneWorldMesh { mesh, material: *m, primary_light: *light, lightmap: *lmap });
        }
    }

    // Brush submodels (window boards, doors, debris...).
    let mut submodels = HashMap::new();
    let mut submodel_bounds = HashMap::new();
    for (n, bm) in world.models.iter().enumerate().skip(1) {
        if bm.surface_count == 0 {
            // Clip-only models (e.g. the debris clip over the stairs): their
            // own bounds, so gameplay can block with them and remove them.
            let (a, b) = (to_bevy(bm.mins), to_bevy(bm.maxs));
            if bm.mins.iter().zip(&bm.maxs).all(|(lo, hi)| lo < hi) {
                submodel_bounds.insert(n, (a.min(b), a.max(b)));
            }
            continue;
        }
        let range = bm.start_surface as usize..(bm.start_surface + bm.surface_count) as usize;
        let mut by_mat: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for si in range.filter(|&i| i < world.surfaces.len()) {
            let s = &world.surfaces[si];
            for v in s.first_vertex..s.first_vertex + s.vertex_count as u32 {
                if let Some(vx) = decode::world_vertex(&nacht, world, v) {
                    let p = to_bevy(vx.pos);
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
            if let Some(m) = s.material.and_then(|m| b.material(0, m)) {
                by_mat.entry(m).or_default().push(si);
            }
        }
        let meshes: Vec<SceneMesh> =
            by_mat.into_iter().filter_map(|(m, surfs)| world_mesh(&nacht, world, &surfs, false).map(|mesh| SceneMesh { mesh, material: m })).collect();
        if !meshes.is_empty() {
            submodels.insert(n, meshes);
            submodel_bounds.insert(n, (lo, hi));
        }
    }

    // Models: static instances, script models and anything gameplay needs.
    let mut models = HashMap::new();
    let mut static_models = Vec::new();
    let c = |v: [f32; 3]| dir_to_bevy(v);
    for sm in &world.smodels {
        let Some(mi) = sm.model else { continue };
        let name = nacht.xmodels[mi as usize].name.trim_start_matches(',').to_string();
        if !models.contains_key(&name) {
            match b.model(&name) {
                Some(m) => {
                    models.insert(name.clone(), m);
                }
                None => continue,
            }
        }
        let rot = Mat3::from_cols(c(sm.axis[0]), c(sm.axis[1]), c(sm.axis[2]));
        // Columns map model-space game axes; conjugate into Bevy space.
        let conv = Mat3::from_cols(Vec3::X, Vec3::NEG_Z, Vec3::Y);
        let r = rot * conv.transpose();
        let t = Transform { translation: to_bevy(sm.origin), rotation: Quat::from_mat3(&r).normalize(), scale: Vec3::splat(sm.scale) };
        static_models.push((name, t));
    }
    let mut extra: Vec<String> = map.props.iter().map(|p| p.0.clone()).collect();
    extra.extend(map.doors.iter().flat_map(|d| d.props.iter().map(|p| p.0.clone())));
    let sky_model = world.sky_box_model.clone();
    let sky_scale = sky_model
        .as_deref()
        .and_then(|n| b.find_model(n))
        .map(|(_, info)| {
            let r = (0..3).map(|k| info.mins[k].abs().max(info.maxs[k].abs())).fold(1.0f32, f32::max) * INCH;
            // Comfortably beyond the map (the camera's far plane is infinite).
            (5000.0 / r).max(1.0)
        })
        .unwrap_or(1.0);
    extra.extend(sky_model.iter().cloned());
    for n in [
        "char_ger_honorgd_body1_1",
        "char_ger_honorgd_body2_1",
        "char_ger_honorgd_body1_2",
        "char_ger_honorgd_body2_2",
        "char_ger_honorgd_zombiehead1_1",
        "char_ger_honorgd_zombiehead2_1",
        "char_ger_honorgd_zombiehead3_1",
        "char_ger_honorgd_zombiehead4_1",
        "viewmodel_usa_marine_arms",
        "viewmodel_usa_colt45_pistol",
        "viewmodel_ger_kar98_rifle",
        "viewmodel_usa_thompson_smg",
        "viewmodel_usa_bar_lmg",
        "viewmodel_usa_m1carbine_rifle",
        "viewmodel_usa_ray_gun",
        "zombie_treasure_box",
        "zombie_treasure_box_lid",
    ] {
        extra.push(n.to_string());
    }
    for w in &nacht.weapons {
        for m in [w.view_model, w.hand_model, w.world_model, w.projectile_model].into_iter().flatten() {
            extra.push(nacht.xmodels[m as usize].name.trim_start_matches(',').to_string());
        }
    }
    for n in extra {
        if !n.is_empty() && !models.contains_key(&n) {
            if let Some(m) = b.model(&n) {
                models.insert(n, m);
            }
        }
    }

    // Collision. The map's own collision (clipMap brushes and terrain, with
    // player and monster clip) is what the player moves against; the
    // opaque render triangles stay as the plain solid geometry for bullets,
    // sight and the zombies. Models collide for everyone.
    let render_blocks = if nacht.clipmap.is_some() { blocks::SOLID } else { blocks::ALL };
    let mut tris = Vec::new();
    for &si in &collide {
        for t in decode::world_triangles(&nacht, world, &world.surfaces[si]) {
            let p: Vec<V3> = t
                .iter()
                .map(|&v| decode::world_vertex(&nacht, world, v).map(|x| to_bevy(x.pos)).unwrap_or(Vec3::ZERO))
                .map(|v| V3::new(v.x, v.y, v.z))
                .collect();
            if let Some(tri) = Tri::new(p[0], p[1], p[2]) {
                tris.push(tri.blocking(render_blocks));
            }
        }
    }
    // Props: static and script models the game treats as solid (they have
    // collision surfaces; foliage and wires don't), collided with through
    // the LOD the game uses for collision.
    let mut prop_tris: HashMap<String, Option<Vec<[Vec3; 3]>>> = HashMap::new();
    let mut add_model = |name: &str, t: &Transform, tris: &mut Vec<Tri>| {
        let local = prop_tris.entry(name.to_string()).or_insert_with(|| model_collision(&zones, name));
        for tri in local.iter().flatten() {
            let p = tri.map(|v| t.transform_point(v)).map(|v| V3::new(v.x, v.y, v.z));
            if let Some(tri) = Tri::new(p[0], p[1], p[2]) {
                tris.push(tri);
            }
        }
    };
    for (name, t) in &static_models {
        add_model(name, t, &mut tris);
    }
    for e in entities.iter().filter(|e| e.classname() == "script_model" && !e.targetname().starts_with("upstairs_blocker")) {
        if let Some(model) = e.get("model") {
            let t = Transform::from_translation(to_bevy(e.origin())).with_rotation(angles_to_quat(e.angles()));
            add_model(model, &t, &mut tris);
        }
    }
    // Scenery brush models (not boards or debris, which come and go).
    let gameplay: std::collections::HashSet<usize> =
        map.windows.iter().flat_map(|w| w.boards.iter().map(|b| b.submodel)).chain(map.doors.iter().flat_map(|d| d.blockers.iter().copied())).collect();
    for e in &entities {
        let Some(n) = e.submodel() else { continue };
        if gameplay.contains(&n) || e.classname().starts_with("trigger") {
            continue;
        }
        let Some(bm) = world.models.get(n) else { continue };
        let t = Transform::from_translation(to_bevy(e.origin())).with_rotation(angles_to_quat(e.angles()));
        for si in bm.start_surface as usize..(bm.start_surface + bm.surface_count) as usize {
            let Some(surf) = world.surfaces.get(si) else { continue };
            let solid = surf.material.and_then(|m| b.material(0, m)).is_some_and(|m| b.materials[m].blend != Blend::Blend && !b.materials[m].unlit);
            if !solid {
                continue;
            }
            for tri in decode::world_triangles(&nacht, world, surf) {
                let p: Vec<V3> = tri
                    .iter()
                    .map(|&v| decode::world_vertex(&nacht, world, v).map(|x| t.transform_point(to_bevy(x.pos))).unwrap_or(Vec3::ZERO))
                    .map(|v| V3::new(v.x, v.y, v.z))
                    .collect();
                if let Some(tri) = Tri::new(p[0], p[1], p[2]) {
                    tris.push(tri.blocking(render_blocks));
                }
            }
        }
    }
    if let Some(cm) = &nacht.clipmap {
        let clip = clip_collision(cm, &entities, &gameplay);
        info!("clipMap {}: {} brushes, {} collision triangles", cm.name, cm.brushes.len(), clip.len());
        tris.extend(clip);
    }
    let collision = TriMesh::new(tris, 2.0);

    // The mystery box: trigger -> lid script model -> weapon script origin
    // (as the box script walks them), with the box model next to it.
    let chest = entities.iter().find(|e| e.targetname() == "treasure_chest_use").and_then(|trig| {
        let lid = entities.iter().find(|e| !trig.target().is_empty() && e.targetname() == trig.target())?;
        let org = entities.iter().find(|e| !lid.target().is_empty() && e.targetname() == lid.target())?;
        let lid_t = Transform::from_translation(to_bevy(lid.origin())).with_rotation(angles_to_quat(lid.angles()));
        let a = org.angles();
        let weapon = Transform::from_translation(to_bevy(org.origin())).with_rotation(angles_to_quat([a[0], a[1] + 90.0, a[2]]));
        // Bounds from the nearest box model instance.
        let near = static_models
            .iter()
            .filter(|(n, _)| n == "zombie_treasure_box")
            .min_by(|a, b| a.1.translation.distance(lid_t.translation).total_cmp(&b.1.translation.distance(lid_t.translation)))
            .and_then(|(n, t)| b.find_model(n).map(|(_, info)| (info.mins, info.maxs, *t)));
        let bounds = match near {
            Some((lo, hi, t)) => {
                let mut mn = Vec3::splat(f32::MAX);
                let mut mx = Vec3::splat(f32::MIN);
                for i in 0..8 {
                    let c = [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
                    let p = t.transform_point(to_bevy(c));
                    mn = mn.min(p);
                    mx = mx.max(p);
                }
                (mn, mx)
            }
            None => (lid_t.translation - Vec3::new(0.6, 0.5, 0.6), lid_t.translation + Vec3::new(0.6, 0.1, 0.6)),
        };
        Some(SceneChest { bounds, lid_model: lid.get("model")?.to_string(), lid: lid_t, weapon })
    });

    let lightmap_pages = (0..world.surfaces.iter().map(|s| s.lightmap as usize + 1).max().unwrap_or(1)).map(|i| lightmap_pages(&nacht, i)).collect();
    let lights = scene_lights(&nacht, world);
    let irradiance = irradiance_volume(&nacht, world, &lights, &collision);
    let (fog, film) = map_look(&zones, iwd, world);
    let mut view_models = HashMap::new();
    for &id in &wanted.weapon_ids {
        let zname = zone_weapon_name(id);
        let model = zones.iter().find_map(|zd| zd.weapon(zname).and_then(|w| w.view_model).map(|m| zd.xmodels[m as usize].name.clone()));
        if let Some(parts) = model.and_then(|m| b.view_model(&m)) {
            view_models.insert(id.to_string(), parts);
        }
    }
    // Animations: decoded from whichever zone has them (common.ff for some).
    let clip = |name: &str| -> Option<AnimClip> {
        zones.iter().find_map(|zd| zd.xanims.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| (*zd, a))).and_then(|(zd, a)| {
            waw_assets::t4::anim::decode(zd, a).map(|c| AnimClip::from_clip(&c)).map_err(|e| eprintln!("anim {name}: {e}")).ok()
        })
    };
    let zombie_anims: Vec<AnimClip> = ZOMBIE_ANIMS.iter().filter_map(|n| clip(n)).collect();
    let view_rig = b.skinned("viewmodel_usa_marine_arms").map(|arms| {
        let mut guns = HashMap::new();
        let mut anims = HashMap::new();
        for &id in &wanted.weapon_ids {
            let Some(w) = zones.iter().find_map(|zd| zd.weapon(zone_weapon_name(id)).map(|w| (*zd, w))) else { continue };
            if let Some(gun) = w.1.view_model.map(|m| w.0.xmodels[m as usize].name.clone()).and_then(|m| b.skinned(&m)) {
                guns.insert(id.to_string(), gun);
            }
            let slots: Vec<(&'static str, AnimClip)> = VIEW_ANIM_SLOTS
                .iter()
                .filter_map(|(i, slot)| w.1.xanims.get(*i).filter(|n| !n.is_empty()).and_then(|n| clip(n)).map(|c| (*slot, c)))
                .collect();
            anims.insert(id.to_string(), slots);
        }
        // The knife (all weapons share it) lives in common.ff.
        if let Some(knife) = clip("viewmodel_knife_slash") {
            anims.entry("__knife".to_string()).or_insert_with(Vec::new).push(("melee", knife));
        }
        SceneViewRig { arms, guns, anims }
    });
    let characters: Vec<SceneCharacter> = ZOMBIE_BODIES.iter().filter_map(|body| b.character(body, ZOMBIE_HEADS)).collect();
    // Sounds: configured, scripted, weapon fields and notetracks, zombie
    // animation notetracks (`sndnt#alias`) and the map's ambient emitters.
    let weapon_sounds = weapon_sounds(&zones, &wanted.weapon_ids);
    let mut aliases: Vec<String> = wanted.aliases.clone();
    aliases.extend(GAME_ALIASES.iter().map(|a| a.to_string()));
    for ws in weapon_sounds.values() {
        aliases.extend(ws.fields.values().cloned());
        aliases.extend(ws.notetracks.values().cloned());
    }
    // Viewmodel notetracks that name an alias directly (anims of weapons
    // without a notetrack map, e.g. the knife lunge).
    if let Some(vr) = &view_rig {
        aliases.extend(vr.anims.values().flatten().flat_map(|(_, c)| c.notify.iter().map(|n| n.0.to_ascii_lowercase())));
    }
    aliases.extend(zombie_anims.iter().flat_map(|c| c.notify.iter().filter_map(|n| n.0.strip_prefix("sndnt#").map(str::to_string))));
    aliases.extend(map.ambient.iter().map(|a| a.alias.clone()));
    aliases.sort();
    aliases.dedup();
    let weapon_zone_names: Vec<(&str, &str)> = wanted.weapon_ids.iter().map(|&id| (id, zone_weapon_name(id))).collect();
    let fx = crate::fx::data::extract(&zones, iwd, bc, &weapon_zone_names);
    aliases.extend(fx.sound_aliases.iter().cloned());
    let sounds = collect_sounds(&zones, iwd, &aliases);
    let localized = |key: &str| zones.iter().find_map(|z| z.localized(key).or_else(|| z.localized(key.trim_start_matches('&'))).map(str::to_string));
    let weapon_world_models: HashMap<String, String> = wanted
        .weapon_ids
        .iter()
        .filter_map(|&id| {
            let (zd, w) = zones.iter().find_map(|z| z.weapon(zone_weapon_name(id)).map(|w| (*z, w)))?;
            let name = zd.xmodels[w.world_model? as usize].name.trim_start_matches(',').to_string();
            Some((id.to_string(), name))
        })
        .collect();
    for name in weapon_world_models.values().chain(fx.models.iter()) {
        if !models.contains_key(name) {
            if let Some(m) = b.model(name) {
                models.insert(name.clone(), m);
            }
        }
    }
    let weapon_names = wanted
        .weapon_ids
        .iter()
        .filter_map(|&id| {
            let w = zones.iter().find_map(|z| z.weapon(zone_weapon_name(id)))?;
            localized(&w.display_name).filter(|n| !n.trim().is_empty()).map(|n| (id.to_string(), n))
        })
        .collect();
    let weapon_stats = wanted
        .weapon_ids
        .iter()
        .filter_map(|&id| {
            let w = zones.iter().find_map(|z| z.weapon(zone_weapon_name(id)))?;
            (!w.stats.is_empty()).then(|| (id.to_string(), w.stats.clone()))
        })
        .collect();
    let flesh_penetration = zones.iter().find_map(|z| z.rawfile("info/bullet_penetration_sp")).and_then(|t| zm_core::weapons::parse_penetration_table(&t));
    // One line per weapon, so a weapon the map's zones lack shows up.
    for &id in &wanted.weapon_ids {
        let rig = view_rig.as_ref();
        info!(
            "weapon {id} ({}): rig gun {}, anims {}, view model {} ({}), sounds {}, world model {}",
            zone_weapon_name(id),
            rig.is_some_and(|r| r.guns.contains_key(id)),
            rig.and_then(|r| r.anims.get(id)).map_or(0, Vec::len),
            view_models.contains_key(id),
            zones.iter().find_map(|zd| zd.weapon(zone_weapon_name(id)).and_then(|w| w.view_model).map(|m| zd.xmodels[m as usize].name.clone())).unwrap_or_default(),
            weapon_sounds.get(id).map_or(0, |s| s.fields.len() + s.notetracks.len()),
            weapon_world_models.get(id).map_or("-", String::as_str),
        );
    }
    let images = std::mem::take(&mut b.images);
    let materials = std::mem::take(&mut b.materials);
    drop(b);
    Ok(NachtScene {
        images,
        lightmap_pages,
        irradiance,
        fog,
        film,
        lights,
        materials,
        world: world_meshes,
        submodels,
        submodel_bounds,
        models,
        static_models,
        sky_model,
        sky_scale,
        collision,
        map,
        entities,
        sounds,
        weapon_sounds,
        weapon_names,
        weapon_world_models,
        weapon_stats,
        flesh_penetration,
        chest,
        characters,
        zombie_anims,
        view_rig,
        view_models,
        rules,
        load_secs: t0.elapsed().as_secs_f32(),
        fx,
    })
}

/// The map's zombie rules from its own data: the `set_zombie_var` calls of
/// the zombie mode script its level script runs (and of the power-up
/// script), with `mp/zombiemode.csv` overrides, and the power-ups it
/// includes. `script_zones` are searched in order (patch first).
fn zombie_rules(map: &str, script_zones: &[&ZoneData], common: Option<&ZoneData>) -> zm_core::rules::ZombieRules {
    use zm_core::rules::{self, ZombieRules};
    let raw = |name: &str| script_zones.iter().find_map(|z| z.rawfile(name));
    let mut out = ZombieRules::nacht();
    let Some(level) = raw(&format!("maps/{map}.gsc")) else {
        warn!("No level script for {map}: default zombie rules");
        return out;
    };
    let mode = rules::zombiemode_script_name(&level).unwrap_or_else(|| "maps/_zombiemode.gsc".into());
    let mut text = raw(&mode).unwrap_or_default();
    text.push_str(&raw("maps/_zombiemode_powerups.gsc").unwrap_or_default());
    let calls = rules::parse_zombie_vars(&text);
    let table = common.and_then(|c| c.string_table("mp/zombiemode.csv"));
    let vars = rules::resolve_zombie_vars(&calls, &|k: &str| table.and_then(|t| t.lookup(0, k, 1)).map(str::to_string));
    out.apply_vars(&vars);
    let powerups = rules::included_powerups(&level);
    if !powerups.is_empty() {
        out.powerups = powerups;
    }
    info!(
        "Zombie rules for {map}: {} vars from {mode} (+ zombiemode.csv: {}), health {} +{} then x{}, spawn delay {}, max ai {}, power-ups {:?}",
        vars.len(),
        table.is_some(),
        out.health_start,
        out.health_increase,
        1.0 + out.health_increase_percent,
        out.spawn_delay_start,
        out.max_ai,
        out.powerups
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn techset_blend_modes() {
        assert_eq!(classify("wc_l_sm_r0c0n0s0_sco").0, Blend::Opaque);
        assert_eq!(classify(",wc_l_sm_b0c0").0, Blend::Blend);
        assert_eq!(classify("wc_l_sm_t0c0n0s0").0, Blend::Mask);
        assert_eq!(classify("l_sm_r0c0n0s0_sco_b1c1").0, Blend::Opaque);
        assert!(classify("wc_unlit").1);
        assert!(skip_material("wc_sky", "wc/sky_mak1"));
        // Sky-box model layers draw (unlit, without depth); world sky doesn't.
        assert!(!skip_material("mc_sky_noncubemap", "mc/mtl_skybox_zombie"));
        assert_eq!(classify("mc_sky_noncubemap_add").0, Blend::Add);
        assert_eq!(classify("mc_sky_noncubemap").0, Blend::Blend);
        assert!(skip_material("wc_tools", "wc/caulk_shadow"));
        assert!(skip_material("wc_l_sm_r0c0", "wc/clip_player"));
        assert!(!skip_material("mc_cooktorrance_sm", "mc/mtl_drumclip_mg42"));
    }


    /// Real data: the player's collision from Nacht's clipMap carries the
    /// player up the stairs to the help room as a continuous ramp.
    /// `UNDEAD_WAW=<install> cargo test -p zm_game -- --ignored`.
    #[test]
    #[ignore]
    fn nacht_stairs_climb_smoothly() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let ff = std::fs::read(std::path::Path::new(&root).join("zone/english/nazi_zombie_prototype.ff")).unwrap();
        let zd = t4::walk(waw_assets::zone::decompress(&ff).unwrap());
        let entities = mapents::parse(zd.map_ents.as_deref().unwrap());
        let tris = clip_collision(zd.clipmap.as_ref().unwrap(), &entities, &Default::default());
        assert!(tris.len() > 40_000, "{}", tris.len());
        let mesh = TriMesh::new(tris, 2.0);
        // Plain queries don't see clip-only collision.
        assert!(mesh.ground(to_bevy([180.0, 1060.0, 0.0]).x, to_bevy([180.0, 1060.0, 0.0]).z, 1.0, -1.0, 0.7).is_none());
        let (r, step) = (15.0 * INCH, 18.0 * INCH);
        let mut feet = to_bevy([0.0, 0.0, 1.0]).y;
        let mut max_rise = 0.0f32;
        let mut x = 180.0;
        while x > -150.0 {
            let p = to_bevy([x, 1060.0, 0.0]);
            let g = mesh.support_sphere_mask(p.x, p.z, r, feet + step, feet - step, 0.7, blocks::PLAYER).expect("ground");
            // (The top tread is one unit above the upper floor.)
            assert!(g >= feet - 1.1 * INCH, "dropped at x={x}");
            max_rise = max_rise.max(g - feet);
            feet = g;
            x -= 0.5;
        }
        // Up 144 units onto the upper floor; a ray would rise 6 units at
        // once at every riser, the ball at most ~1.5 per half unit moved.
        assert!((feet / INCH - 144.0).abs() < 1.5, "{}", feet / INCH);
        assert!(max_rise / INCH < 1.6, "{}", max_rise / INCH);
    }

    #[test]
    #[ignore]
    fn light_grid_stats() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let ff = std::fs::read(std::path::Path::new(&root).join("zone/english/nazi_zombie_prototype.ff")).unwrap();
        let zd = t4::walk(waw_assets::zone::decompress(&ff).unwrap());
        let w = zd.world.as_ref().unwrap();
        let g = w.light_grid.as_ref().unwrap();
        let (mut n, mut nt, mut dark, mut dark_t, mut sum, mut sum_t) = (0, 0, 0, 0, 0.0f64, 0.0f64);
        for x in g.mins[0]..=g.maxs[0] {
            for y in g.mins[1]..=g.maxs[1] {
                for z in g.mins[2]..=g.maxs[2] {
                    let Some(e) = g.entry_index(&zd.data, [x as i64, y as i64, z as i64]).and_then(|i| g.entry(&zd.data, i)) else { continue };
                    let Some(c) = g.cube(&zd.data, e) else { continue };
                    let mut m = 0.0f64;
                    let mut k = 0;
                    for a in 0..4 { for b in 0..4 { for d in 0..4 { let v = c[a][b][d]; if v != [0.0; 3] { m += (v[0] + v[1] + v[2]) as f64 / 3.0; k += 1; } } } }
                    let m = m / k.max(1) as f64;
                    n += 1;
                    sum += m;
                    if m < 0.02 { dark += 1; }
                    if e.needs_trace { nt += 1; sum_t += m; if m < 0.02 { dark_t += 1; } }
                }
            }
        }
        println!("entries {n}, needs_trace {nt}; mean {:.3} (trace {:.3}); near-black {dark} (of which trace {dark_t})", sum / n as f64, sum_t / nt.max(1) as f64);
    }

    #[test]
    fn coordinate_conversion() {
        // Game +X forward stays +X; game +Y (left) becomes -Z; +Z up becomes +Y.
        assert_eq!(to_bevy([100.0, 0.0, 0.0]), Vec3::new(2.54, 0.0, 0.0));
        assert_eq!(dir_to_bevy([0.0, 1.0, 0.0]), Vec3::new(0.0, 0.0, -1.0));
        let q = angles_to_quat([0.0, 90.0, 0.0]);
        assert!((q * Vec3::X - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5);
    }
}

