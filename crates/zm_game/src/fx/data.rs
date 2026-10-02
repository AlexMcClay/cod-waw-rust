//! Effect data gathered from the zones while a map loads (off the main
//! thread): the effects the game plays, their textures, the impact table,
//! each weapon's effects, the scripts' `level._effect` names and the map's
//! placed (createfx) effects. Nothing here is hand-tuned per map.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use waw_assets::iwi::Iwi;
use waw_assets::t4::{decode, fx as t4fx, ZoneData};
use waw_assets::Iwd;

/// How a material's texture combines with what is behind it (from the
/// material's technique set name: `effect_add`, `effect_blend`...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blend {
    Add,
    Blend,
    /// `screen` techsets: drawn additively here.
    Screen,
    Multiply,
}

impl Blend {
    pub fn from_techset(t: &str) -> Blend {
        let t = t.trim_start_matches(',');
        if t.contains("screen") {
            Blend::Screen
        } else if t.contains("add") {
            Blend::Add
        } else if t.contains("multiply") {
            Blend::Multiply
        } else {
            Blend::Blend
        }
    }
}

#[derive(Debug, Clone)]
pub struct FxMaterial {
    #[allow(dead_code)] // for debugging
    pub name: String,
    /// Colour map image name (key into [`FxData::images`]).
    pub image: Option<String>,
    pub blend: Blend,
    /// `(rows, columns)` of the texture atlas.
    pub atlas: (u8, u8),
    /// "zfeather" techsets fade where they meet the scene, over this many
    /// metres (the material's `featherParms`; 0 = hard).
    pub feather: f32,
}

/// What an element draws, resolved across zones.
#[derive(Debug, Clone)]
pub enum Visual {
    /// Index into [`FxData::materials`].
    Material(usize),
    Model(String),
    Effect(String),
    Sound(String),
    /// A decal: the world material of a mark (index into materials).
    Decal(usize),
}

#[derive(Debug, Clone)]
pub struct ElemDef {
    pub e: t4fx::Elem,
    pub visuals: Vec<Visual>,
}

#[derive(Debug, Clone)]
pub struct EffectDef {
    #[allow(dead_code)] // for debugging
    pub name: String,
    /// Seconds the looping elements keep spawning (`None` = forever).
    pub looping_life: Option<f32>,
    pub looping: usize,
    pub oneshot: usize,
    pub elems: Vec<ElemDef>,
}

impl EffectDef {
    pub fn has_infinite_loop(&self) -> bool {
        self.looping > 0 && self.looping_life.is_none()
    }
}

/// A `createLoopEffect` / `createOneshotEffect` from the map's createfx
/// script.
#[derive(Debug, Clone)]
pub struct Placed {
    pub id: String,
    /// Game units (inches, Z up).
    pub origin: [f32; 3],
    /// Pitch, yaw, roll in degrees.
    pub angles: [f32; 3],
    /// Loop effects: seconds between plays. One-shots: start delay; a
    /// negative delay starts the effect that long in the past.
    pub delay: f32,
    pub looped: bool,
}

/// Map triangles with their surface types, bucketed in a grid, so an
/// impact point can find what it hit (for the impact table) and its normal.
#[derive(Debug, Clone, Default)]
pub struct SurfaceGrid {
    pub origin: [f32; 3],
    pub cell: f32,
    pub dims: [usize; 3],
    /// Per cell: range into `items`.
    pub cells: Vec<(u32, u32)>,
    pub items: Vec<u32>,
    pub tris: Vec<([[f32; 3]; 3], u8)>,
}

impl SurfaceGrid {
    fn build(tris: Vec<([[f32; 3]; 3], u8)>, cell: f32) -> SurfaceGrid {
        if tris.is_empty() {
            return SurfaceGrid::default();
        }
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for (t, _) in &tris {
            for v in t {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
        }
        let dims = [0, 1, 2].map(|k| (((hi[k] - lo[k]) / cell).floor() as usize + 1).min(4096));
        let n = dims[0] * dims[1] * dims[2];
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (i, (t, _)) in tris.iter().enumerate() {
            let mut a = [usize::MAX; 3];
            let mut b = [0usize; 3];
            for v in t {
                for k in 0..3 {
                    let c = (((v[k] - lo[k]) / cell).floor().max(0.0) as usize).min(dims[k] - 1);
                    a[k] = a[k].min(c);
                    b[k] = b[k].max(c);
                }
            }
            for z in a[2]..=b[2] {
                for y in a[1]..=b[1] {
                    for x in a[0]..=b[0] {
                        buckets[(z * dims[1] + y) * dims[0] + x].push(i as u32);
                    }
                }
            }
        }
        let mut cells = Vec::with_capacity(n);
        let mut items = Vec::new();
        for b in buckets {
            cells.push((items.len() as u32, b.len() as u32));
            items.extend(b);
        }
        SurfaceGrid { origin: lo, cell, dims, cells, items, tris }
    }

    /// The closest triangle to `p` (game units) within `radius`: its
    /// surface type, its normal (facing `p`) and the closest point.
    pub fn query(&self, p: Vec3, radius: f32) -> Option<(u8, Vec3, Vec3)> {
        if self.cells.is_empty() {
            return None;
        }
        let o = Vec3::from(self.origin);
        let lo = ((p - Vec3::splat(radius) - o) / self.cell).floor();
        let hi = ((p + Vec3::splat(radius) - o) / self.cell).floor();
        let clamp = |v: f32, k: usize| (v.max(0.0) as usize).min(self.dims[k] - 1);
        if hi.x < 0.0 || hi.y < 0.0 || hi.z < 0.0 {
            return None;
        }
        let (x0, y0, z0) = (clamp(lo.x, 0), clamp(lo.y, 1), clamp(lo.z, 2));
        let (x1, y1, z1) = (clamp(hi.x, 0), clamp(hi.y, 1), clamp(hi.z, 2));
        let mut best: Option<(f32, u32, Vec3)> = None;
        for z in z0..=z1 {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (s, n) = self.cells[(z * self.dims[1] + y) * self.dims[0] + x];
                    for &ti in &self.items[s as usize..(s + n) as usize] {
                        let (t, _) = &self.tris[ti as usize];
                        let q = closest_on_triangle(p, Vec3::from(t[0]), Vec3::from(t[1]), Vec3::from(t[2]));
                        let d = q.distance_squared(p);
                        if d <= radius * radius && best.is_none_or(|b| d < b.0) {
                            best = Some((d, ti, q));
                        }
                    }
                }
            }
        }
        let (_, ti, q) = best?;
        let (t, surf) = &self.tris[ti as usize];
        let (a, b, c) = (Vec3::from(t[0]), Vec3::from(t[1]), Vec3::from(t[2]));
        let mut n = (b - a).cross(c - a).normalize_or_zero();
        if n.dot(p - q) < 0.0 {
            n = -n;
        }
        Some((*surf, n, q))
    }
}

/// Closest point to `p` on triangle `abc` (Ericson, Real-Time Collision
/// Detection 5.1.5).
pub fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Everything the effect player needs, read from the zones.
#[derive(Default)]
pub struct FxData {
    pub effects: HashMap<String, Arc<EffectDef>>,
    pub materials: Vec<FxMaterial>,
    pub images: HashMap<String, Image>,
    /// Rows of the impact table (`FxImpactTable`).
    pub impacts: Vec<t4fx::ImpactEntry>,
    /// Our weapon id -> its effects.
    pub weapons: HashMap<String, t4fx::WeaponFx>,
    /// `level._effect["key"]` -> effect name, from the zones' scripts.
    pub level_effects: HashMap<String, String>,
    /// Power-up name (`add_zombie_powerup`) -> its effect.
    pub powerup_effects: HashMap<String, String>,
    pub placed: Vec<Placed>,
    pub surfaces: SurfaceGrid,
    /// Models effects spawn (shells, gibs, splinters).
    pub models: Vec<String>,
    /// Sound aliases effects play.
    pub sound_aliases: Vec<String>,
}

fn key(name: &str) -> String {
    name.trim_start_matches(',').replace('\\', "/").to_ascii_lowercase()
}

/// The first two double-quoted strings in `s`.
fn quoted(s: &str) -> Vec<&str> {
    s.split('"').skip(1).step_by(2).collect()
}

/// `level._effect["key"] = loadfx("name")` and `add_zombie_powerup(name,
/// model, hint, fx)` in a script.
fn parse_script(text: &str, level: &mut HashMap<String, String>, powerups: &mut HashMap<String, String>) {
    for line in text.lines() {
        let l = line.trim_start();
        if l.starts_with("//") {
            continue;
        }
        let lower = l.to_ascii_lowercase();
        if lower.contains("_effect[") && lower.contains("loadfx") {
            let q = quoted(l);
            if q.len() >= 2 {
                level.entry(q[0].to_string()).or_insert_with(|| key(q[1]));
            }
        } else if lower.starts_with("add_zombie_powerup(") {
            let q = quoted(l);
            if q.len() >= 3 {
                powerups.entry(q[0].to_string()).or_insert_with(|| key(q[q.len() - 1]));
            }
        }
    }
}

/// `(x, y, z)` after an `=`.
fn vec3_after_eq(line: &str) -> Option<[f32; 3]> {
    let s = line.split('=').nth(1)?;
    let inner = s.split('(').nth(1)?.split(')').next()?;
    let v: Vec<f32> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

/// The createfx script's placed effects.
pub fn parse_createfx(text: &str) -> Vec<Placed> {
    let mut out: Vec<Placed> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("//") {
            continue;
        }
        let lower = l.to_ascii_lowercase();
        if lower.contains("createloopeffect(") || lower.contains("createoneshoteffect(") {
            let looped = lower.contains("createloopeffect(");
            let id = quoted(l).first().map(|s| s.to_string()).unwrap_or_default();
            // The defaults `createLoopEffect` / `createOneshotEffect` set.
            out.push(Placed { id, origin: [0.0; 3], angles: [0.0; 3], delay: if looped { 0.5 } else { -15.0 }, looped });
        } else if let Some(p) = out.last_mut() {
            if lower.starts_with("ent.v[") {
                let field = quoted(l).first().copied().unwrap_or("");
                match field {
                    "origin" => p.origin = vec3_after_eq(l).unwrap_or(p.origin),
                    "angles" => p.angles = vec3_after_eq(l).unwrap_or(p.angles),
                    "fxid" => {
                        if let Some(v) = quoted(l).get(1) {
                            p.id = v.to_string();
                        }
                    }
                    "delay" => {
                        if let Some(v) = l.split('=').nth(1).and_then(|s| s.trim().trim_end_matches(';').trim().parse().ok()) {
                            p.delay = v;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

/// Bullet impact types use two rows each (entry, exit) with one shared
/// underwater row after the small bullets; the other types follow in
/// order: `impactType` 1 small, 2 large, 3 armour-piercing, 4 shotgun,
/// 5 grenade bounce, 6 grenade explosion, 7 rifle grenade, 8 rocket...
pub fn impact_row(impact_type: i32, exit: bool) -> Option<usize> {
    let e = exit as usize;
    Some(match impact_type {
        1 => e,
        2 => 3 + e,
        3 => 5 + e,
        4 => 7 + e,
        5..=11 => (impact_type + 4) as usize,
        _ => return None,
    })
}

struct Gather<'a> {
    zones: &'a [&'a ZoneData],
    iwd: &'a Iwd,
    bc: bool,
    out: FxData,
    mat_index: HashMap<String, usize>,
    models: HashSet<String>,
    sounds: HashSet<String>,
}

impl Gather<'_> {
    /// The defining (non-stub) effect named `name` and its zone.
    fn find(&self, name: &str) -> Option<(usize, &t4fx::Effect)> {
        let k = key(name);
        self.zones.iter().enumerate().find_map(|(zi, z)| z.fx.iter().find(|f| !f.is_stub() && key(&f.name) == k).map(|f| (zi, f)))
    }

    fn image(&mut self, name: &str) -> Option<String> {
        let name = key(name);
        if self.out.images.contains_key(&name) {
            return Some(name);
        }
        let bytes = self.iwd.read_image(&name)?;
        let iwi = Iwi::parse(&bytes).ok()?;
        self.out.images.insert(name.clone(), crate::waw::iwi_to_image(&iwi, true, false, self.bc));
        Some(name)
    }

    /// Our material for zone `zi`'s material `idx` (stubs resolved by name).
    fn material(&mut self, zi: usize, idx: u32) -> Option<usize> {
        let zones = self.zones;
        let mut m = zones[zi].materials.get(idx as usize)?;
        let mut mz = zi;
        if m.name.starts_with(',') || m.techset.is_none() {
            let real = m.name.trim_start_matches(',');
            if let Some((z, rm)) = zones.iter().enumerate().find_map(|(z, zd)| zd.materials.iter().find(|x| x.name == real && x.techset.is_some()).map(|x| (z, x))) {
                m = rm;
                mz = z;
            }
        }
        let name = m.name.trim_start_matches(',').to_string();
        if let Some(&i) = self.mat_index.get(&name) {
            return (i != usize::MAX).then_some(i);
        }
        let techset = m.techset.as_deref().unwrap_or("");
        if techset.contains("distortion") {
            // Heat haze: its colour is distortion strength (e.g. the blue
            // ring of the shotgun flash), not something to draw. We have no
            // refraction pass, so these elements are left out.
            self.mat_index.insert(name, usize::MAX);
            return None;
        }
        let image_name = m.color_map().map(|t| zones[mz].image_name(t).to_string());
        let image = image_name.and_then(|n| self.image(&n));
        if image.is_none() {
            // E.g. IWI format 9 (fxt_light_spot_beam), which we can't decode:
            // drawing it untextured would show a solid quad.
            warn!("fx material {name}: texture not readable, its elements are skipped");
            self.mat_index.insert(name, usize::MAX);
            return None;
        }
        let blend = Blend::from_techset(techset);
        let feather = if techset.contains("zfeather") {
            zones[mz].material_constant(m, "featherParms").map(|f| f[1]).filter(|d| *d > 0.0).unwrap_or(8.0) * crate::nacht::build::INCH
        } else {
            0.0
        };
        let i = self.out.materials.len();
        self.out.materials.push(FxMaterial { name: name.clone(), image, blend, atlas: m.atlas, feather });
        self.mat_index.insert(name, i);
        Some(i)
    }

    /// Adds effect `name` and everything it spawns.
    fn add(&mut self, name: &str) {
        let k = key(name);
        if k.is_empty() || k.contains([' ', '+']) || self.out.effects.contains_key(&k) {
            return;
        }
        let Some((zi, def)) = self.find(&k) else {
            debug!("effect {k} not found in the loaded zones");
            return;
        };
        let def = def.clone();
        // Reserve the name first (effects can refer to themselves).
        self.out.effects.insert(k.clone(), Arc::new(EffectDef { name: k.clone(), looping_life: None, looping: 0, oneshot: 0, elems: Vec::new() }));
        let mut children = Vec::new();
        let mut elems = Vec::with_capacity(def.elems.len());
        for e in &def.elems {
            let mut visuals = Vec::new();
            for v in &e.visuals {
                match v {
                    t4fx::Visual::Material(m) => {
                        if let Some(i) = self.material(zi, *m) {
                            visuals.push(Visual::Material(i));
                        }
                    }
                    t4fx::Visual::Model(m) => {
                        let n = self.zones[zi].xmodels[*m as usize].name.trim_start_matches(',').to_string();
                        if n.starts_with("fx_decal_") {
                            // Splats the engine sticks onto the character
                            // that was hit (fx_decal_character_blood, 20 s);
                            // as a free model it hung in the air.
                            continue;
                        }
                        self.models.insert(n.clone());
                        visuals.push(Visual::Model(n));
                    }
                    t4fx::Visual::Effect(n) => {
                        children.push(n.clone());
                        visuals.push(Visual::Effect(key(n)));
                    }
                    t4fx::Visual::Sound(s) => {
                        self.sounds.insert(s.clone());
                        visuals.push(Visual::Sound(s.clone()));
                    }
                    t4fx::Visual::Mark([_, w]) => {
                        if let Some(i) = w.and_then(|w| self.material(zi, w)) {
                            visuals.push(Visual::Decal(i));
                        }
                    }
                }
            }
            for n in [&e.effect_on_death, &e.effect_on_impact, &e.effect_emitted].into_iter().flatten() {
                children.push(n.clone());
            }
            elems.push(ElemDef { e: e.clone(), visuals });
        }
        let looping_life = (def.msec_looping_life > 0 && def.msec_looping_life < i32::MAX).then(|| def.msec_looping_life as f32 / 1000.0);
        self.out.effects.insert(k.clone(), Arc::new(EffectDef { name: k, looping_life, looping: def.looping, oneshot: def.oneshot, elems }));
        for c in children {
            self.add(&c);
        }
    }
}

/// World triangles and their surface types (static geometry, no decals).
fn surface_grid(zone: &ZoneData) -> SurfaceGrid {
    let Some(w) = zone.world.as_ref() else { return SurfaceGrid::default() };
    let mut tris = Vec::new();
    for (i, s) in w.surfaces.iter().enumerate() {
        if w.decal_range.contains(&(i as u32)) {
            continue;
        }
        let Some(m) = s.material.and_then(|m| zone.materials.get(m as usize)) else { continue };
        let t = m.techset.as_deref().unwrap_or("");
        if crate::nacht::build::skip_material(t, &m.name) {
            continue;
        }
        let surf = if m.surface_type_bits == 0 { 0 } else { m.surface_type_bits.trailing_zeros().min(30) as u8 };
        for tri in decode::world_triangles(zone, w, s) {
            let v = tri.map(|vi| decode::world_vertex(zone, w, vi).map(|x| x.pos).unwrap_or([0.0; 3]));
            tris.push((v, surf));
        }
    }
    SurfaceGrid::build(tris, 64.0)
}

/// Reads the effects a map uses from its zones (`zones[0]` is the map).
/// `weapons` pairs our weapon ids with the zone's weapon names.
pub fn extract(zones: &[&ZoneData], iwd: &Iwd, bc: bool, weapons: &[(&str, &str)]) -> FxData {
    let t0 = std::time::Instant::now();
    let mut g = Gather { zones, iwd, bc, out: FxData::default(), mat_index: HashMap::new(), models: HashSet::new(), sounds: HashSet::new() };
    // Script names, the map's own zone first.
    let (mut level, mut powerups) = (HashMap::new(), HashMap::new());
    for z in zones {
        for (name, ..) in &z.rawfiles {
            if name.ends_with(".gsc") && !name.contains("createfx/") {
                if let Some(text) = z.rawfile(name) {
                    parse_script(&text, &mut level, &mut powerups);
                }
            }
        }
    }
    // Placed effects: `maps/createfx/<map>_fx.gsc` in the map's zone.
    let createfx = zones[0].rawfiles.iter().find(|(n, ..)| n.starts_with("maps/createfx/") && n.ends_with("_fx.gsc")).map(|(n, ..)| n.clone());
    if let Some(text) = createfx.as_deref().and_then(|n| zones[0].rawfile(n)) {
        g.out.placed = parse_createfx(&text);
    }
    // Weapons and the impact table.
    for &(id, zname) in weapons {
        if let Some(w) = zones.iter().find_map(|z| z.weapon(zname)) {
            g.out.weapons.insert(id.to_string(), w.fx.clone());
        }
    }
    g.out.impacts = zones.iter().find_map(|z| z.impact_fx.iter().find(|t| !t.entries.is_empty())).map(|t| t.entries.clone()).unwrap_or_default();

    // Everything reachable from what the game plays.
    let mut roots: Vec<String> = Vec::new();
    for w in g.out.weapons.values() {
        roots.extend(
            [&w.view_flash, &w.world_flash, &w.view_shell_eject, &w.world_shell_eject, &w.proj_explosion, &w.proj_trail].into_iter().flatten().cloned(),
        );
        for exit in [false, true] {
            if let Some(row) = impact_row(w.impact_type, exit).and_then(|r| g.out.impacts.get(r)) {
                roots.extend(row.nonflesh.iter().chain(row.flesh.iter()).flatten().cloned());
            }
        }
    }
    roots.extend(level.values().cloned());
    roots.extend(powerups.values().cloned());
    for p in &g.out.placed {
        if let Some(n) = level.get(&p.id) {
            roots.push(n.clone());
        }
    }
    for r in roots {
        g.add(&r);
    }
    g.out.level_effects = level;
    g.out.powerup_effects = powerups;
    g.out.surfaces = surface_grid(zones[0]);
    g.out.models = g.models.into_iter().collect();
    g.out.models.sort();
    g.out.sound_aliases = g.sounds.into_iter().collect();
    g.out.sound_aliases.sort();
    let elems: usize = g.out.effects.values().map(|e| e.elems.len()).sum();
    info!(
        "Effects: {} ({} elements), {} materials, {} textures, {} placed, {} script names, impact rows {}, surface triangles {}, in {:.2}s",
        g.out.effects.len(),
        elems,
        g.out.materials.len(),
        g.out.images.len(),
        g.out.placed.len(),
        g.out.level_effects.len(),
        g.out.impacts.len(),
        g.out.surfaces.tris.len(),
        t0.elapsed().as_secs_f32()
    );
    g.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_createfx_entries() {
        let text = r#"
     	ent = maps\_utility::createOneshotEffect( "fog_thick" );
     	ent.v[ "origin" ] = ( -673.074, -766.968, -26.5289 );
     	ent.v[ "angles" ] = ( 270, 0, 0 );
     	ent.v[ "fxid" ] = "fog_thick";
     	ent.v[ "delay" ] = -15;

     	ent = maps\_utility::createLoopEffect( "god_rays_small" );
     	ent.v[ "origin" ] = ( 1, 2, 3 );
     	ent.v[ "delay" ] = 0.1;
"#;
        let p = parse_createfx(text);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].id, "fog_thick");
        assert!(!p[0].looped && p[0].delay == -15.0);
        assert_eq!(p[0].angles, [270.0, 0.0, 0.0]);
        assert!((p[0].origin[0] + 673.074).abs() < 1e-3);
        assert!(p[1].looped && (p[1].delay - 0.1).abs() < 1e-6 && p[1].angles == [0.0; 3]);
    }

    #[test]
    fn parses_level_effects() {
        let text = r#"
	level._effect["wood_chunk_destory"]	 	= loadfx( "impacts/large_woodhit" );
	level._effect["eye_glow"]			 	= LoadFx( "misc/fx_zombie_eye_single" );
//	level._effect["x"] = loadfx("y");
	add_zombie_powerup( "nuke", 		"zombie_bomb",		&"ZOMBIE_POWERUP_NUKE", 			"misc/fx_zombie_mini_nuke" );
//	add_zombie_powerup( "nuke", 		"zombie_bomb",		&"ZOMBIE_POWERUP_NUKE", 			"misc/other" );
"#;
        let (mut l, mut p) = (HashMap::new(), HashMap::new());
        parse_script(text, &mut l, &mut p);
        assert_eq!(l.get("wood_chunk_destory").map(String::as_str), Some("impacts/large_woodhit"));
        assert_eq!(l.get("eye_glow").map(String::as_str), Some("misc/fx_zombie_eye_single"));
        assert!(!l.contains_key("x"));
        assert_eq!(p.get("nuke").map(String::as_str), Some("misc/fx_zombie_mini_nuke"));
    }

    #[test]
    fn impact_rows() {
        assert_eq!(impact_row(1, false), Some(0));
        assert_eq!(impact_row(1, true), Some(1));
        assert_eq!(impact_row(2, false), Some(3));
        assert_eq!(impact_row(6, false), Some(10));
        assert_eq!(impact_row(0, false), None);
    }

    #[test]
    fn surface_grid_finds_nearest() {
        let floor = ([[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]], 5u8);
        let wall = ([[0.0, 0.0, 0.0], [0.0, 100.0, 0.0], [0.0, 0.0, 100.0]], 21u8);
        let g = SurfaceGrid::build(vec![floor, wall], 32.0);
        let (s, n, _) = g.query(Vec3::new(20.0, 20.0, 1.0), 4.0).unwrap();
        assert_eq!(s, 5);
        assert!((n - Vec3::Z).length() < 1e-4);
        let (s, n, _) = g.query(Vec3::new(1.5, 30.0, 40.0), 4.0).unwrap();
        assert_eq!(s, 21);
        assert!((n - Vec3::X).length() < 1e-4);
        assert!(g.query(Vec3::new(50.0, 50.0, 50.0), 4.0).is_none());
    }
}
