//! World at War's effects, played from the game's own effect definitions
//! (`FxEffectDef`, see `research/vfx/WAW_FX.md`).
//!
//! An effect instance spawns particles for each of its elements on the
//! effect's schedule (one-shot counts, looping intervals, spawn delays and
//! lifetimes), moves them along their velocity graphs with gravity, and
//! draws them with their colour/alpha/size/rotation curves and atlas
//! frames. Particles of one material in one instance are a single dynamic
//! mesh of camera-facing quads; lights, models, sounds, decals and child
//! effects (runners, on-death effects) are handled per particle.
//!
//! Simulation runs in game space (inches, Z up) relative to the effect's
//! frame; vertices are written in Bevy space.
//!
//! Gameplay asks for effects with [`FxEvent`].

pub mod data;
pub mod material;

use crate::{Dynamic, GameState, SessionEntity};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::primitives::Aabb;
use bevy::render::view::VisibilitySystems;
use bevy::transform::TransformSystem;
use data::{Blend, EffectDef, FxData, Visual};
use material::{FxParams, FxParticleMaterial};
use std::collections::HashMap;
use std::sync::Arc;
use waw_assets::t4::fx::{elem_flags as ef, elem_type as et, Atlas, Elem};

const INCH: f32 = 0.0254;
/// `g_gravity`: effect gravity is a multiple of it (inches/s²).
const GRAVITY: f32 = 800.0;
/// Most particles one instance keeps alive.
const MAX_PARTICLES: usize = 600;
/// How long decals stay (seconds) and how many the world keeps.
const DECAL_LIFE: f32 = 30.0;
const MAX_DECALS: usize = 96;
/// Brightness of lit particles (smoke, dust) relative to unlit ones: the
/// game lights them from the light grid; Nacht at night is dark.
const LIT_AMBIENT: f32 = 0.35;
/// Bevy point light intensity (lumens) for a game light of colour strength
/// `c` and radius `r` (metres). The game adds `colour * (1 - d / r) * N.L`;
/// a Bevy light gives `I / (4 pi^2 d^2) * exposure` on a diffuse surface, so
/// the two are matched at `d` = a quarter of the radius (0.3..1.5 m; inverse
/// square is brighter nearer, dimmer further). Real maps use a fixed
/// exposure ([`crate::nacht::REAL_MAP_EV`]).
/// Most effect lights lit at once. Every fire or lamp particle carries its
/// own light; Bevy assigns each point light to screen tiles, and hundreds of
/// them crowded the light-grid volume out of tiles (models went black in
/// screen-aligned blocks, flickering as the camera moved). The game applies
/// only a few dynamic lights too. The strongest ones near the camera win.
const MAX_FX_LIGHTS: usize = 8;

/// Light requests from this frame's particles and the pool that shows them.
#[derive(Resource, Default)]
struct FxLights {
    pool: Vec<Entity>,
    requests: Vec<LightRequest>,
}

struct LightRequest {
    pos: Vec3,
    color: LinearRgba,
    intensity: f32,
    range: f32,
}

#[derive(Component)]
struct FxLight;

pub fn light_intensity(c: f32, r: f32) -> f32 {
    let d = (r * 0.25).clamp(0.3, 1.5).min(r * 0.9);
    let falloff = (1.0 - d / r.max(1e-3)).max(0.0);
    let inv_exposure = 1.2 * 2f32.powf(crate::nacht::REAL_MAP_EV);
    4.0 * std::f32::consts::PI * std::f32::consts::PI * d * d * falloff * c * inv_exposure
}

/// Game point (inches, Z up) to Bevy (metres, Y up).
pub fn to_bevy(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.z, -p.y) * INCH
}
fn dir_to_bevy(d: Vec3) -> Vec3 {
    Vec3::new(d.x, d.z, -d.y)
}
pub fn to_game(p: Vec3) -> Vec3 {
    Vec3::new(p.x, -p.z, p.y) / INCH
}
fn dir_to_game(d: Vec3) -> Vec3 {
    Vec3::new(d.x, -d.z, d.y)
}

/// Axes (forward, left, up) for game angles in degrees (`AnglesToForward`...).
pub fn angles_to_axes(a: [f32; 3]) -> Mat3 {
    let (sp, cp) = a[0].to_radians().sin_cos();
    let (sy, cy) = a[1].to_radians().sin_cos();
    let (sr, cr) = a[2].to_radians().sin_cos();
    let fwd = Vec3::new(cp * cy, cp * sy, -sp);
    let right = Vec3::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp);
    let up = Vec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    Mat3::from_cols(fwd, -right, up)
}

/// Axes from a forward direction (and optional up), as `PlayFX` builds them.
pub fn axes_from_forward(fwd: Vec3, up: Option<Vec3>) -> Mat3 {
    let f = fwd.normalize_or(Vec3::X);
    let mut u = up.unwrap_or(if f.z.abs() > 0.99 { Vec3::X } else { Vec3::Z });
    u = (u - f * f.dot(u)).normalize_or(f.any_orthonormal_vector());
    Mat3::from_cols(f, u.cross(f), u)
}

/// An effect's position and orientation in game space.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: Vec3,
    pub axes: Mat3,
}

impl Frame {
    /// From a Bevy transform: game-space axes of the entity's local game
    /// axes (bones and models map game X, Y, Z to Bevy X, -Z, Y).
    pub fn from_global(t: &GlobalTransform) -> Frame {
        let m = t.affine();
        let col = |v: Vec3| dir_to_game(Vec3::from(m.matrix3 * bevy::math::Vec3A::from(v))).normalize_or_zero();
        Frame { origin: to_game(Vec3::from(m.translation)), axes: Mat3::from_cols(col(Vec3::X), col(-Vec3::Z), col(Vec3::Y)) }
    }
    /// From a Bevy position and a Bevy forward direction.
    pub fn at(pos: Vec3, forward: Vec3) -> Frame {
        Frame { origin: to_game(pos), axes: axes_from_forward(dir_to_game(forward), None) }
    }
}

/// A request to play an effect. Positions and directions are Bevy space.
#[derive(Event, Clone, Debug)]
pub enum FxEvent {
    /// An effect by name (`impacts/large_woodhit`) or by the scripts'
    /// `level._effect` key (`wood_chunk_destory`).
    Play { name: String, pos: Vec3, forward: Vec3, attach: Option<Entity> },
    /// The weapon's first-person muzzle flash (and shell eject) on the gun.
    MuzzleFlash { weapon: &'static str },
    /// A bullet hit the world at `pos` travelling along `dir`.
    BulletImpact { weapon: &'static str, pos: Vec3, dir: Vec3 },
    /// A bullet hit a zombie.
    FleshImpact { weapon: &'static str, pos: Vec3, dir: Vec3, head: bool, fatal: bool },
    /// A grenade (or other explosive weapon) exploded.
    Explosion { weapon: &'static str, pos: Vec3 },
}

impl FxEvent {
    pub fn play(name: impl Into<String>, pos: Vec3) -> FxEvent {
        FxEvent::Play { name: name.into(), pos, forward: Vec3::Y, attach: None }
    }
}

/// Effect definitions and their GPU materials, ready to play.
#[derive(Resource)]
pub struct FxLibrary {
    pub data: FxData,
    materials: Vec<(Handle<FxParticleMaterial>, (u32, u32))>,
}

impl FxLibrary {
    /// An effect by name or `level._effect` key.
    pub fn effect(&self, name: &str) -> Option<&Arc<EffectDef>> {
        let k = name.to_ascii_lowercase();
        self.data.effects.get(&k).or_else(|| self.data.level_effects.get(name).and_then(|n| self.data.effects.get(n)))
    }
}

/// The effect data from a map load, waiting to be uploaded.
#[derive(Resource)]
pub struct PendingFx(pub FxData);

/// A playing effect.
#[derive(Component)]
pub struct FxInstance {
    def: Arc<EffectDef>,
    /// Seconds since the effect started.
    t: f32,
    frame: Frame,
    attach: Option<Entity>,
    /// Stop spawning looping elements (the particles live on).
    stopped: bool,
    /// Per element: looping spawns done so far.
    loop_index: Vec<u32>,
    oneshots_done: bool,
    particles: Vec<Particle>,
    /// Material -> batch slot (index into the pool).
    batches: Vec<(usize, usize)>,
    rng: fastrand::Rng,
    spawn_counter: u32,
    /// Don't create light elements (the view model has its own flash light).
    no_lights: bool,
}

impl FxInstance {
    fn new(def: Arc<EffectDef>, frame: Frame, attach: Option<Entity>, start: f32) -> FxInstance {
        let mut loop_index = vec![0u32; def.elems.len()];
        // Starting in the past (pre-warmed placed effects): skip looping
        // spawns that would already be dead.
        if start > 0.0 {
            for (i, el) in def.elems.iter().enumerate().take(def.looping) {
                let interval = el.e.looping_interval().max(1) as f32 / 1000.0;
                let span = (el.e.life_span_msec.max() + el.e.spawn_delay_msec.max()) as f32 / 1000.0;
                loop_index[i] = ((start - span) / interval).floor().max(0.0) as u32;
            }
        }
        FxInstance {
            def,
            t: start,
            frame,
            attach,
            stopped: false,
            loop_index,
            oneshots_done: start > 0.0,
            particles: Vec::new(),
            batches: Vec::new(),
            rng: fastrand::Rng::new(),
            spawn_counter: 0,
            no_lights: false,
        }
    }

    /// Stops the looping elements; the instance ends once its particles die.
    pub fn stop(&mut self) {
        self.stopped = true;
    }
}

/// Plays a placed effect again every `period` seconds (createfx loop
/// effects, `playLoopedFx`), stopping the previous play.
#[derive(Component)]
pub struct FxLooper {
    def: Arc<EffectDef>,
    frame: Frame,
    period: f32,
    timer: f32,
    current: Option<Entity>,
}

/// Random numbers of one particle (`fx_randomTable` slots).
const R_VEL: usize = 0; // 0..3
const R_COLOR: usize = 3;
const R_ALPHA: usize = 4;
const R_SIZE0: usize = 5;
const R_SIZE1: usize = 6;
const R_SCALE: usize = 7;
const R_ROT: usize = 8;
const R_ANGVEL: usize = 9; // 9..12
const R_COUNT: usize = 12;

struct Particle {
    elem: u16,
    /// Instance time (s) it becomes active (after its spawn delay).
    born: f32,
    life: f32,
    r: [f32; R_COUNT],
    /// Spawn frame (world) and offset in it; RUN_RELATIVE_TO_EFFECT
    /// particles use the instance's current frame instead.
    frame: Frame,
    offset: Vec3,
    /// Orientation relative to the frame (spawn angles).
    angles: Vec3,
    rot0: f32,
    gravity: f32,
    visual: u16,
    atlas0: u16,
    started: bool,
    model: Option<Entity>,
    /// A runner's effect, moved along with it.
    child: Option<Entity>,
}

/// A world-space decal (bullet holes, scorch marks).
struct Decal {
    material: usize,
    corners: [Vec3; 4],
    uv: [Vec2; 2],
    color: [f32; 4],
    age: f32,
}

#[derive(Resource, Default)]
struct Decals {
    list: Vec<Decal>,
    batches: HashMap<usize, usize>,
}

/// Reusable particle meshes (one entity + mesh each).
#[derive(Resource, Default)]
struct BatchPool {
    slots: Vec<Slot>,
}

struct Slot {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Option<usize>,
    /// Owning instance (or `None` for decals / free).
    owner: Option<Entity>,
    in_use: bool,
    /// Touched this frame.
    seen: bool,
}

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(material::FxMaterialPlugin)
            .add_event::<FxEvent>()
            .init_resource::<BatchPool>()
            .init_resource::<Decals>()
            .init_resource::<FxLights>()
            .init_resource::<PlacedSpawned>()
            .add_systems(Update, (upload_library, update_live, hooks::muzzle_sphere))
            .add_systems(OnEnter(GameState::Loading), |mut p: ResMut<PlacedSpawned>| p.0 = false)
            .add_systems(
                Update,
                spawn_placed.run_if(in_state(GameState::Playing).and(resource_exists::<FxLibrary>).and(resource_exists::<crate::nacht::NachtActive>)),
            )
            .add_systems(
                Update,
                (handle_events, run_loopers, hooks::eye_glow, hooks::powerups, hooks::boards, hooks::chest_light)
                    .run_if(resource_exists::<FxLibrary>),
            )
            .add_systems(
                PostUpdate,
                (simulate, apply_fx_lights)
                    .chain()
                    .after(TransformSystem::TransformPropagate)
                    .before(VisibilitySystems::CheckVisibility)
                    .run_if(resource_exists::<FxLibrary>),
            );
    }
}

fn material_for(blend: Blend, image: Option<Handle<Image>>, feather: f32) -> FxParticleMaterial {
    let (mode, alpha) = match blend {
        Blend::Add | Blend::Screen => (0.0, AlphaMode::Premultiplied),
        Blend::Blend => (1.0, AlphaMode::Premultiplied),
        Blend::Multiply => (2.0, AlphaMode::Multiply),
    };
    FxParticleMaterial { params: FxParams { mode: Vec4::new(mode, feather, 0.0, 0.0) }, color: image, alpha }
}

/// Turns the loaded effect data into GPU textures and materials.
fn upload_library(
    mut commands: Commands,
    pending: Option<ResMut<PendingFx>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<FxParticleMaterial>>,
) {
    let Some(mut pending) = pending else { return };
    let mut data = std::mem::take(&mut pending.0);
    commands.remove_resource::<PendingFx>();
    let handles: HashMap<String, Handle<Image>> = std::mem::take(&mut data.images).into_iter().map(|(k, v)| (k, images.add(v))).collect();
    let materials = data
        .materials
        .iter()
        .map(|m| {
            let tex = m.image.as_ref().and_then(|i| handles.get(i).cloned());
            let h = mats.add(material_for(m.blend, tex, m.feather));
            (h, (m.atlas.0 as u32, m.atlas.1 as u32))
        })
        .collect();
    info!("Effects ready: {} effects, {} materials", data.effects.len(), data.materials.len());
    commands.insert_resource(FxLibrary { data, materials });
}

/// The placed effects of this session have been started.
#[derive(Resource, Default)]
struct PlacedSpawned(bool);

/// Starts the map's placed (createfx) effects.
fn spawn_placed(mut commands: Commands, lib: Res<FxLibrary>, mut spawned: ResMut<PlacedSpawned>, existing: Query<(), Or<(With<FxLooper>, With<PlacedFx>)>>) {
    if spawned.0 {
        return;
    }
    spawned.0 = true;
    if !existing.is_empty() {
        return;
    }
    let mut n = 0;
    for p in &lib.data.placed {
        let Some(def) = lib.effect(&p.id).cloned() else { continue };
        let frame = Frame { origin: Vec3::from(p.origin), axes: angles_to_axes(p.angles) };
        if p.looped && !def.has_infinite_loop() {
            commands.spawn((FxLooper { def, frame, period: p.delay.max(0.05), timer: 0.0, current: None }, PlacedFx, SessionEntity));
        } else {
            // One-shots with a negative delay start that long in the past.
            let start = if p.looped { 15.0 } else { (-p.delay).max(0.0) };
            commands.spawn((FxInstance::new(def, frame, None, start), PlacedFx, SessionEntity));
        }
        n += 1;
    }
    info!("Placed effects started: {n} of {}", lib.data.placed.len());
}

/// Marker: started from the map's createfx list.
#[derive(Component)]
struct PlacedFx;

fn run_loopers(time: Res<Time>, mut commands: Commands, mut loopers: Query<&mut FxLooper>, mut instances: Query<&mut FxInstance>) {
    let dt = time.delta_secs();
    for mut l in &mut loopers {
        l.timer -= dt;
        if l.timer > 0.0 {
            continue;
        }
        l.timer += l.period;
        if l.timer < 0.0 {
            l.timer = l.period;
        }
        if let Some(mut old) = l.current.and_then(|e| instances.get_mut(e).ok()) {
            old.stop();
        }
        let e = commands.spawn((FxInstance::new(l.def.clone(), l.frame, None, 0.0), SessionEntity)).id();
        l.current = Some(e);
    }
}

/// Finds a joint (or any named entity) below `root`.
pub fn find_named(root: Entity, name: &str, children: &Query<&Children>, names: &Query<&Name>) -> Option<Entity> {
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if names.get(e).is_ok_and(|n| n.as_str().eq_ignore_ascii_case(name)) {
            return Some(e);
        }
        if let Ok(c) = children.get(e) {
            stack.extend(c.iter());
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn handle_events(
    mut events: EventReader<FxEvent>,
    mut commands: Commands,
    lib: Res<FxLibrary>,
    flash: Query<(Entity, &GlobalTransform), With<crate::weapons::MuzzleFlash>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    names: Query<&Name>,
    globals: Query<&GlobalTransform>,
) {
    for ev in events.read() {
        match ev {
            FxEvent::Play { name, pos, forward, attach } => {
                if let Some(def) = lib.effect(name) {
                    commands.spawn((FxInstance::new(def.clone(), Frame::at(*pos, *forward), *attach, 0.0), Dynamic));
                }
            }
            FxEvent::MuzzleFlash { weapon } => {
                let Some(w) = lib.data.weapons.get(*weapon) else { continue };
                let Ok((fe, fg)) = flash.single() else { continue };
                // On the gun's `tag_flash` joint when the real rig is in use.
                let tag = parents.get(fe).ok().map(|p| p.parent()).filter(|p| names.get(*p).is_ok());
                let (at, frame) = match tag.and_then(|t| globals.get(t).ok().map(|g| (t, Frame::from_global(g)))) {
                    Some(x) => x,
                    None => (fe, Frame::from_global(fg)),
                };
                if let Some(def) = w.view_flash.as_deref().and_then(|n| lib.effect(n)) {
                    let mut inst = FxInstance::new(def.clone(), frame, Some(at), 0.0);
                    inst.no_lights = true;
                    commands.spawn((inst, Dynamic));
                }
                // Shells fly from the gun's brass tag.
                let Some(def) = w.view_shell_eject.as_deref().and_then(|n| lib.effect(n)) else { continue };
                // Up the gun's skeleton to the rig root, then down to the tag.
                let mut root = fe;
                while let Ok(p) = parents.get(root) {
                    root = p.parent();
                    if names.get(root).is_err() {
                        break;
                    }
                }
                if let Some(brass) = find_named(root, "tag_brass", &children, &names) {
                    if let Ok(g) = globals.get(brass) {
                        commands.spawn((FxInstance::new(def.clone(), Frame::from_global(g), Some(brass), 0.0), Dynamic));
                    }
                }
            }
            FxEvent::BulletImpact { weapon, pos, dir } => {
                let ty = lib.data.weapons.get(*weapon).map_or(1, |w| w.impact_type);
                let p = to_game(*pos);
                let hit = lib.data.surfaces.query(p, 6.0);
                let (surf, normal, at) = hit.unwrap_or((0, -dir_to_game(*dir), p));
                let row = data::impact_row(ty, false).and_then(|r| lib.data.impacts.get(r));
                let name = row.and_then(|r| r.nonflesh.get(surf as usize).cloned().flatten().or_else(|| r.nonflesh.first().cloned().flatten()));
                if let Some(def) = name.as_deref().and_then(|n| lib.effect(n)) {
                    let frame = Frame { origin: at + normal * 0.5, axes: axes_from_forward(normal, None) };
                    commands.spawn((FxInstance::new(def.clone(), frame, None, 0.0), Dynamic));
                }
            }
            FxEvent::FleshImpact { weapon, pos, dir, head, fatal } => {
                let ty = lib.data.weapons.get(*weapon).map_or(1, |w| w.impact_type);
                let idx = (*head as usize) * 2 + (*fatal as usize);
                let d = dir_to_game(*dir);
                for (exit, forward) in [(false, -d), (true, d)] {
                    let row = data::impact_row(ty, exit).and_then(|r| lib.data.impacts.get(r));
                    if let Some(def) = row.and_then(|r| r.flesh.get(idx).cloned().flatten()).as_deref().and_then(|n| lib.effect(n)) {
                        let frame = Frame { origin: to_game(*pos), axes: axes_from_forward(forward, None) };
                        commands.spawn((FxInstance::new(def.clone(), frame, None, 0.0), Dynamic));
                    }
                }
                // (A popped head's own blood is played by `gibs`.)
            }
            FxEvent::Explosion { weapon, pos } => {
                let w = lib.data.weapons.get(*weapon);
                let p = to_game(*pos);
                // Explosions face up, with the surface underneath choosing
                // the effect (`projExplosionEffectForceNormalUp`).
                let surf = lib.data.surfaces.query(p - Vec3::Z * 4.0, 24.0).map_or(0, |h| h.0);
                let name = w.and_then(|w| w.proj_explosion.clone()).or_else(|| {
                    let row = data::impact_row(w.map_or(6, |w| w.impact_type), false).and_then(|r| lib.data.impacts.get(r))?;
                    row.nonflesh.get(surf as usize).cloned().flatten().or_else(|| row.nonflesh.first().cloned().flatten())
                });
                info!("explosion of {weapon}: {} on {}", name.as_deref().unwrap_or("-"), waw_assets::t4::fx::SURFACE_TYPES[surf as usize % 31]);
                if let Some(def) = name.as_deref().and_then(|n| lib.effect(n)) {
                    let frame = Frame { origin: p, axes: axes_from_forward(Vec3::Z, Some(Vec3::X)) };
                    commands.spawn((FxInstance::new(def.clone(), frame, None, 0.0), Dynamic));
                }
            }
        }
    }
}

// ------------------------------------------------------------- simulation

/// `base + amp * r`, per component.
fn v3r(base: [f32; 3], amp: [f32; 3], r: &[f32]) -> Vec3 {
    Vec3::new(base[0] + amp[0] * r[0], base[1] + amp[1] * r[1], base[2] + amp[2] * r[2])
}

/// Sample index and fraction for normalised time `f` over `n` intervals.
fn segment(f: f32, n: usize) -> (usize, f32) {
    let s = f.clamp(0.0, 1.0) * n as f32;
    let i = (s.floor() as usize).min(n.saturating_sub(1));
    (i, s - i as f32)
}

/// Displacement (game units) and velocity (units/ms) from a velocity graph
/// at normalised time `f` of a `life_ms` life.
fn integrate(e: &Elem, world: bool, r: &[f32], f: f32, life_ms: f32) -> (Vec3, Vec3) {
    let n = e.vel.len().saturating_sub(1);
    if n == 0 {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    let (i, u) = segment(f, n);
    let pick = |k: usize| {
        let s = &e.vel[k];
        if world {
            (v3r(s.world_velocity.base, s.world_velocity.amp, r), v3r(s.world_delta.base, s.world_delta.amp, r))
        } else {
            (v3r(s.local_velocity.base, s.local_velocity.amp, r), v3r(s.local_delta.base, s.local_delta.amp, r))
        }
    };
    let (v0, d0) = pick(i);
    let (v1, _) = pick(i + 1);
    let disp = (d0 + v0 * u + (v1 - v0) * (0.5 * u * u)) * (life_ms / n as f32);
    (disp, v0.lerp(v1, u))
}

/// The look of a particle at normalised time `f`.
struct Look {
    /// Linear RGB and alpha.
    color: [f32; 4],
    size: [f32; 2],
    rotation: f32,
    scale: f32,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn look(e: &Elem, p: &Particle, f: f32) -> Look {
    let m = e.vis.len().saturating_sub(1);
    if e.vis.is_empty() {
        return Look { color: [1.0; 4], size: [1.0; 2], rotation: p.rot0, scale: 1.0 };
    }
    let (i, u) = if m == 0 { (0, 0.0) } else { segment(f, m) };
    let j = (i + 1).min(m);
    let r = &p.r;
    // Colours: a random point between the two stored colours (B, G, R, A).
    let col = |k: usize| {
        let s = &e.vis[k];
        let c = |ch: usize, rr: f32| s.base.color[ch] as f32 + (s.amp.color[ch] as f32 - s.base.color[ch] as f32) * rr;
        [c(2, r[R_COLOR]), c(1, r[R_COLOR]), c(0, r[R_COLOR]), c(3, r[R_ALPHA])]
    };
    let (c0, c1) = (col(i), col(j));
    let lerp = |a: f32, b: f32| a + (b - a) * u;
    let color = [
        srgb_to_linear((lerp(c0[0], c1[0]) / 255.0).clamp(0.0, 1.0)),
        srgb_to_linear((lerp(c0[1], c1[1]) / 255.0).clamp(0.0, 1.0)),
        srgb_to_linear((lerp(c0[2], c1[2]) / 255.0).clamp(0.0, 1.0)),
        (lerp(c0[3], c1[3]) / 255.0).clamp(0.0, 1.0),
    ];
    let fl = |k: usize, g: fn(&waw_assets::t4::fx::VisState) -> f32, rr: f32| g(&e.vis[k].base) + g(&e.vis[k].amp) * rr;
    let s0 = lerp(fl(i, |v| v.size[0], r[R_SIZE0]), fl(j, |v| v.size[0], r[R_SIZE0]));
    let s1 = if e.has(ef::NONUNIFORM_SCALE) { lerp(fl(i, |v| v.size[1], r[R_SIZE1]), fl(j, |v| v.size[1], r[R_SIZE1])) } else { s0 };
    let scale = lerp(fl(i, |v| v.scale, r[R_SCALE]), fl(j, |v| v.scale, r[R_SCALE]));
    let life_ms = p.life * 1000.0;
    let rot_total = lerp(fl(i, |v| v.rotation_total, r[R_ROT]), fl(j, |v| v.rotation_total, r[R_ROT]));
    let rotation = p.rot0 + rot_total * if m > 0 { life_ms / m as f32 } else { 0.0 };
    Look { color, size: [s0.max(0.0), s1.max(0.0)], rotation, scale }
}

/// Atlas cell UV rectangle for a particle.
fn atlas_uv(a: &Atlas, mat_atlas: (u32, u32), start: u32, f: f32, age: f32) -> [Vec2; 2] {
    let (mut cols, mut rows) = (1u32 << a.col_index_bits.min(8), 1u32 << a.row_index_bits.min(8));
    if cols * rows <= 1 && mat_atlas.0 * mat_atlas.1 > 1 {
        rows = mat_atlas.0.max(1);
        cols = mat_atlas.1.max(1);
    }
    let entries = (a.entry_count.max(1) as u32).min(cols * rows).max(1);
    let mut frame = start;
    if a.behavior & Atlas::PLAY_OVER_LIFE != 0 {
        frame += (f.clamp(0.0, 0.9999) * entries as f32) as u32;
    } else if a.fps > 0 {
        let n = (age * a.fps as f32) as u32;
        frame += if a.behavior & Atlas::LOOP_ONLY_N_TIMES != 0 { n.min(entries * a.loop_count.max(1) as u32 - 1) } else { n };
    }
    frame %= entries;
    let (c, r) = (frame % cols, (frame / cols) % rows);
    [Vec2::new(c as f32 / cols as f32, r as f32 / rows as f32), Vec2::new((c + 1) as f32 / cols as f32, (r + 1) as f32 / rows as f32)]
}

/// Alpha factor from the camera distance (`fadeInRange` fades far
/// particles out, `fadeOutRange` near ones).
fn distance_fade(e: &Elem, d: f32) -> f32 {
    let mut a = 1.0;
    let fi = e.fade_in_range;
    if fi.amp > 0.0 {
        a *= ((fi.base + fi.amp - d) / fi.amp).clamp(0.0, 1.0);
    } else if fi.base > 0.0 && d > fi.base {
        a = 0.0;
    }
    let fo = e.fade_out_range;
    if fo.amp > 0.0 {
        a *= ((d - fo.base) / fo.amp).clamp(0.0, 1.0);
    }
    a
}

/// Element orientation (game) from spawn angles plus angular velocity.
/// Spawn angles and angular velocity are stored in radians.
fn elem_axes(frame: &Frame, angles: Vec3) -> Mat3 {
    frame.axes * angles_to_axes([angles.x.to_degrees(), angles.y.to_degrees(), angles.z.to_degrees()])
}

#[derive(Default)]
struct Quads {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    color: Vec<[f32; 4]>,
    /// Sort key (camera distance) per quad.
    depth: Vec<f32>,
}

impl Quads {
    fn push(&mut self, c: [Vec3; 4], uv: [Vec2; 2], color: [f32; 4], depth: f32) {
        self.pos.extend(c.map(|v| v.to_array()));
        self.uv.extend([[uv[0].x, uv[1].y], [uv[1].x, uv[1].y], [uv[1].x, uv[0].y], [uv[0].x, uv[0].y]]);
        self.color.extend([color; 4]);
        self.depth.push(depth);
    }
}

/// Writes quads (relative to `origin`) into a batch mesh; sorted back to
/// front when alpha-blended.
fn write_mesh(mesh: &mut Mesh, q: &Quads, origin: Vec3, sort: bool) -> Aabb {
    let n = q.depth.len();
    let mut order: Vec<usize> = (0..n).collect();
    if sort {
        order.sort_by(|&a, &b| q.depth[b].total_cmp(&q.depth[a]));
    }
    let mut pos = Vec::with_capacity(n * 4);
    let mut uv = Vec::with_capacity(n * 4);
    let mut col = Vec::with_capacity(n * 4);
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for &k in &order {
        for v in 0..4 {
            let p = Vec3::from(q.pos[k * 4 + v]) - origin;
            lo = lo.min(p);
            hi = hi.max(p);
            pos.push(p.to_array());
            uv.push(q.uv[k * 4 + v]);
            col.push(q.color[k * 4 + v]);
        }
    }
    let mut idx = Vec::with_capacity(n * 6);
    for k in 0..n as u32 {
        let b = k * 4;
        idx.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    if n == 0 {
        return Aabb::from_min_max(Vec3::ZERO, Vec3::ZERO);
    }
    Aabb::from_min_max(lo, hi)
}

fn empty_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    m.insert_indices(Indices::U32(Vec::new()));
    m
}

impl BatchPool {
    /// A free slot set to `material` for `owner`, growing the pool if needed.
    fn take(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>, lib: &FxLibrary, material: usize, owner: Option<Entity>) -> usize {
        let i = match self.slots.iter().position(|s| !s.in_use) {
            Some(i) => i,
            None => {
                let mesh = meshes.add(empty_mesh());
                let entity = commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(lib.materials[material].0.clone()),
                        Transform::default(),
                        Visibility::Hidden,
                        Aabb::default(),
                        FxBatch,
                        material::batch_bundle(),
                    ))
                    .id();
                self.slots.push(Slot { entity, mesh, material: Some(material), owner, in_use: false, seen: false });
                self.slots.len() - 1
            }
        };
        let s = &mut self.slots[i];
        if s.material != Some(material) {
            commands.entity(s.entity).insert(MeshMaterial3d(lib.materials[material].0.clone()));
            s.material = Some(material);
        }
        s.in_use = true;
        s.owner = owner;
        s.seen = true;
        i
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn simulate(
    time: Res<Time>,
    mut commands: Commands,
    lib: Res<FxLibrary>,
    mut pool: ResMut<BatchPool>,
    mut decals: ResMut<Decals>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut instances: Query<(Entity, &mut FxInstance)>,
    camera: Query<&GlobalTransform, (With<crate::player::Player>, Without<FxBatch>)>,
    globals: Query<&GlobalTransform, Without<FxBatch>>,
    mut batches: Query<(&mut Transform, &mut GlobalTransform, &mut Visibility, &mut Aabb), With<FxBatch>>,
    mut fx_lights: ResMut<FxLights>,
    mut models: Query<&mut Transform, (With<FxModel>, Without<FxBatch>)>,
    mut alias: EventWriter<crate::audio::PlayAlias>,
    nacht: Option<Res<crate::nacht::NachtAssets>>,
    world: Option<Res<crate::World>>,
) {
    let dt = time.delta_secs().min(0.1);
    fx_lights.requests.clear();
    let Ok(cam) = camera.single() else { return };
    let mesh = world.as_ref().and_then(|w| w.mesh.clone());
    let cam_pos = cam.translation();
    let cam_right = cam.right().as_vec3();
    let cam_up = cam.up().as_vec3();
    let cam_game = to_game(cam_pos);
    for s in &mut pool.slots {
        s.seen = false;
    }
    let mut quads: HashMap<usize, Quads> = HashMap::new();
    let mut children: Vec<(Arc<EffectDef>, Frame)> = Vec::new();
    let mut child_frames: Vec<(Entity, Frame)> = Vec::new();
    let mut stop_children: Vec<Entity> = Vec::new();
    for (ie, mut inst) in &mut instances {
        let inst = &mut *inst;
        if let Some(a) = inst.attach {
            match globals.get(a) {
                Ok(g) => inst.frame = Frame::from_global(g),
                Err(_) => {
                    inst.attach = None;
                    inst.stopped = true;
                }
            }
        }
        let def = inst.def.clone();
        let t0 = inst.t;
        inst.t += dt;
        let t = inst.t;
        let cam_dist = inst.frame.origin.distance(cam_game);
        // Spawn.
        let mut new: Vec<(usize, f32)> = Vec::new();
        if !inst.oneshots_done {
            inst.oneshots_done = true;
            for i in def.looping..(def.looping + def.oneshot).min(def.elems.len()) {
                let e = &def.elems[i].e;
                let n = e.oneshot_count().at(inst.rng.f32()).max(0);
                for _ in 0..n {
                    new.push((i, 0.0));
                }
            }
        }
        let looping_on = !inst.stopped && def.looping_life.is_none_or(|l| t0 < l);
        if looping_on {
            for i in 0..def.looping.min(def.elems.len()) {
                let e = &def.elems[i].e;
                let interval = e.looping_interval().max(1) as f32 / 1000.0;
                let max_count = e.looping_count().max(0) as u32;
                // Placed effects far from the player wait (spawnRange).
                let in_range = e.spawn_range.amp <= 0.0 || cam_dist <= e.spawn_range.max() + 200.0;
                while inst.loop_index[i] < max_count {
                    let at = inst.loop_index[i] as f32 * interval;
                    if at > t || def.looping_life.is_some_and(|l| at >= l) {
                        break;
                    }
                    inst.loop_index[i] += 1;
                    if in_range && inst.particles.len() + new.len() < MAX_PARTICLES {
                        new.push((i, at));
                    }
                }
            }
        }
        for (i, at) in new {
            let e = &def.elems[i].e;
            let rng = &mut inst.rng;
            let delay = e.spawn_delay_msec.at(rng.f32()).max(0) as f32 / 1000.0;
            let life = (e.life_span_msec.at(rng.f32()).max(1) as f32 / 1000.0).max(0.001);
            let born = at + delay;
            if born + life < t && e.elem_type != et::DECAL {
                continue;
            }
            let mut r = [0f32; R_COUNT];
            for x in &mut r {
                *x = rng.f32();
            }
            let mut offset = Vec3::new(e.spawn_origin[0].at(rng.f32()), e.spawn_origin[1].at(rng.f32()), e.spawn_origin[2].at(rng.f32()));
            match e.flags & ef::SPAWN_OFFSET_MASK {
                ef::SPAWN_OFFSET_SPHERE => {
                    let d = Vec3::new(rng.f32() * 2.0 - 1.0, rng.f32() * 2.0 - 1.0, rng.f32() * 2.0 - 1.0).normalize_or(Vec3::X);
                    offset += d * e.spawn_offset_radius.at(rng.f32());
                }
                ef::SPAWN_OFFSET_CYLINDER => {
                    let a = rng.f32() * std::f32::consts::TAU;
                    let rad = e.spawn_offset_radius.at(rng.f32());
                    offset += Vec3::new((rng.f32() - 0.5) * e.spawn_offset_height.at(rng.f32()), a.cos() * rad, a.sin() * rad);
                }
                _ => {}
            }
            let angles = Vec3::new(e.spawn_angles[0].at(rng.f32()), e.spawn_angles[1].at(rng.f32()), e.spawn_angles[2].at(rng.f32()));
            let nv = def.elems[i].visuals.len();
            let atlas0 = match e.atlas.behavior & Atlas::START_MASK {
                Atlas::START_RANDOM => rng.u32(..(e.atlas.entry_count.max(1) as u32)),
                Atlas::START_INDEXED => inst.spawn_counter,
                _ => e.atlas.index as u32,
            };
            inst.spawn_counter += 1;
            inst.particles.push(Particle {
                elem: i as u16,
                born,
                life,
                r,
                frame: inst.frame,
                offset,
                angles,
                rot0: e.initial_rotation.at(rng.f32()),
                gravity: if e.has(ef::HAS_GRAVITY) { e.gravity.at(rng.f32()) } else { 0.0 },
                visual: if nv > 1 { rng.usize(..nv) as u16 } else { 0 },
                atlas0: atlas0 as u16,
                started: false,
                model: None,
                child: None,
            });
        }

        // Update and draw.
        let mut keep = Vec::with_capacity(inst.particles.len());
        for mut p in std::mem::take(&mut inst.particles) {
            let ed = &def.elems[p.elem as usize];
            let e = &ed.e;
            let age = t - p.born;
            if age < 0.0 {
                keep.push(p);
                continue;
            }
            let f = age / p.life;
            let frame = if e.flags & ef::RUN_MASK >= ef::RUN_RELATIVE_TO_EFFECT { inst.frame } else { p.frame };
            let life_ms = p.life * 1000.0;
            let mut pos = frame.origin + frame.axes * p.offset;
            let mut vel = Vec3::ZERO;
            if e.has(ef::HAS_VELOCITY_GRAPH_LOCAL) {
                let (d, v) = integrate(e, false, &p.r[R_VEL..R_VEL + 3], f, life_ms);
                pos += frame.axes * d;
                vel += frame.axes * v;
            }
            if e.has(ef::HAS_VELOCITY_GRAPH_WORLD) {
                let (d, v) = integrate(e, true, &p.r[R_VEL..R_VEL + 3], f, life_ms);
                pos += d;
                vel += v;
            }
            if p.gravity != 0.0 {
                pos.z -= 0.5 * p.gravity * GRAVITY * age * age;
                vel.z -= p.gravity * GRAVITY * age / 1000.0;
            }
            let ang_vel = Vec3::new(e.angular_velocity[0].at(p.r[R_ANGVEL]), e.angular_velocity[1].at(p.r[R_ANGVEL + 1]), e.angular_velocity[2].at(p.r[R_ANGVEL + 2]));
            let orient = elem_axes(&frame, p.angles + ang_vel * age * 1000.0);
            let first = !p.started;
            p.started = true;
            let visual = ed.visuals.get(p.visual as usize);
            // Things that happen once, when the particle appears (even if
            // its life is shorter than a frame).
            if first {
                let lk0 = look(e, &p, 0.0);
                match (e.elem_type, visual) {
                    (et::SOUND, Some(Visual::Sound(s))) => {
                        alias.write(crate::audio::PlayAlias::at(s.clone(), to_bevy(pos)));
                    }
                    (et::RUNNER, Some(Visual::Effect(n))) => {
                        if let Some(d) = lib.effect(n) {
                            p.child = Some(commands.spawn((FxInstance::new(d.clone(), Frame { origin: pos, axes: orient }, None, 0.0), Dynamic)).id());
                        }
                    }
                    (et::DECAL, Some(Visual::Decal(m))) => {
                        // A decal marks the world where it lands: the game
                        // projects it onto geometry within its size around
                        // the effect. Blood from a zombie in the open has
                        // nothing to land on (it used to hang in the air).
                        let half = Vec2::new(lk0.size[0], lk0.size[1]) * INCH;
                        let x = dir_to_bevy(orient.x_axis).normalize_or_zero();
                        let o = to_bevy(pos);
                        let reach = half.max_element().max(2.0 * INCH) + 0.02;
                        let v = |a: Vec3| zm_core::geom::V3::new(a.x, a.y, a.z);
                        let surface = mesh.as_ref().and_then(|mesh| {
                            [-x, x].into_iter().find_map(|d| {
                                let start = o - d * 0.02;
                                mesh.raycast(v(start), v(d), reach).map(|h| {
                                    let n = Vec3::new(h.normal.x, h.normal.y, h.normal.z);
                                    (start + d * h.t, if n.dot(d) > 0.0 { -n } else { n })
                                })
                            })
                        });
                        if let Some((at, n)) = surface {
                            // The effect's own axes, laid flat on the surface.
                            let y0 = dir_to_bevy(orient.y_axis);
                            let y = (y0 - n * y0.dot(n)).try_normalize().unwrap_or_else(|| n.any_orthonormal_vector());
                            let z = n.cross(y);
                            let (s, c) = lk0.rotation.sin_cos();
                            let (ry, rz) = (y * c + z * s, -y * s + z * c);
                            let center = at + n * 0.004;
                            let corners = [
                                center - ry * half.x - rz * half.y,
                                center + ry * half.x - rz * half.y,
                                center + ry * half.x + rz * half.y,
                                center - ry * half.x + rz * half.y,
                            ];
                            let uv = atlas_uv(&e.atlas, lib.materials[*m].1, p.atlas0 as u32, 0.0, 0.0);
                            decals.list.push(Decal { material: *m, corners, uv, color: lk0.color, age: 0.0 });
                            if decals.list.len() > MAX_DECALS {
                                decals.list.remove(0);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let Some(c) = p.child {
                child_frames.push((c, Frame { origin: pos, axes: orient }));
            }
            if f >= 1.0 {
                if let Some(m) = p.model.take() {
                    commands.entity(m).try_despawn();
                }
                if let Some(c) = p.child.take() {
                    stop_children.push(c);
                }
                if let Some(d) = e.effect_on_death.as_deref().and_then(|n| lib.effect(n)) {
                    children.push((d.clone(), Frame { origin: pos, axes: orient }));
                }
                continue;
            }
            let lk = look(e, &p, f);
            match e.elem_type {
                et::SOUND | et::RUNNER | et::DECAL => {}
                et::OMNI_LIGHT | et::SPOT_LIGHT => {
                    if inst.no_lights {
                        keep.push(p);
                        continue;
                    }
                    let c = Vec3::new(lk.color[0], lk.color[1], lk.color[2]) * lk.color[3];
                    let range = (lk.size[0] * INCH).max(0.05);
                    let intensity = light_intensity(c.max_element(), range);
                    let m = c.max_element().max(1e-4);
                    let col = LinearRgba::rgb(c.x / m, c.y / m, c.z / m);
                    fx_lights.requests.push(LightRequest { pos: to_bevy(pos), color: col, intensity, range });
                }
                et::MODEL => {
                    let Some(Visual::Model(name)) = visual else {
                        keep.push(p);
                        continue;
                    };
                    let scale = if lk.scale > 0.0 { lk.scale } else { 1.0 };
                    // Model axes: game X, Y, Z are Bevy X, -Z, Y.
                    let rot = Quat::from_mat3(&Mat3::from_cols(dir_to_bevy(orient.x_axis), dir_to_bevy(orient.z_axis), dir_to_bevy(-orient.y_axis)));
                    let tr = Transform::from_translation(to_bevy(pos)).with_rotation(rot).with_scale(Vec3::splat(scale));
                    match p.model.and_then(|m| models.get_mut(m).ok()) {
                        Some(mut mt) => *mt = tr,
                        None if p.model.is_none() => {
                            if let Some(parts) = nacht.as_ref().and_then(|n| n.models.get(name)) {
                                let ent = commands
                                    .spawn((tr, Visibility::default(), FxModel, Dynamic))
                                    .with_children(|c| {
                                        for (mesh, mat) in &parts.parts {
                                            c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), bevy::pbr::NotShadowCaster));
                                        }
                                    })
                                    .id();
                                p.model = Some(ent);
                            }
                        }
                        None => {}
                    }
                }
                _ => {
                    let Some(Visual::Material(m)) = visual else {
                        keep.push(p);
                        continue;
                    };
                    let bpos = to_bevy(pos);
                    let d_in = bpos.distance(cam_pos) / INCH;
                    let mut color = lk.color;
                    color[3] *= distance_fade(e, d_in);
                    if e.lighting_frac > 0 {
                        let k = 1.0 + (LIT_AMBIENT - 1.0) * e.lighting_frac as f32 / 255.0;
                        for c in color.iter_mut().take(3) {
                            *c *= k;
                        }
                    }
                    if color[3] <= 0.002 || (lk.size[0] <= 0.0 && lk.size[1] <= 0.0) {
                        keep.push(p);
                        continue;
                    }
                    let uv = atlas_uv(&e.atlas, lib.materials[*m].1, p.atlas0 as u32, f, age);
                    let (w, h) = (lk.size[0] * INCH, lk.size[1] * INCH);
                    let corners = match e.elem_type {
                        et::TAIL => {
                            let dir = dir_to_bevy(vel).normalize_or(dir_to_bevy(frame.axes.x_axis));
                            let side = dir.cross(cam_pos - bpos).normalize_or(cam_right);
                            let end = bpos + dir * h;
                            [bpos - side * w, bpos + side * w, end + side * w, end - side * w]
                        }
                        et::SPRITE_ORIENTED => {
                            let (y, z) = (dir_to_bevy(orient.y_axis), dir_to_bevy(orient.z_axis));
                            let (s, c) = lk.rotation.sin_cos();
                            let (ry, rz) = (y * c + z * s, -y * s + z * c);
                            [bpos - ry * w - rz * h, bpos + ry * w - rz * h, bpos + ry * w + rz * h, bpos - ry * w + rz * h]
                        }
                        _ => {
                            // Billboards; "rotated" sprites line up with
                            // their screen-space velocity.
                            let rot = if e.elem_type == et::SPRITE_ROTATED {
                                let v = dir_to_bevy(vel);
                                v.dot(cam_up).atan2(v.dot(cam_right))
                            } else {
                                lk.rotation
                            };
                            let (s, c) = rot.sin_cos();
                            let (rx, ry) = (cam_right * c + cam_up * s, -cam_right * s + cam_up * c);
                            [bpos - rx * w - ry * h, bpos + rx * w - ry * h, bpos + rx * w + ry * h, bpos - rx * w + ry * h]
                        }
                    };
                    let key = *m;
                    quads.entry(key).or_default().push(corners, uv, color, bpos.distance_squared(cam_pos));
                }
            }
            keep.push(p);
        }
        inst.particles = keep;

        // Write this instance's batches.
        let origin = to_bevy(inst.frame.origin);
        let mut used: Vec<(usize, usize)> = Vec::new();
        for (m, q) in quads.drain() {
            let slot = match inst.batches.iter().find(|b| b.0 == m) {
                Some(&(_, s)) => s,
                None => pool.take(&mut commands, &mut meshes, &lib, m, Some(ie)),
            };
            used.push((m, slot));
            let s = &mut pool.slots[slot];
            s.seen = true;
            if let Some(mesh) = meshes.get_mut(&s.mesh) {
                let sort = lib.data.materials[m].blend == Blend::Blend;
                let aabb = write_mesh(mesh, &q, origin, sort);
                if let Ok((mut tr, mut gt, mut vis, mut bb)) = batches.get_mut(s.entity) {
                    *tr = Transform::from_translation(origin);
                    *gt = GlobalTransform::from(*tr);
                    *vis = Visibility::Visible;
                    *bb = aabb;
                }
            }
        }
        inst.batches = used;

        let done_spawning = inst.oneshots_done
            && (inst.stopped
                || def.looping == 0
                || def.looping_life.is_some_and(|l| t >= l)
                || (0..def.looping.min(def.elems.len())).all(|i| inst.loop_index[i] >= def.elems[i].e.looping_count().max(0) as u32));
        if done_spawning && inst.particles.is_empty() {
            commands.entity(ie).try_despawn();
        }
    }
    for (def, frame) in children {
        commands.spawn((FxInstance::new(def, frame, None, 0.0), Dynamic));
    }
    for (c, frame) in child_frames {
        if let Ok((_, mut i)) = instances.get_mut(c) {
            i.frame = frame;
        }
    }
    for c in stop_children {
        if let Ok((_, mut i)) = instances.get_mut(c) {
            i.stop();
        }
    }

    // Decals: one batch per material, fading out at the end.
    for d in &mut decals.list {
        d.age += dt;
    }
    decals.list.retain(|d| d.age < DECAL_LIFE);
    let mut dq: HashMap<usize, Quads> = HashMap::new();
    for d in &decals.list {
        let mut c = d.color;
        c[3] *= ((DECAL_LIFE - d.age) / 2.0).clamp(0.0, 1.0);
        dq.entry(d.material).or_default().push(d.corners, d.uv, c, 0.0);
    }
    let mut dbatches = std::mem::take(&mut decals.batches);
    for (m, q) in dq {
        let slot = match dbatches.get(&m) {
            Some(&s) => s,
            None => pool.take(&mut commands, &mut meshes, &lib, m, None),
        };
        dbatches.insert(m, slot);
        let s = &mut pool.slots[slot];
        s.seen = true;
        if let Some(mesh) = meshes.get_mut(&s.mesh) {
            let aabb = write_mesh(mesh, &q, Vec3::ZERO, false);
            if let Ok((mut tr, mut gt, mut vis, mut bb)) = batches.get_mut(s.entity) {
                *tr = Transform::IDENTITY;
                *gt = GlobalTransform::IDENTITY;
                *vis = Visibility::Visible;
                *bb = aabb;
            }
        }
    }
    dbatches.retain(|_, s| pool.slots[*s].seen);
    decals.batches = dbatches;

    // Free batches nobody drew this frame.
    for s in &mut pool.slots {
        if s.in_use && !s.seen {
            s.in_use = false;
            s.owner = None;
            if let Ok((_, _, mut vis, _)) = batches.get_mut(s.entity) {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// Marker for particle batch meshes.
#[derive(Component)]
struct FxBatch;

/// Marker for models spawned by effects.
#[derive(Component)]
struct FxModel;

pub mod hooks;

static LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The game's own effects are playing (a real map with its effects
/// loaded): gameplay skips its stand-in visuals.
pub fn live() -> bool {
    LIVE.load(std::sync::atomic::Ordering::Relaxed)
}

fn update_live(lib: Option<Res<FxLibrary>>, nacht: Option<Res<crate::nacht::NachtActive>>) {
    LIVE.store(lib.is_some() && nacht.is_some(), std::sync::atomic::Ordering::Relaxed);
}

/// Shows the strongest of this frame's effect lights (by brightness and
/// nearness to the camera) through a fixed pool of point lights; the rest
/// are hidden so they stay out of Bevy's light clusters.
fn apply_fx_lights(
    mut commands: Commands,
    mut fx: ResMut<FxLights>,
    camera: Query<&GlobalTransform, (With<crate::player::Player>, Without<FxLight>)>,
    mut lights: Query<(&mut PointLight, &mut Transform, &mut GlobalTransform, &mut Visibility), With<FxLight>>,
) {
    let cam = camera.single().map(|c| c.translation()).unwrap_or(Vec3::ZERO);
    let FxLights { pool, requests } = &mut *fx;
    pool.retain(|e| lights.contains(*e));
    while pool.len() < MAX_FX_LIGHTS {
        pool.push(commands.spawn((PointLight { intensity: 0.0, shadows_enabled: false, ..default() }, Transform::default(), Visibility::Hidden, FxLight, Dynamic)).id());
    }
    let score = |r: &LightRequest| {
        let d = (r.pos.distance(cam) - r.range).max(0.0);
        r.intensity / (1.0 + d * d)
    };
    requests.sort_by(|a, b| score(b).total_cmp(&score(a)));
    for (i, e) in pool.iter().enumerate() {
        let Ok((mut pl, mut t, mut gt, mut vis)) = lights.get_mut(*e) else { continue };
        match requests.get(i) {
            Some(r) => {
                pl.color = r.color.into();
                pl.intensity = r.intensity;
                pl.range = r.range;
                *t = Transform::from_translation(r.pos);
                *gt = GlobalTransform::from(*t);
                *vis = Visibility::Visible;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}
