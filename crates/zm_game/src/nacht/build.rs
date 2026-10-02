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
use zm_core::trimesh::{Tri, TriMesh};

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
}

#[derive(Debug, Clone)]
pub struct SceneMaterial {
    pub color: Option<String>,
    pub blend: Blend,
    pub unlit: bool,
    pub lightmapped: bool,
    /// Drawn without back-face culling.
    pub two_sided: bool,
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
    pub lightmap: Option<Image>,
    pub materials: Vec<SceneMaterial>,
    pub world: Vec<SceneMesh>,
    /// Brush submodels by number (`"*N"` in the entities), in local space.
    pub submodels: HashMap<usize, Vec<SceneMesh>>,
    /// Local-space bounds of each submodel (Bevy space, metres).
    pub submodel_bounds: HashMap<usize, (Vec3, Vec3)>,
    pub models: HashMap<String, SceneModel>,
    pub static_models: Vec<(String, Transform)>,
    pub sky_model: Option<String>,
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
    /// The mystery box as the map builds it.
    pub chest: Option<SceneChest>,
    pub characters: Vec<SceneCharacter>,
    pub zombie_anims: Vec<AnimClip>,
    pub view_rig: Option<SceneViewRig>,
    /// First-person gun model per weapon id (bind pose, grip at the origin).
    pub view_models: HashMap<String, Vec<SceneMesh>>,
    pub load_secs: f32,
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
    t.contains("sky") || t.contains("tools") || t.contains("shadowcaster") || t.contains("water") || name.contains("caulk") || name.contains("clip")
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
        let m = self.materials.len();
        self.materials.push(SceneMaterial { color, blend, unlit, lightmapped, two_sided });
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
    let mut idx: Vec<u32> = Vec::new();
    let mut remap: HashMap<u32, u32> = HashMap::new();
    for &si in surfs {
        let s = &w.surfaces[si];
        remap.clear();
        for tri in decode::world_triangles(zone, w, s) {
            for &v in &[tri[0], tri[2], tri[1]] {
                let i = *remap.entry(v).or_insert_with(|| {
                    let vx = decode::world_vertex(zone, w, v).unwrap_or(decode::Vertex { pos: [0.0; 3], normal: [0.0, 0.0, 1.0], uv: [0.0; 2], color: [255; 4] });
                    pos.push(to_bevy(vx.pos).to_array());
                    nor.push(dir_to_bevy(vx.normal).normalize_or_zero().to_array());
                    uv0.push(vx.uv);
                    uv1.push(decode::world_lightmap_uv(zone, w, v).unwrap_or([0.0; 2]));
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
    }
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}

/// f32 to IEEE binary16 bits (round toward zero; fine for light values).
fn f16_bits(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp <= 0 {
        sign
    } else if exp >= 31 {
        sign | 0x7bff
    } else {
        sign | ((exp as u16) << 10) | (man >> 13) as u16
    }
}

/// Bakes the map's two lightmap pages into one HDR lightmap: the secondary
/// page's two halves hold indirect colour, the primary page masks the sun.
fn bake_lightmap(zone: &ZoneData, sun: Vec3) -> Option<Image> {
    let find = |suffix: &str| zone.images.iter().find(|i| i.name.ends_with(suffix)).and_then(|i| i.inline.clone());
    let primary = find("lightmap0_primary")?;
    let secondary = find("lightmap0_secondary")?;
    let (pw, ph) = (primary.dims[0] as usize, primary.dims[1] as usize);
    let (sw, sh) = (secondary.dims[0] as usize, secondary.dims[1] as usize);
    if primary.len < pw * ph || secondary.len < sw * sh * 4 || sh < 2 {
        return None;
    }
    let p = &zone.data[primary.fpos..primary.fpos + pw * ph];
    let s = &zone.data[secondary.fpos..secondary.fpos + sw * sh * 4];
    let half = sh / 2;
    let mut out = Vec::with_capacity(pw * ph * 8);
    for y in 0..ph {
        for x in 0..pw {
            let sx = x * sw / pw;
            let sy = y * half / ph;
            let px = |row: usize| {
                let o = (row * sw + sx) * 4;
                Vec3::new(s[o + 2] as f32, s[o + 1] as f32, s[o] as f32) / 255.0
            };
            let indirect = (px(sy) + px(sy + half)) * 0.5;
            let shadow = p[y * pw + x] as f32 / 255.0;
            let c = indirect * 2.0 + sun * shadow;
            for v in [c.x, c.y, c.z, 1.0] {
                out.extend_from_slice(&f16_bits(v).to_le_bytes());
            }
        }
    }
    let mut img = Image::new(
        Extent3d { width: pw as u32, height: ph as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        out,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = sampler(false, false);
    Some(img)
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
    let world = nacht.world.as_ref().ok_or("map has no world geometry")?;
    let text = nacht.map_ents.as_deref().ok_or("map has no entities")?;
    let entities = mapents::parse(text);
    let map = ZombieMap::from_entities(&entities);
    let worldspawn = entities.iter().find(|e| e.classname() == "worldspawn");
    let sun_color = worldspawn.and_then(|e| e.vec3("suncolor")).unwrap_or([0.6, 0.7, 1.0]);
    let sun_light = worldspawn.and_then(|e| e.f32("sunlight")).unwrap_or(0.75);

    let mut b = Builder { zones: zones.clone(), iwd, bc, materials: Vec::new(), mat_index: HashMap::new(), images: HashMap::new() };

    // Static world, grouped by material.
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut collide: Vec<usize> = Vec::new();
    for i in 0..world.static_surface_count.min(world.surfaces.len() as u32) as usize {
        let s = &world.surfaces[i];
        let Some(m) = s.material.and_then(|m| b.material(0, m)) else { continue };
        groups.entry(m).or_default().push(i);
        let mat = &b.materials[m];
        if mat.blend != Blend::Blend && !world.decal_range.contains(&(i as u32)) && !mat.unlit {
            collide.push(i);
        }
    }
    let mut world_meshes = Vec::new();
    for (m, surfs) in &groups {
        let lit = b.materials[*m].lightmapped;
        if let Some(mesh) = world_mesh(&nacht, world, surfs, lit) {
            world_meshes.push(SceneMesh { mesh, material: *m });
        }
    }

    // Brush submodels (window boards, doors, debris...).
    let mut submodels = HashMap::new();
    let mut submodel_bounds = HashMap::new();
    for (n, bm) in world.models.iter().enumerate().skip(1) {
        if bm.surface_count == 0 {
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
        for m in [w.view_model, w.hand_model, w.world_model].into_iter().flatten() {
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

    // Collision: static world triangles that render opaque.
    let mut tris = Vec::new();
    for &si in &collide {
        for t in decode::world_triangles(&nacht, world, &world.surfaces[si]) {
            let p: Vec<V3> = t
                .iter()
                .map(|&v| decode::world_vertex(&nacht, world, v).map(|x| to_bevy(x.pos)).unwrap_or(Vec3::ZERO))
                .map(|v| V3::new(v.x, v.y, v.z))
                .collect();
            if let Some(tri) = Tri::new(p[0], p[1], p[2]) {
                tris.push(tri);
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
                    tris.push(tri);
                }
            }
        }
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

    let sun = Vec3::from(sun_color) * sun_light * 0.6;
    let lightmap = bake_lightmap(&nacht, sun);
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
    for name in weapon_world_models.values() {
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
    let images = std::mem::take(&mut b.images);
    let materials = std::mem::take(&mut b.materials);
    drop(b);
    Ok(NachtScene {
        images,
        lightmap,
        materials,
        world: world_meshes,
        submodels,
        submodel_bounds,
        models,
        static_models,
        sky_model,
        collision,
        map,
        entities,
        sounds,
        weapon_sounds,
        weapon_names,
        weapon_world_models,
        chest,
        characters,
        zombie_anims,
        view_rig,
        view_models,
        load_secs: t0.elapsed().as_secs_f32(),
    })
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
        assert!(skip_material("wc_tools", "wc/caulk_shadow"));
    }

    #[test]
    fn half_floats() {
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(0.0), 0);
        assert_eq!(waw_assets::t4::decode::half(f16_bits(2.75)), 2.75);
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
