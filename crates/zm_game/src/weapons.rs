//! Player weapons: loadout, hitscan firing, reloading, knifing, and the
//! procedural first-person viewmodel.

use crate::audio::{PlayAlias, PlaySfx, Sfx, ZoneSounds};
use crate::player::{Player, PlayerCtl};
use crate::world::Mats;
use crate::zombies::{self, Zombie};
use crate::{cursor_locked, earn, ActivePowerups, Defs, Dynamic, GameState, PointsEvent, Score, World, ZombieKilled};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use zm_core::geom::V3;
use zm_core::rules::{self, KillKind};
use zm_core::weapons::{FireMode, Kind, WeaponDef, START_PISTOL};

pub const MAX_SLOTS: usize = 2;
const KNIFE_DAMAGE: f32 = 150.0;
const KNIFE_RANGE: f32 = 1.9;

#[derive(Debug, Clone)]
pub struct Slot {
    pub def: usize,
    pub clip: u32,
    pub reserve: u32,
}

#[derive(Resource)]
pub struct Loadout {
    pub slots: Vec<Slot>,
    pub cur: usize,
}

impl Loadout {
    pub fn starting(defs: &[WeaponDef]) -> Self {
        let d = &defs[START_PISTOL];
        Loadout { slots: vec![Slot { def: START_PISTOL, clip: d.clip, reserve: d.clip * 4 }], cur: 0 }
    }

    pub fn current(&self) -> &Slot {
        &self.slots[self.cur]
    }

    pub fn has(&self, def: usize) -> Option<usize> {
        self.slots.iter().position(|s| s.def == def)
    }

    /// Give a weapon: refills if already owned, fills an empty slot, or
    /// replaces the one in hand.
    pub fn give(&mut self, defs: &[WeaponDef], def: usize) {
        let d = &defs[def];
        let full = Slot { def, clip: d.clip, reserve: d.max_ammo.saturating_sub(d.clip) };
        if let Some(i) = self.has(def) {
            self.slots[i] = full;
            self.cur = i;
        } else if self.slots.len() < MAX_SLOTS {
            self.slots.push(full);
            self.cur = self.slots.len() - 1;
        } else {
            self.slots[self.cur] = full;
        }
    }

    pub fn refill_all(&mut self, defs: &[WeaponDef]) {
        for s in &mut self.slots {
            let d = &defs[s.def];
            s.reserve = d.max_ammo.saturating_sub(s.clip);
        }
    }
}

#[derive(Resource, Default)]
pub struct Gun {
    pub cooldown: f32,
    pub reload: Option<f32>,
    pub switch: f32,
    pub knife_cd: f32,
    pub knife_anim: f32,
    pub kick: f32,
    pub flash: f32,
    pub hitmarker: f32,
    pub headmarker: bool,
    /// Seconds since the last shot (drives the fire/rechamber animation).
    pub since_shot: Option<f32>,
    /// The running reload started with an empty magazine.
    pub reload_empty: bool,
}

/// The first-person rig (arms + gun) spawned for the weapon in hand.
#[derive(Component)]
pub struct ViewRigState {
    joints: Vec<crate::nacht::Joint>,
    weapon: &'static str,
}

/// Short-lived visual effects (sparks, blood).
#[derive(Component)]
pub struct Fx {
    pub ttl: f32,
    pub max: f32,
    pub vel: Vec3,
    pub size: f32,
}

#[derive(Resource, Default)]
pub struct Tracers(pub Vec<(Vec3, Vec3, f32, Color)>);

#[derive(Component)]
pub struct ViewModel {
    pub shown: Option<usize>,
    /// The real arms rig is in use (animations position the gun).
    pub rig: bool,
}

#[derive(Component)]
pub struct GunModel;

#[derive(Component)]
pub struct KnifeModel;

#[derive(Component)]
pub struct MuzzleFlash;

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gun>()
            .init_resource::<Tracers>()
            .add_systems(Startup, spawn_viewmodel.after(crate::player::spawn_player))
            .add_systems(
                Update,
                (switch_weapons, reload, fire, knife).chain().run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, ((update_viewmodel, animate_view_rig).chain(), update_fx, draw_tracers));
    }
}

fn switch_weapons(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut loadout: ResMut<Loadout>,
    mut gun: ResMut<Gun>,
    (defs, zs, mut alias): (Res<Defs>, Res<ZoneSounds>, EventWriter<PlayAlias>),
) {
    let n = loadout.slots.len();
    if n < 2 {
        return;
    }
    let mut target = None;
    if keys.just_pressed(KeyCode::Digit1) {
        target = Some(0);
    } else if keys.just_pressed(KeyCode::Digit2) {
        target = Some(1);
    } else if keys.just_pressed(KeyCode::KeyQ) || scroll.delta.y.abs() > 0.1 {
        target = Some((loadout.cur + 1) % n);
    }
    if let Some(t) = target.filter(|t| *t != loadout.cur && *t < n) {
        loadout.cur = t;
        gun.switch = 0.45;
        gun.reload = None;
        gun.cooldown = 0.0;
        let id = defs.0[loadout.slots[t].def].id;
        if let Some(a) = weapon_sound(&zs, id, "raiseSoundPlayer", Sfx::Reload) {
            alias.write(a.volume(0.6));
        }
    }
}

fn reload(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    defs: Res<Defs>,
    mut loadout: ResMut<Loadout>,
    mut gun: ResMut<Gun>,
    zs: Res<ZoneSounds>,
    mut alias: EventWriter<PlayAlias>,
) {
    let dt = time.delta_secs();
    gun.switch = (gun.switch - dt).max(0.0);
    let cur = loadout.cur;
    let def = &defs.0[loadout.slots[cur].def];
    if let Some(left) = gun.reload.as_mut() {
        *left -= dt;
        if *left <= 0.0 {
            gun.reload = None;
            let slot = &mut loadout.slots[cur];
            let need = def.clip - slot.clip;
            let take = need.min(slot.reserve);
            slot.clip += take;
            slot.reserve -= take;
            // With game data the reload anim's notetracks make the sounds.
            if !zs.weapons.contains_key(def.id) {
                alias.write(PlayAlias::local("").or(Sfx::Reload).volume(0.7));
            }
        }
        return;
    }
    let slot = &loadout.slots[cur];
    let wants = keys.just_pressed(KeyCode::KeyR) || slot.clip == 0;
    if wants && slot.clip < def.clip && slot.reserve > 0 && gun.switch <= 0.0 {
        gun.reload = Some(def.reload_time);
        gun.reload_empty = slot.clip == 0;
        let field = if gun.reload_empty { "reloadEmptySoundPlayer" } else { "reloadSoundPlayer" };
        if let Some(a) = weapon_sound(&zs, def.id, field, Sfx::Reload) {
            alias.write(a.volume(0.5));
        }
    }
}

/// A sound a weapon definition names (`field`), or the procedural stand-in
/// when the weapon has no game data (prototype map without an install).
pub fn weapon_sound(zs: &ZoneSounds, weapon: &str, field: &str, fallback: Sfx) -> Option<PlayAlias> {
    if zs.weapons.contains_key(weapon) {
        zs.weapon_field(weapon, field).map(PlayAlias::local)
    } else {
        Some(PlayAlias::local("").or(fallback))
    }
}

/// Uniformly random direction inside a cone around `dir`.
fn spread_dir(dir: Vec3, right: Vec3, up: Vec3, half_angle_deg: f32) -> Vec3 {
    if half_angle_deg <= 0.0 {
        return dir;
    }
    let r = half_angle_deg.to_radians().tan() * fastrand::f32().sqrt();
    let a = fastrand::f32() * std::f32::consts::TAU;
    (dir + right * (a.cos() * r) + up * (a.sin() * r)).normalize()
}

fn to_v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

/// Distance to the first solid along the ray.
pub fn wall_distance(world: &World, origin: Vec3, dir: Vec3, max: f32) -> f32 {
    let (o, d) = (to_v3(origin), to_v3(dir));
    let solids = world.bullet_solids.iter().filter_map(|s| s.ray_hit(o, d, max)).fold(max, f32::min);
    match &world.mesh {
        Some(m) => m.raycast(o, d, solids).map_or(solids, |h| h.t),
        None => solids,
    }
}

#[allow(clippy::type_complexity)]
fn fire(
    (mouse, time, windows): (Res<ButtonInput<MouseButton>>, Res<Time>, Query<&Window, With<PrimaryWindow>>),
    (defs, world, pu): (Res<Defs>, Res<World>, Res<ActivePowerups>),
    (mut loadout, mut gun, mut score, mut tracers): (ResMut<Loadout>, ResMut<Gun>, ResMut<Score>, ResMut<Tracers>),
    mut player: Query<(&Transform, &mut PlayerCtl), With<Player>>,
    mut zq: Query<(Entity, &Transform, &mut Zombie), Without<Player>>,
    (mut sfx, mut points, mut killed): (EventWriter<PlaySfx>, EventWriter<PointsEvent>, EventWriter<ZombieKilled>),
    (zs, mut alias): (Res<ZoneSounds>, EventWriter<PlayAlias>),
    mut commands: Commands,
    mats: Res<Mats>,
) {
    let dt = time.delta_secs();
    gun.cooldown = (gun.cooldown - dt).max(0.0);
    if let Some(t) = gun.since_shot.as_mut() {
        *t += dt;
    }
    gun.kick = (gun.kick - dt * 6.0).max(0.0);
    gun.flash = (gun.flash - dt).max(0.0);
    gun.hitmarker = (gun.hitmarker - dt).max(0.0);

    if !cursor_locked(&windows) || gun.switch > 0.0 || gun.reload.is_some() || gun.knife_anim > 0.0 {
        return;
    }
    let cur = loadout.cur;
    let def = defs.0[loadout.slots[cur].def].clone();
    let trigger = match def.mode {
        FireMode::Auto => mouse.pressed(MouseButton::Left),
        FireMode::Semi => mouse.just_pressed(MouseButton::Left),
    };
    if !trigger || gun.cooldown > 0.0 {
        return;
    }
    if loadout.slots[cur].clip == 0 {
        if mouse.just_pressed(MouseButton::Left) {
            if let Some(a) = weapon_sound(&zs, def.id, "emptyFireSoundPlayer", Sfx::DryFire) {
                alias.write(a.volume(0.6));
            }
        }
        return;
    }
    let Ok((cam, mut ctl)) = player.single_mut() else { return };
    loadout.slots[cur].clip -= 1;
    gun.cooldown = def.fire_interval.max(0.04);
    gun.since_shot = Some(0.0);
    gun.kick = 1.0;
    gun.flash = 0.05;
    ctl.recoil += def.kick * 0.012 * (1.0 - 0.5 * ctl.ads);
    // The last round may have its own sound (the Garand's ping).
    match zs.weapon_field(def.id, "fireLastSoundPlayer").filter(|_| loadout.slots[cur].clip == 0) {
        Some(last) => {
            alias.write(PlayAlias::local(last));
        }
        None => {
            sfx.write(PlaySfx::weapon(Sfx::for_weapon(def.kind), def.id));
        }
    }

    let origin = cam.translation;
    let fwd = cam.forward().as_vec3();
    let right = cam.right().as_vec3();
    let up = cam.up().as_vec3();
    let muzzle = origin + right * 0.2 - up * 0.14 + fwd * 0.7;
    let ads_factor = if def.kind == Kind::Shotgun { 1.0 - 0.3 * ctl.ads } else { 1.0 - 0.75 * ctl.ads };
    let move_factor = if ctl.moving { 1.6 } else { 1.0 };
    let insta = pu.insta_kill > 0.0;
    let mut any_hit = false;
    let mut any_head = false;

    for _ in 0..def.pellets.max(1) {
        let dir = spread_dir(fwd, right, up, def.spread * ads_factor * move_factor);
        let max = 120.0;
        let wall_t = wall_distance(&world, origin, dir, max);
        let mut best: Option<(f32, bool, Entity)> = None;
        for (e, t, z) in zq.iter() {
            if !z.alive() {
                continue;
            }
            if let Some((d, head)) = zombies::hit_test(origin, dir, t, &z, wall_t) {
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, head, e));
                }
            }
        }
        let hit = best.and_then(|(d, head, e)| zq.get_mut(e).ok().map(|(_, _, z)| (d, head, z)));

        let end_t = hit.as_ref().map(|h| h.0).unwrap_or(wall_t);
        let end = origin + dir * end_t;
        let color = if def.kind == Kind::Wonder { Color::srgb(0.3, 1.0, 0.4) } else { Color::srgba(1.0, 0.9, 0.6, 0.6) };
        tracers.0.push((muzzle, end, if def.kind == Kind::Wonder { 0.12 } else { 0.035 }, color));

        if let Some((d, head, mut z)) = hit {
            any_hit = true;
            any_head |= head;
            let mut dmg = def.damage_at(d) * if head { def.head_mult } else { 1.0 };
            if def.kind == Kind::Wonder {
                dmg = def.damage;
            }
            let was_alive = z.alive();
            if zombies::apply_damage(&mut z, dmg, insta) && was_alive {
                let kind = if def.kind == Kind::Wonder { KillKind::Explosive } else if head { KillKind::Head } else { KillKind::Body };
                if head {
                    score.headshots += 1;
                    // The head pops.
                    alias.write(PlayAlias::at("zombie_head_gib", end).or(Sfx::Headshot).volume(0.8));
                }
                earn(&mut score, &mut points, &pu, rules::kill_points(kind));
                killed.write(ZombieKilled { pos: end, drop_allowed: true });
                spawn_burst(&mut commands, &mats, end, &mats.blood, 10, 2.5);
            } else {
                earn(&mut score, &mut points, &pu, rules::POINTS_HIT);
                spawn_burst(&mut commands, &mats, end, &mats.blood, 4, 1.5);
            }
        } else if end_t < max {
            spawn_burst(&mut commands, &mats, end - dir * 0.05, &mats.spark, 3, 2.0);
        }

        // Splash damage (Ray Pistol).
        if def.splash_radius > 0.0 {
            spawn_burst(&mut commands, &mats, end, &mats.glow_green, 14, 4.0);
            for (_, t, mut z) in zq.iter_mut() {
                if !z.alive() {
                    continue;
                }
                let c = t.translation + Vec3::Y * 1.0;
                let dist = c.distance(end);
                if dist < def.splash_radius {
                    let falloff = 1.0 - dist / def.splash_radius;
                    let dmg = def.splash_damage * (0.5 + 0.5 * falloff);
                    if zombies::apply_damage(&mut z, dmg, insta) {
                        earn(&mut score, &mut points, &pu, rules::kill_points(KillKind::Explosive));
                        killed.write(ZombieKilled { pos: t.translation, drop_allowed: true });
                    } else {
                        earn(&mut score, &mut points, &pu, rules::POINTS_HIT);
                    }
                    any_hit = true;
                }
            }
        }
    }
    if any_hit {
        gun.hitmarker = 0.12;
        gun.headmarker = any_head;
        // Nacht has no hit sound; keep the stand-in for the prototype map.
        if zs.aliases.is_empty() {
            sfx.write(PlaySfx::at(if any_head { Sfx::Headshot } else { Sfx::Hit }, 0.5));
        }
    }
}

#[allow(clippy::type_complexity)]
fn knife(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    pu: Res<ActivePowerups>,
    mut gun: ResMut<Gun>,
    mut score: ResMut<Score>,
    player: Query<&Transform, With<Player>>,
    mut zq: Query<(&Transform, &mut Zombie), Without<Player>>,
    (mut points, mut killed): (EventWriter<PointsEvent>, EventWriter<ZombieKilled>),
    (zs, defs, loadout, mut alias): (Res<ZoneSounds>, Res<Defs>, Option<Res<Loadout>>, EventWriter<PlayAlias>),
    mut commands: Commands,
    mats: Res<Mats>,
) {
    let dt = time.delta_secs();
    gun.knife_cd = (gun.knife_cd - dt).max(0.0);
    gun.knife_anim = (gun.knife_anim - dt).max(0.0);
    if !cursor_locked(&windows) || !(keys.just_pressed(KeyCode::KeyV) || keys.just_pressed(KeyCode::KeyE)) || gun.knife_cd > 0.0 {
        return;
    }
    let Ok(cam) = player.single() else { return };
    gun.knife_cd = 0.65;
    gun.knife_anim = 0.35;
    gun.reload = None;
    let held = loadout.map(|l| defs.0[l.current().def].id).unwrap_or("");
    if let Some(a) = weapon_sound(&zs, held, "meleeSwipeSoundPlayer", Sfx::Knife) {
        alias.write(a.volume(0.8));
    }
    let fwd = cam.forward().as_vec3();
    let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
    let me = Vec3::new(cam.translation.x, 0.0, cam.translation.z);
    let target = zq
        .iter_mut()
        .filter(|(_, z)| z.alive())
        .filter_map(|(t, z)| {
            let p = Vec3::new(t.translation.x, 0.0, t.translation.z);
            let d = p.distance(me);
            let facing = (p - me).normalize_or_zero().dot(flat);
            (d < KNIFE_RANGE && facing > 0.55).then_some((d, t.translation, z))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, pos, mut z)) = target {
        let hit_point = pos + Vec3::Y * 1.2;
        alias.write(PlayAlias::at("melee_hit", hit_point));
        if zombies::apply_damage(&mut z, KNIFE_DAMAGE, pu.insta_kill > 0.0) {
            earn(&mut score, &mut points, &pu, rules::kill_points(KillKind::Melee));
            killed.write(ZombieKilled { pos, drop_allowed: true });
            spawn_burst(&mut commands, &mats, hit_point, &mats.blood, 10, 2.5);
        } else {
            earn(&mut score, &mut points, &pu, rules::POINTS_HIT);
            spawn_burst(&mut commands, &mats, hit_point, &mats.blood, 5, 1.5);
        }
        gun.hitmarker = 0.12;
        gun.headmarker = false;
    }
}

pub fn spawn_burst(commands: &mut Commands, mats: &Mats, at: Vec3, mat: &Handle<StandardMaterial>, n: usize, speed: f32) {
    for _ in 0..n {
        let v = Vec3::new(fastrand::f32() - 0.5, fastrand::f32() * 0.8, fastrand::f32() - 0.5).normalize_or_zero() * speed * (0.4 + fastrand::f32());
        let size = 0.03 + fastrand::f32() * 0.05;
        commands.spawn((
            Mesh3d(mats.sphere.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            Fx { ttl: 0.5, max: 0.5, vel: v, size },
            NotShadowCaster,
            Dynamic,
        ));
    }
}

fn update_fx(time: Res<Time>, mut commands: Commands, mut q: Query<(Entity, &mut Transform, &mut Fx)>) {
    let dt = time.delta_secs();
    for (e, mut t, mut fx) in &mut q {
        fx.ttl -= dt;
        if fx.ttl <= 0.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        fx.vel.y -= 9.0 * dt;
        t.translation += fx.vel * dt;
        if t.translation.y < 0.02 {
            t.translation.y = 0.02;
            fx.vel = Vec3::ZERO;
        }
        t.scale = Vec3::splat(fx.size * (fx.ttl / fx.max).max(0.2));
    }
}

fn draw_tracers(time: Res<Time>, mut tracers: ResMut<Tracers>, mut gizmos: Gizmos) {
    let dt = time.delta_secs();
    for (a, b, _, c) in &tracers.0 {
        gizmos.line(*a, *b, *c);
    }
    tracers.0.iter_mut().for_each(|t| t.2 -= dt);
    tracers.0.retain(|t| t.2 > 0.0);
}

fn spawn_viewmodel(mut commands: Commands, player: Query<Entity, With<Player>>, mats: Res<Mats>, mut meshes: ResMut<Assets<Mesh>>) {
    let Ok(cam) = player.single() else { return };
    let root = commands
        .spawn((Transform::from_xyz(0.22, -0.2, -0.45), Visibility::default(), ViewModel { shown: None, rig: false }))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.02, 0.025, 0.28))),
                MeshMaterial3d(mats.gun_metal.clone()),
                Transform::from_xyz(-0.08, -0.02, -0.1),
                Visibility::Hidden,
                KnifeModel,
                NotShadowCaster,
            ));
            p.spawn((
                PointLight { color: Color::srgb(1.0, 0.7, 0.3), intensity: 0.0, range: 6.0, ..default() },
                Mesh3d(mats.sphere.clone()),
                MeshMaterial3d(mats.spark.clone()),
                Transform::from_xyz(0.0, 0.02, -0.55).with_scale(Vec3::ZERO),
                MuzzleFlash,
                NotShadowCaster,
            ));
        })
        .id();
    commands.entity(cam).add_child(root);
}

fn build_gun(p: &mut ChildSpawnerCommands, def: &WeaponDef, mats: &Mats) {
    // Parts are authored at full size; the GunModel parent scales them down.
    let part = |p: &mut ChildSpawnerCommands, size: Vec3, pos: Vec3, m: &Handle<StandardMaterial>| {
        p.spawn((
            Mesh3d(mats.cube.clone()),
            MeshMaterial3d(m.clone()),
            Transform::from_translation(pos).with_scale(size),
            NotShadowCaster,
        ));
    };
    let (len, stock, mag) = match def.kind {
        Kind::Pistol => (0.18, false, false),
        Kind::Smg => (0.42, true, true),
        Kind::Shotgun => (0.62, true, false),
        Kind::Lmg => (0.68, true, true),
        Kind::Rifle => (0.7, true, def.clip > 8),
        Kind::Wonder => (0.26, false, false),
    };
    if def.kind == Kind::Wonder {
        part(p, Vec3::new(0.08, 0.1, 0.2), Vec3::new(0.0, 0.0, -0.05), &mats.gun_metal);
        part(p, Vec3::new(0.05, 0.12, 0.06), Vec3::new(0.0, -0.09, 0.03), &mats.gun_wood);
        for i in 0..3 {
            part(p, Vec3::new(0.1, 0.1, 0.02), Vec3::new(0.0, 0.0, -0.17 - i as f32 * 0.05), &mats.glow_green);
        }
        part(p, Vec3::new(0.03, 0.03, 0.12), Vec3::new(0.0, 0.0, -0.3), &mats.glow_red);
        return;
    }
    part(p, Vec3::new(0.06, 0.08, len * 0.55), Vec3::new(0.0, 0.0, -len * 0.2), &mats.gun_metal);
    part(p, Vec3::new(0.025, 0.025, len * 0.5), Vec3::new(0.0, 0.02, -len * 0.62), &mats.gun_metal);
    part(p, Vec3::new(0.05, 0.04, len * 0.45), Vec3::new(0.0, -0.04, -len * 0.4), &mats.gun_wood);
    part(p, Vec3::new(0.045, 0.1, 0.05), Vec3::new(0.0, -0.08, 0.02), &mats.gun_wood);
    if stock {
        part(p, Vec3::new(0.05, 0.09, 0.24), Vec3::new(0.0, -0.03, 0.18), &mats.gun_wood);
    }
    if mag {
        part(p, Vec3::new(0.035, 0.14, 0.05), Vec3::new(0.0, -0.11, -len * 0.25), &mats.gun_metal);
    }
    if def.kind == Kind::Shotgun && def.clip == 2 {
        part(p, Vec3::new(0.025, 0.025, len * 0.5), Vec3::new(0.03, 0.02, -len * 0.62), &mats.gun_metal);
    }
}

#[allow(clippy::type_complexity)]
fn update_viewmodel(
    mut commands: Commands,
    time: Res<Time>,
    defs: Res<Defs>,
    loadout: Option<Res<Loadout>>,
    gun: Res<Gun>,
    mats: Res<Mats>,
    view_models: Res<crate::nacht::ViewModels>,
    view_rig: Res<crate::nacht::ViewRig>,
    player: Query<&PlayerCtl, With<Player>>,
    mut vm: Query<(Entity, &mut Transform, &mut ViewModel), (Without<KnifeModel>, Without<MuzzleFlash>)>,
    models: Query<Entity, With<GunModel>>,
    mut knife: Query<(&mut Transform, &mut Visibility), (With<KnifeModel>, Without<MuzzleFlash>, Without<ViewModel>)>,
    mut flash: Query<(Entity, &mut Transform, &mut PointLight), (With<MuzzleFlash>, Without<KnifeModel>, Without<ViewModel>)>,
) {
    let Some(loadout) = loadout else { return };
    let Ok((root, mut t, mut vm)) = vm.single_mut() else { return };
    let Ok(ctl) = player.single() else { return };
    let def_idx = loadout.current().def;
    if vm.shown != Some(def_idx) || ((view_models.is_changed() || view_rig.is_changed()) && !view_models.0.is_empty()) {
        // Keep the muzzle flash: it may be attached to the old gun's flash tag.
        if let Ok((fe, ..)) = flash.single() {
            commands.entity(fe).insert(ChildOf(root));
        }
        for m in &models {
            commands.entity(m).try_despawn();
        }
        let def = defs.0[def_idx].clone();
        vm.shown = Some(def_idx);
        // The game's arms with the weapon on them, animated.
        if let (Some(arms), Some(gun_part)) = (&view_rig.arms, view_rig.guns.get(def.id)) {
            let rig_root = commands
                .spawn((
                    Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                    Visibility::default(),
                    GunModel,
                    ChildOf(root),
                ))
                .id();
            let mut joints = Vec::new();
            crate::nacht::spawn_part(&mut commands, arms, &mut joints, rig_root, rig_root);
            let tag_weapon = joints.iter().find(|j| j.0 == "tag_weapon").map(|j| j.1).unwrap_or(rig_root);
            crate::nacht::spawn_part(&mut commands, gun_part, &mut joints, tag_weapon, rig_root);
            if let (Some(tag_flash), Ok((fe, ..))) = (joints.iter().find(|j| j.0 == "tag_flash").map(|j| j.1), flash.single()) {
                commands.entity(fe).insert(ChildOf(tag_flash));
            }
            commands.entity(rig_root).insert(ViewRigState { joints, weapon: def.id });
            vm.rig = true;
        } else {
            vm.rig = false;
        }
        let mats_ref: &Mats = &mats;
        let real = if vm.rig { None } else { view_models.0.get(def.id).cloned() };
        let rig = vm.rig;
        commands.entity(root).with_children(|p| match real {
            // The game's own gun model: grip at the origin, barrel along +X.
            Some(parts) => {
                p.spawn((
                    Transform::from_xyz(0.0, -0.02, 0.06).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                    Visibility::default(),
                    GunModel,
                ))
                .with_children(|g| {
                    for (mesh, mat) in parts {
                        g.spawn((Mesh3d(mesh), MeshMaterial3d(mat), NotShadowCaster));
                    }
                });
            }
            None if !rig => {
                p.spawn((Transform::from_scale(Vec3::splat(0.8)), Visibility::default(), GunModel)).with_children(|g| build_gun(g, &def, mats_ref));
            }
            None => {}
        });
    }

    if vm.rig {
        // Animations place the arms; only add a little walk bob and recoil.
        let tt = time.elapsed_secs();
        let sway = if ctl.moving { 1.0 - 0.8 * ctl.ads } else { 0.25 };
        t.translation = Vec3::new((ctl.bob * 0.5).sin() * 0.008, (ctl.bob).sin().abs() * -0.008 + (tt * 1.6).sin() * 0.002, gun.kick * 0.02) * sway.max(0.3);
        t.rotation = Quat::from_rotation_x(gun.kick * 0.03);
        if let Ok((_, mut kv)) = knife.single_mut() {
            *kv = Visibility::Hidden;
        }
        if let Ok((_, mut ft, mut fl)) = flash.single_mut() {
            let on = gun.flash > 0.0;
            fl.intensity = if on { 120_000.0 } else { 0.0 };
            ft.translation = Vec3::ZERO;
            ft.scale = if on { Vec3::splat(0.07 + fastrand::f32() * 0.05) } else { Vec3::ZERO };
        }
        return;
    }
    let hip = Vec3::new(0.21, -0.19, -0.44);
    let ads = Vec3::new(0.0, -0.13, -0.36);
    let mut pos = hip.lerp(ads, ctl.ads);
    let mut rot = Quat::IDENTITY;
    let tt = time.elapsed_secs();
    let sway = if ctl.moving { 1.0 - 0.8 * ctl.ads } else { 0.25 };
    pos += Vec3::new((ctl.bob * 0.5).sin() * 0.012, (ctl.bob).sin().abs() * -0.012, 0.0) * sway;
    pos.y += (tt * 1.6).sin() * 0.003;
    pos.z += gun.kick * 0.05;
    rot *= Quat::from_rotation_x(gun.kick * 0.08);
    if ctl.sprinting {
        pos += Vec3::new(-0.05, -0.06, 0.05);
        rot *= Quat::from_euler(EulerRot::XYZ, -0.3, 0.5, 0.2);
    }
    if let Some(left) = gun.reload {
        let def = &defs.0[def_idx];
        let k = (std::f32::consts::PI * (1.0 - left / def.reload_time.max(0.01))).sin();
        pos.y -= 0.12 * k;
        rot *= Quat::from_euler(EulerRot::XYZ, -0.6 * k, 0.0, 0.5 * k);
    }
    if gun.switch > 0.0 {
        pos.y -= gun.switch * 0.6;
    }
    if gun.knife_anim > 0.0 {
        pos += Vec3::new(0.1, -0.15, 0.1);
    }
    t.translation = pos;
    t.rotation = rot;

    if let Ok((mut kt, mut kv)) = knife.single_mut() {
        if gun.knife_anim > 0.0 {
            *kv = Visibility::Visible;
            let k = 1.0 - gun.knife_anim / 0.35;
            kt.translation = Vec3::new(-0.25 + k * 0.3, 0.05, -0.25 - (k * std::f32::consts::PI).sin() * 0.25);
            kt.rotation = Quat::from_rotation_y(0.8 - k * 1.6);
        } else {
            *kv = Visibility::Hidden;
        }
    }
    if let Ok((_, mut ft, mut fl)) = flash.single_mut() {
        let on = gun.flash > 0.0;
        fl.intensity = if on { 120_000.0 } else { 0.0 };
        let def = &defs.0[def_idx];
        let tip = match def.kind {
            Kind::Pistol => -0.22,
            Kind::Wonder => -0.38,
            Kind::Smg => -0.55,
            _ => -0.8,
        };
        ft.translation = Vec3::new(0.0, 0.02, tip);
        ft.scale = if on { Vec3::splat(0.07 + fastrand::f32() * 0.05) } else { Vec3::ZERO };
    }
}

/// Plays the weapon's own animations on the first-person arms: raise when
/// switching, fire (and the bolt rechamber), reload, knife, sprint and idle;
/// aiming down the sights follows the `ads_up` animation.
#[allow(clippy::too_many_arguments)]
fn animate_view_rig(
    time: Res<Time>,
    defs: Res<Defs>,
    loadout: Option<Res<Loadout>>,
    gun: Res<Gun>,
    view_rig: Res<crate::nacht::ViewRig>,
    player: Query<&PlayerCtl, With<Player>>,
    rig_q: Query<&ViewRigState>,
    mut tq: Query<&mut Transform>,
    (zs, mut alias, mut last): (Res<ZoneSounds>, EventWriter<PlayAlias>, Local<Option<(String, f32)>>),
) {
    let (Some(loadout), Ok(rig), Ok(ctl)) = (loadout, rig_q.single(), player.single()) else { return };
    let Some(anims) = view_rig.anims.get(rig.weapon) else { return };
    let def = &defs.0[loadout.current().def];
    let get = |slot: &str| anims.get(slot).cloned();
    let knife = view_rig.anims.get("__knife").and_then(|a| a.get("melee").cloned());
    // (clip, normalised progress 0..1)
    let progress = |c: &std::sync::Arc<crate::nacht::build::AnimClip>, secs: f32| if c.duration > 0.0 { (secs / c.duration).clamp(0.0, 1.0) } else { 1.0 };
    let empty = loadout.current().clip == 0;
    let choice = if gun.knife_anim > 0.0 {
        knife.map(|k| (k.clone(), 1.0 - gun.knife_anim / 0.35))
    } else if gun.switch > 0.0 {
        get("raise").map(|c| (c, 1.0 - gun.switch / 0.45))
    } else if let Some(left) = gun.reload {
        let c = if gun.reload_empty { get("reload_empty").or_else(|| get("reload")) } else { get("reload") };
        c.map(|c| (c, 1.0 - left / def.reload_time.max(0.01)))
    } else {
        // The shot that empties the clip has its own animation (the
        // Garand's clip flies out).
        let last = if empty { get("last_shot") } else { None };
        let fire = last.or_else(|| get(if ctl.ads > 0.5 { "ads_fire" } else { "fire" })).or_else(|| get("fire"));
        let rechamber = get("rechamber").filter(|_| def.rechamber);
        match (gun.since_shot, fire) {
            (Some(s), Some(f)) if s < f.duration => Some((f.clone(), progress(&f, s))),
            (Some(s), Some(f)) if rechamber.as_ref().is_some_and(|r| s < f.duration + r.duration) => {
                let r = rechamber.unwrap();
                let p = progress(&r, s - f.duration);
                Some((r, p))
            }
            _ if ctl.sprinting && get("sprint_loop").is_some() => {
                let c = get("sprint_loop").unwrap();
                let p = (time.elapsed_secs() / c.duration.max(0.01)).fract();
                Some((c, p))
            }
            _ => get(if empty { "empty_idle" } else { "idle" }).or_else(|| get("idle")).map(|c| (c, 0.0)),
        }
    };
    if let Some((clip, p)) = choice.as_ref() {
        // Notetrack sounds crossed since the last frame (a new clip starts
        // from its beginning; looping clips wrap).
        let p = p.clamp(0.0, 1.0);
        let from = match last.as_ref() {
            Some((name, q)) if *name == clip.name => Some(*q),
            _ => None,
        };
        for (note, t) in &clip.notify {
            let crossed = match from {
                Some(q) if p >= q => *t > q && *t <= p,
                Some(q) => *t > q || *t <= p,
                None => *t <= p && p > 0.0,
            };
            let crossed = crossed || (from.is_none() && *t == 0.0);
            if crossed {
                if let Some(a) = zs.notetrack(rig.weapon, note) {
                    alias.write(PlayAlias::local(a));
                }
            }
        }
        *last = Some((clip.name.clone(), p));
    }
    if let Some((clip, p)) = choice {
        let map = crate::nacht::track_map(&clip, &rig.joints);
        crate::nacht::pose_mapped(&clip, p.clamp(0.0, 1.0) * clip.numframes, &rig.joints, &map, &mut tq, None);
    }
    // Hip to sights: tag_torso follows the ADS animation by the aim amount.
    if let Some(ads) = get("ads_up") {
        let map = crate::nacht::track_map(&ads, &rig.joints);
        crate::nacht::pose_mapped(&ads, ctl.ads.clamp(0.0, 1.0) * ads.numframes, &rig.joints, &map, &mut tq, Some("tag_torso"));
    }
}
