//! The in-game HUD, laid out like World at War's zombies HUD: chalk tally
//! marks for the round, the score on its blood-red bar with floating point
//! popups, the weapon name, clip and reserve ammo, use hints, the Reload
//! warning, power-up timers, the nuke flash, the low-health overlay and the
//! crosshair. (Game over and pause screens live in `menu.rs`.)
//!
//! Positions use the game's 640x480 virtual screen, scaled by the window
//! height with left/right elements pinned to the edges. Text is drawn with
//! the game's own bitmap fonts (`code_post_gfx.ff` glyph tables over the
//! `gamefonts_pc` atlas); without an install, Bevy's font stands in.
//!
//! Each frame the HUD is described as a list of quads and texts, which are
//! synced onto pools of UI nodes.

use crate::audio::{AssetDir, SoundBank};
use crate::interact::Prompt;
use crate::player::{Health, Player, PlayerCtl};
use crate::powerups::PowerupGrabbed;
use crate::waw::{Waw, WawImages};
use crate::weapons::{Gun, Loadout};
use crate::{ActivePowerups, Defs, GameState, PointsEvent, Round, Score};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;
use waw_assets::t4::Glyph;
use zm_core::rules::{self, Powerup};
use zm_core::weapons::Kind;

/// The zombie scripts' dark red.
const DARK_RED: Color = Color::srgb(0.423, 0.004, 0.0);

/// Parent of every HUD element; hidden outside gameplay.
#[derive(Component)]
struct HudRoot;

/// Pooled image quad `n`.
#[derive(Component)]
struct QuadSlot(usize);

/// Pooled fallback text `n` (no install).
#[derive(Component)]
struct TextSlot(usize);

#[derive(Component)]
struct HelpText;

/// One raster size of the game's UI face.
struct HudFont {
    pixel_height: f32,
    glyphs: HashMap<u16, Glyph>,
}

/// The game's fonts and HUD images, if the install has them.
#[derive(Resource, Default)]
struct HudAssets {
    atlas: Option<(Handle<Image>, Vec2)>,
    /// Sorted by raster height.
    fonts: Vec<HudFont>,
    chalk: Vec<Option<Handle<Image>>>,
    scorebar: Option<Handle<Image>>,
    /// bullet, rifle bullet, shotgun shell.
    ammo: [Option<(Handle<Image>, Vec2)>; 3],
    low_health: Option<Handle<Image>>,
    grenade: Option<Handle<Image>>,
    /// Scope overlays by name (`adsOverlayShader`), loaded when first seen.
    scopes: HashMap<String, Option<Handle<Image>>>,
}

/// Animation state of the script-driven parts.
#[derive(Resource, Default)]
struct HudAnim {
    /// Round shown by the chalk.
    shown_round: u32,
    /// Seconds since round 1 started (the intro), if running.
    intro: Option<f32>,
    /// Seconds into a round-change fade.
    change: Option<f32>,
    /// Seconds since the last round ended.
    round_end: Option<f32>,
    was_intermission: bool,
    popups: Vec<PopupAnim>,
    max_ammo: Option<f32>,
    nuke: Option<f32>,
    /// Seconds into the session (for the help hint).
    session: f32,
}

struct PopupAnim {
    value: i32,
    t: f32,
    dx: f32,
    dy: f32,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudAnim>()
            .init_resource::<HudAssets>()
            .add_systems(Startup, (load_hud_assets, spawn_hud))
            .add_systems(Update, (load_scopes, animate, draw).chain())
            .add_systems(Update, (help_toggle, hud_visibility))
            .add_systems(OnEnter(GameState::Loading), reset_anim)
            .add_systems(OnEnter(GameState::MainMenu), reset_anim);
    }
}

/// Loads the scope overlays of the weapons that have one (their names come
/// from the weapon files, read after start-up).
fn load_scopes(
    waw: Res<Waw>,
    defs: Res<Defs>,
    mut wimg: ResMut<WawImages>,
    mut images: ResMut<Assets<Image>>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
    mut assets: ResMut<HudAssets>,
) {
    for name in defs.0.iter().filter_map(|d| d.ads_overlay.as_deref()) {
        if !assets.scopes.contains_key(name) {
            let h = wimg.get(&waw, &mut images, device.as_deref(), name, true, false);
            assets.scopes.insert(name.to_string(), h);
        }
    }
}

fn reset_anim(mut anim: ResMut<HudAnim>) {
    *anim = HudAnim::default();
}

fn load_hud_assets(
    waw: Res<Waw>,
    mut wimg: ResMut<WawImages>,
    mut images: ResMut<Assets<Image>>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
    mut assets: ResMut<HudAssets>,
) {
    let Some(install) = &waw.install else { return };
    let device = device.as_deref();
    let mut get = |name: &str| wimg.get(&waw, &mut images, device, name, true, false);
    let atlas = get("gamefonts_pc");
    assets.chalk = (1..=5).map(|i| get(&format!("chalkmarks_{i}"))).collect();
    assets.scorebar = get("scorebar_zom_1");
    assets.low_health = get("overlay_low_health");
    assets.grenade = get(crate::grenades::HUD_ICON);
    let ammo_names = ["ammo_counter_bullet", "ammo_counter_riflebullet", "ammo_counter_shotgunshell"];
    let ammo: Vec<Option<Handle<Image>>> = ammo_names.iter().map(|n| get(n)).collect();
    let size = |h: &Handle<Image>| images.get(h).map(|i| i.size().as_vec2());
    for (slot, h) in assets.ammo.iter_mut().zip(ammo) {
        *slot = h.and_then(|h| size(&h).map(|s| (h, s)));
    }
    assets.atlas = atlas.and_then(|h| size(&h).map(|s| (h, s)));
    // The glyph tables live in code_post_gfx.ff.
    let zone = std::fs::read(install.fastfile("code_post_gfx"))
        .ok()
        .and_then(|raw| waw_assets::zone::decompress(&raw).ok())
        .map(waw_assets::t4::walk);
    if let Some(zd) = zone {
        for name in ["fonts/smallFont", "fonts/objectiveFont", "fonts/normalFont", "fonts/bigFont"] {
            if let Some(f) = zd.font(name) {
                let glyphs = zd.glyphs(f).into_iter().map(|g| (g.letter, g)).collect();
                assets.fonts.push(HudFont { pixel_height: f.pixel_height as f32, glyphs });
            }
        }
        assets.fonts.sort_by(|a, b| a.pixel_height.total_cmp(&b.pixel_height));
    }
    info!("HUD: {} game fonts, atlas {}", assets.fonts.len(), assets.atlas.is_some());
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        Visibility::Hidden,
        HudRoot,
    ));
}

fn hud_visibility(state: Res<State<GameState>>, mut q: Query<&mut Visibility, With<HudRoot>>) {
    let show = matches!(state.get(), GameState::Playing | GameState::Paused | GameState::GameOver);
    if let Ok(mut v) = q.single_mut() {
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing

#[derive(Clone, Copy, PartialEq)]
enum H {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq)]
enum V {
    Top,
    Middle,
    Bottom,
}

struct QuadDesc {
    image: Handle<Image>,
    /// Screen rect in logical pixels.
    rect: Rect,
    /// Sub-rect of the image in texels.
    uv: Option<Rect>,
    color: Color,
}

struct TextDesc {
    text: String,
    pos: Vec2,
    size: f32,
    color: Color,
    align: H,
}

/// The frame's HUD in screen pixels.
struct Canvas<'a> {
    w: f32,
    h: f32,
    /// Pixels per virtual unit.
    u: f32,
    assets: &'a HudAssets,
    quads: Vec<QuadDesc>,
    texts: Vec<TextDesc>,
}

impl Canvas<'_> {
    /// A virtual position relative to an edge or the centre, in pixels.
    fn at(&self, h: H, v: V, x: f32, y: f32) -> Vec2 {
        let px = match h {
            H::Left => 0.0,
            H::Center => self.w * 0.5,
            H::Right => self.w,
        } + x * self.u;
        let py = match v {
            V::Top => 0.0,
            V::Middle => self.h * 0.5,
            V::Bottom => self.h,
        } + y * self.u;
        Vec2::new(px, py)
    }

    /// An image whose `align` corner sits at `p` (pixels), `size` in units.
    fn image(&mut self, image: &Handle<Image>, p: Vec2, size: Vec2, align: (H, V), color: Color) {
        let s = size * self.u;
        let x = match align.0 {
            H::Left => p.x,
            H::Center => p.x - s.x * 0.5,
            H::Right => p.x - s.x,
        };
        let y = match align.1 {
            V::Top => p.y,
            V::Middle => p.y - s.y * 0.5,
            V::Bottom => p.y - s.y,
        };
        self.quads.push(QuadDesc { image: image.clone(), rect: Rect::new(x, y, x + s.x, y + s.y), uv: None, color });
    }

    fn solid(&mut self, rect: Rect, color: Color) {
        self.quads.push(QuadDesc { image: Handle::default(), rect, uv: None, color });
    }

    /// The raster closest to (but not below) the drawn size, for sharp text.
    fn font_for(&self, px: f32) -> Option<&HudFont> {
        let f = &self.assets.fonts;
        f.iter().find(|f| f.pixel_height >= px * 0.9).or(f.last())
    }

    fn measure(font: &HudFont, text: &str, scale: f32) -> f32 {
        text.chars().map(|c| font.glyphs.get(&(c as u16)).or(font.glyphs.get(&(b'?' as u16))).map_or(0.0, |g| g.dx as f32)).sum::<f32>() * scale
    }

    /// Text `height` units tall whose line box is aligned at `p`: the
    /// origin is the bottom of the box, like the game's.
    fn text(&mut self, text: &str, p: Vec2, height: f32, align: (H, V), color: Color, shadow: bool) {
        let px = height * self.u;
        let origin_y = match align.1 {
            V::Top => p.y + px,
            V::Middle => p.y + px * 0.5,
            V::Bottom => p.y,
        };
        let (Some(font), Some((atlas, atlas_size))) = (self.font_for(px), self.assets.atlas.clone()) else {
            // Bevy's font stands in (no install).
            let x = p.x;
            self.texts.push(TextDesc { text: text.to_string(), pos: Vec2::new(x, origin_y - px), size: px * 0.9, color, align: align.0 });
            return;
        };
        let scale = px / font.pixel_height;
        let width = Self::measure(font, text, scale);
        let mut pen = match align.0 {
            H::Left => p.x,
            H::Center => p.x - width * 0.5,
            H::Right => p.x - width,
        };
        let mut glyph_quads = Vec::new();
        for c in text.chars() {
            let Some(g) = font.glyphs.get(&(c as u16)).or(font.glyphs.get(&(b'?' as u16))) else { continue };
            if g.width > 0 && g.height > 0 {
                let x = pen + g.x0 as f32 * scale;
                let y = origin_y + g.y0 as f32 * scale;
                let rect = Rect::new(x, y, x + g.width as f32 * scale, y + g.height as f32 * scale);
                let uv = Rect::new(g.uv[0] * atlas_size.x, g.uv[1] * atlas_size.y, g.uv[2] * atlas_size.x, g.uv[3] * atlas_size.y);
                glyph_quads.push((rect, uv));
            }
            pen += g.dx as f32 * scale;
        }
        if shadow {
            let off = Vec2::splat((self.u * 0.75).max(1.0));
            let a = color.alpha();
            for (r, uv) in &glyph_quads {
                let rect = Rect::from_corners(r.min + off, r.max + off);
                self.quads.push(QuadDesc { image: atlas.clone(), rect, uv: Some(*uv), color: Color::srgba(0.0, 0.0, 0.0, a * 0.9) });
            }
        }
        for (rect, uv) in glyph_quads {
            self.quads.push(QuadDesc { image: atlas.clone(), rect, uv: Some(uv), color });
        }
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let (a, b) = (a.to_srgba(), b.to_srgba());
    let t = t.clamp(0.0, 1.0);
    Color::srgba(a.red + (b.red - a.red) * t, a.green + (b.green - a.green) * t, a.blue + (b.blue - a.blue) * t, a.alpha + (b.alpha - a.alpha) * t)
}

fn with_alpha(c: Color, a: f32) -> Color {
    let mut c = c;
    c.set_alpha(a.clamp(0.0, 1.0));
    c
}

/// Advances the script-driven animations from game events.
fn animate(
    time: Res<Time>,
    state: Res<State<GameState>>,
    round: Res<Round>,
    mut anim: ResMut<HudAnim>,
    mut points: EventReader<PointsEvent>,
    mut grabbed: EventReader<PowerupGrabbed>,
) {
    if *state.get() != GameState::Playing {
        points.clear();
        grabbed.clear();
        return;
    }
    let dt = time.delta_secs();
    anim.session += dt;
    let r = round.0.round;
    if r != anim.shown_round && r > 0 {
        if r == 1 {
            anim.intro = Some(0.0);
            anim.shown_round = 1;
        } else if anim.change.is_none() {
            anim.change = Some(0.0);
        }
    }
    if let Some(t) = anim.change.as_mut() {
        *t += dt;
        // The new tally takes over halfway (faded out).
        if *t >= 0.5 {
            anim.shown_round = r;
        }
        if anim.change.is_some_and(|t| t >= 1.0) {
            anim.change = None;
        }
    }
    if let Some(t) = anim.intro.as_mut() {
        *t += dt;
        if *t > 7.0 {
            anim.intro = None;
        }
    }
    let intermission = round.0.in_intermission() && r > 0;
    if intermission && !anim.was_intermission {
        anim.round_end = Some(0.0);
    }
    anim.was_intermission = intermission;
    if let Some(t) = anim.round_end.as_mut() {
        *t += dt;
        if *t > 15.0 {
            anim.round_end = None;
        }
    }
    for ev in points.read() {
        anim.popups.push(PopupAnim { value: ev.0, t: 0.0, dx: -(20.0 + fastrand::f32() * 39.0), dy: -(-15.0 + fastrand::f32() * 30.0) });
    }
    for p in anim.popups.iter_mut() {
        p.t += dt;
    }
    anim.popups.retain(|p| p.t < 0.5);
    for ev in grabbed.read() {
        match ev.0 {
            Powerup::MaxAmmo => anim.max_ammo = Some(0.0),
            Powerup::Nuke => anim.nuke = Some(0.0),
            _ => {}
        }
    }
    let anim = &mut *anim;
    for t in [&mut anim.max_ammo, &mut anim.nuke] {
        if let Some(v) = t.as_mut() {
            *v += dt;
            if *v > 2.5 {
                *t = None;
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    window: Query<&Window, With<PrimaryWindow>>,
    assets: Res<HudAssets>,
    anim: Res<HudAnim>,
    (score, defs, pu, prompt, health, gun): (Res<Score>, Res<Defs>, Res<ActivePowerups>, Res<Prompt>, Res<Health>, Res<Gun>),
    (loadout, grenades): (Option<Res<Loadout>>, Option<Res<crate::grenades::Grenades>>),
    names: Option<Res<crate::nacht::WeaponNames>>,
    time: Res<Time>,
    player: Query<(&PlayerCtl, &Projection), With<Player>>,
    root: Query<Entity, With<HudRoot>>,
    mut quads: Query<(&QuadSlot, &mut Node, &mut ImageNode, &mut Visibility), Without<TextSlot>>,
    mut texts: Query<(&TextSlot, &mut Node, &mut Text, &mut TextFont, &mut TextColor, &mut Visibility), Without<QuadSlot>>,
    mut commands: Commands,
) {
    let (Ok(win), Ok(root)) = (window.single(), root.single()) else { return };
    let (w, h) = (win.width(), win.height());
    let mut c = Canvas { w, h, u: h / 480.0, assets: &assets, quads: Vec::new(), texts: Vec::new() };
    let t = time.elapsed_secs();

    // Scope overlay when fully aimed with a scoped weapon (the gun is
    // hidden): centred at its size in the 640 x 480 screen, black around it.
    if let (Some(l), Ok((ctl, _))) = (&loadout, player.single()) {
        let def = &defs.0[l.current().def];
        if crate::weapons::scope_shown(def, ctl) {
            let size = Vec2::from(def.ads_overlay_size) * c.u;
            let r = Rect::from_center_size(Vec2::new(w, h) * 0.5, size);
            let black = Color::BLACK;
            c.solid(Rect::new(0.0, 0.0, r.min.x.max(0.0), h), black);
            c.solid(Rect::new(r.max.x.min(w), 0.0, w, h), black);
            c.solid(Rect::new(r.min.x, 0.0, r.max.x, r.min.y.max(0.0)), black);
            c.solid(Rect::new(r.min.x, r.max.y.min(h), r.max.x, h), black);
            if let Some(img) = def.ads_overlay.as_deref().and_then(|n| assets.scopes.get(n)).cloned().flatten() {
                c.quads.push(QuadDesc { image: img, rect: r, uv: None, color: Color::WHITE });
            }
        }
    }

    // Low-health overlay (under everything): pulses harder the lower the
    // health, flashes on a hit.
    if let Some(img) = &assets.low_health {
        let hurt = 1.0 - (health.hp / rules::PLAYER_MAX_HEALTH).clamp(0.0, 1.0);
        let pulse = 0.8 + 0.2 * (t * std::f32::consts::TAU / 0.8).sin();
        let a = (hurt * 1.4 * pulse + health.flash * 0.6).min(1.0);
        if a > 0.01 {
            c.quads.push(QuadDesc { image: img.clone(), rect: Rect::new(0.0, 0.0, w, h), uv: None, color: with_alpha(Color::WHITE, a) });
        }
    } else {
        let a = ((1.0 - health.hp / rules::PLAYER_MAX_HEALTH) * 0.45 + health.flash * 0.25).min(0.7);
        c.solid(Rect::new(0.0, 0.0, w, h), Color::srgba(0.55, 0.0, 0.0, a));
    }

    draw_round(&mut c, &anim);
    draw_score(&mut c, &anim, score.points);
    if let Some(l) = &loadout {
        draw_weapon(&mut c, l, &defs, names.as_deref(), &gun, t);
    }
    if let Some(g) = &grenades {
        draw_grenades(&mut c, g);
    }

    // Use hint: centred under the crosshair.
    if !prompt.0.is_empty() {
        let p = c.at(H::Center, V::Middle, 0.0, 85.0);
        c.text(&prompt.0, p, 14.86, (H::Center, V::Bottom), Color::WHITE, true);
    }

    // Power-up timers: "Double Points: 27", "Insta-Kill: 12", "Max Ammo!".
    let timer = |c: &mut Canvas, label: &str, secs: f32, y: f32| {
        if secs > 0.0 {
            let p = c.at(H::Center, V::Top, 0.0, y);
            c.text(&format!("{label}: {}", secs.ceil() as i32), p, 24.0, (H::Center, V::Top), Color::WHITE, true);
        }
    };
    timer(&mut c, "Double Points", pu.double_points, 350.0);
    timer(&mut c, "Insta-Kill", pu.insta_kill, 380.0);
    if let Some(m) = anim.max_ammo {
        let k = ((m - 0.5) / 1.5).clamp(0.0, 1.0);
        let a = (m / 0.5).min(1.0) * (1.0 - k);
        let p = c.at(H::Center, V::Top, 0.0, 290.0 - 20.0 * k);
        c.text("Max Ammo!", p, 24.0, (H::Center, V::Top), with_alpha(Color::WHITE, a), true);
    }

    draw_crosshair(&mut c, &player, &loadout, &defs, &gun);

    // Nuke: a white flash over everything.
    if let Some(n) = anim.nuke {
        let a = if n < 0.2 { n / 0.2 * 0.8 } else if n < 0.5 { 0.8 } else { (0.8 * (1.0 - (n - 0.5) / 1.0)).max(0.0) };
        c.solid(Rect::new(0.0, 0.0, w, h), with_alpha(Color::WHITE, a));
    }

    // The help hint for the first seconds of a session.
    if anim.session < 10.0 {
        let p = c.at(H::Left, V::Top, 8.0, 8.0);
        let a = (10.0 - anim.session).min(1.0) * 0.6;
        c.text("H - help", p, 10.0, (H::Left, V::Top), with_alpha(Color::WHITE, a), true);
    }

    sync(&mut commands, root, c.quads, c.texts, &mut quads, &mut texts);
}

/// The chalk tally (rounds 1-10), the round number from 11, and the intro.
fn draw_round(c: &mut Canvas, anim: &HudAnim) {
    let r = anim.shown_round;
    if r == 0 {
        return;
    }
    // Colour: red, whitening and blinking after a round ends.
    let mut color = DARK_RED;
    let mut alpha = 1.0;
    if let Some(e) = anim.round_end {
        let white = if e < 2.5 { e / 2.5 } else if e < 12.5 { 1.0 } else { 1.0 - (e - 12.5) / 2.5 };
        color = mix(DARK_RED, Color::WHITE, white);
        if (2.5..12.5).contains(&e) {
            let k = (e - 2.5).fract();
            alpha = if k < 0.5 { 1.0 - k * 2.0 } else { (k - 0.5) * 2.0 };
        }
    }
    if let Some(ch) = anim.change {
        alpha *= if ch < 0.5 { 1.0 - ch * 2.0 } else { (ch - 0.5) * 2.0 };
    }
    // Where the chalk sits: bottom-left, or sliding there during the intro.
    let home = c.at(H::Left, V::Bottom, -3.0, -4.0);
    let mut pos = home;
    if let Some(i) = anim.intro {
        let start = c.at(H::Center, V::Bottom, -5.0, -200.0);
        let k = ((i - 4.75) / 1.75).clamp(0.0, 1.0);
        pos = start.lerp(home, k);
        alpha *= ((i - 1.5) / 0.5).clamp(0.0, 1.0);
        // "Round" above it: fades in white, turns red, fades out.
        let ra = (i / 1.0).min(1.0) * (1.0 - ((i - 4.5) / 1.0).clamp(0.0, 1.0));
        let rc = mix(Color::WHITE, DARK_RED, ((i - 1.0) / 3.0).clamp(0.0, 1.0));
        let p = c.at(H::Center, V::Bottom, 0.0, -265.0);
        c.text("Round", p, 32.0, (H::Center, V::Bottom), with_alpha(rc, ra), false);
    }
    let col = with_alpha(color, alpha);
    let chalk = |c: &mut Canvas, n: u32, p: Vec2| match c.assets.chalk.get(n as usize - 1).cloned().flatten() {
        Some(img) => c.image(&img, p, Vec2::splat(64.0), (H::Left, V::Bottom), col),
        None => c.text(&n.to_string(), p, 64.0, (H::Left, V::Bottom), col, false),
    };
    match r {
        1..=5 => chalk(c, r, pos),
        6..=10 => {
            chalk(c, 5, pos);
            chalk(c, r - 5, pos + Vec2::new(64.0 * c.u, 0.0));
        }
        _ => c.text(&r.to_string(), pos, 64.0, (H::Left, V::Bottom), col, false),
    }
}

/// The score on its red brush stroke, and the floating point popups.
fn draw_score(c: &mut Canvas, anim: &HudAnim, points: u32) {
    // Measured from the PC game at 1080p: the bar spans x -102..-11.
    let anchor = c.at(H::Right, V::Bottom, -103.0, -71.0);
    if let Some(bar) = c.assets.scorebar.clone() {
        let p = c.at(H::Right, V::Bottom, -102.0, -71.0);
        c.image(&bar, p, Vec2::new(91.0, 20.0), (H::Left, V::Middle), Color::srgba(0.424, 0.004, 0.0, 0.8));
    }
    let p = c.at(H::Right, V::Bottom, -97.0, -71.0);
    c.text(&points.to_string(), p, 12.5, (H::Left, V::Middle), Color::WHITE, false);
    for pop in &anim.popups {
        let k = (pop.t / 0.5).min(1.0);
        let p = anchor + Vec2::new(pop.dx, pop.dy) * k * c.u;
        let a = if pop.t < 0.25 { 1.0 } else { 1.0 - (pop.t - 0.25) / 0.25 };
        let (text, color) = if pop.value >= 1 { (format!("+{}", pop.value), Color::srgb(0.9, 0.9, 0.0)) } else { (pop.value.to_string(), DARK_RED) };
        c.text(&text, p, 16.0, (H::Right, V::Middle), with_alpha(color, a), false);
    }
}

/// Weapon name, clip icons, reserve ammo and the Reload warning.
fn draw_weapon(c: &mut Canvas, l: &Loadout, defs: &Defs, names: Option<&crate::nacht::WeaponNames>, gun: &Gun, t: f32) {
    let slot = l.current();
    let def = &defs.0[slot.def];
    let name = names.and_then(|n| n.0.get(def.id)).cloned().unwrap_or_else(|| def.name.clone());
    let white = Color::srgba(1.0, 1.0, 1.0, 0.75);
    let p = c.at(H::Right, V::Bottom, -43.0, -38.0);
    c.text(&name, p, 14.86, (H::Right, V::Bottom), white, true);
    // Reserve, red when it won't fill the clip much more.
    let low_stock = (slot.reserve as f32) < def.clip as f32 * 1.5;
    let stock_col = if low_stock { Color::srgba(0.85, 0.12, 0.1, 0.9) } else { white };
    let p = c.at(H::Right, V::Bottom, -67.0, 2.0);
    c.text(&slot.reserve.to_string(), p, 14.86, (H::Left, V::Bottom), stock_col, true);

    // One icon per round in the clip: bullets in a row growing left,
    // rifle rounds stacked.
    let kind = match def.kind {
        Kind::Rifle => 1,
        Kind::Shotgun => 2,
        _ => 0,
    };
    if let Some((img, texels)) = c.assets.ammo[kind].clone() {
        // Icons at half their texel size in a row growing left (rifle
        // rounds lie as dashes), as the PC game draws them.
        let size = texels * 0.5;
        let base = c.at(H::Right, V::Bottom, -78.0, -4.0);
        let col = Color::srgba(1.0, 1.0, 1.0, 0.65);
        for i in 0..slot.clip.min(60) {
            let p = base + Vec2::new(-(size.x + 1.0), 0.0) * i as f32 * c.u;
            c.image(&img, p, size, (H::Right, V::Bottom), col);
        }
    } else {
        let p = c.at(H::Right, V::Bottom, -79.0, 4.0);
        c.text(&slot.clip.to_string(), p, 14.86, (H::Right, V::Bottom), white, true);
    }

    // Reload / LOW AMMO, pulsing, when the clip is a third full or less.
    let low = def.clip > 0 && (slot.clip as f32) <= def.clip as f32 * 0.33 && gun.reload.is_none();
    if low && !(slot.clip == 0 && slot.reserve == 0 && def.clip == 0) {
        let k = 0.5 + 0.5 * (t * 1.7 * std::f32::consts::TAU).sin();
        let (text, a, b) = if slot.reserve > 0 {
            ("Reload", Color::srgba(0.7, 0.7, 0.7, 0.8), Color::WHITE)
        } else if slot.clip > 0 {
            ("LOW AMMO", Color::srgba(0.7, 0.7, 0.3, 0.8), Color::srgb(1.0, 1.0, 0.5))
        } else {
            ("NO AMMO", Color::srgba(0.8, 0.25, 0.25, 0.8), Color::srgb(1.0, 0.25, 0.25))
        };
        let p = c.at(H::Center, V::Middle, 0.0, 30.0);
        c.text(text, p, 14.86, (H::Center, V::Middle), mix(a, b, k), true);
    }
}

/// The grenade icon and count (`offhandFragIcon` / `offhandfragammo`), and
/// the flash of a nearby explosion.
fn draw_grenades(c: &mut Canvas, g: &crate::grenades::Grenades) {
    if g.flash > 0.0 {
        c.solid(Rect::new(0.0, 0.0, c.w, c.h), Color::srgba(1.0, 0.92, 0.8, g.flash.min(1.0)));
    }
    if g.count == 0 {
        return;
    }
    let p = c.at(H::Right, V::Bottom, -104.0, -38.0);
    match c.assets.grenade.clone() {
        Some(img) => c.image(&img, p, Vec2::splat(24.0), (H::Left, V::Top), Color::srgba(1.0, 1.0, 1.0, 0.65)),
        None => {
            // Stand-in: a stick grenade's head and handle.
            let u = c.u;
            c.solid(Rect::new(p.x + 7.0 * u, p.y + 3.0 * u, p.x + 17.0 * u, p.y + 11.0 * u), Color::srgba(0.8, 0.8, 0.8, 0.65));
            c.solid(Rect::new(p.x + 10.5 * u, p.y + 11.0 * u, p.x + 13.5 * u, p.y + 22.0 * u), Color::srgba(0.8, 0.8, 0.8, 0.65));
        }
    }
    let p = c.at(H::Right, V::Bottom, -84.0, -8.0);
    c.text(&g.count.to_string(), p, 14.86, (H::Left, V::Bottom), Color::srgba(1.0, 1.0, 1.0, 0.75), true);
}

/// Four ticks around the centre, spread with the weapon's accuracy; gone
/// when aiming, faded while firing.
fn draw_crosshair(c: &mut Canvas, player: &Query<(&PlayerCtl, &Projection), With<Player>>, loadout: &Option<Res<Loadout>>, defs: &Defs, gun: &Gun) {
    let Ok((ctl, proj)) = player.single() else { return };
    if ctl.ads > 0.5 || ctl.sprinting {
        return;
    }
    let fov = match proj {
        Projection::Perspective(p) => p.fov,
        _ => 1.2,
    };
    // The weapon's current cone (the engine's aim-spread model).
    let spread = if gun.spread > 0.0 { gun.spread } else { loadout.as_ref().map_or(2.0, |l| defs.0[l.current().def].spread.stand.0) };
    let off = spread.to_radians().tan() / (fov * 0.5).tan() * c.h * 0.5 + 4.0 * c.u;
    let firing = gun.since_shot.is_some_and(|s| s < 0.25);
    let a = (if firing { 0.35 } else { 0.8 }) * (1.0 - ctl.ads * 2.0);
    let col = with_alpha(Color::WHITE, a);
    let center = Vec2::new(c.w * 0.5, c.h * 0.5);
    let (len, wid) = (8.0 * c.u * 0.6, (1.0 * c.u).max(1.0));
    for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
        let p = center + Vec2::new(dx, dy) * (off + len * 0.5);
        let half = if dx == 0.0 { Vec2::new(wid, len) * 0.5 } else { Vec2::new(len, wid) * 0.5 };
        c.solid(Rect::from_center_half_size(p, half), col);
    }
}

/// Puts the frame's quads and texts onto pooled UI nodes.
fn sync(
    commands: &mut Commands,
    root: Entity,
    quad_list: Vec<QuadDesc>,
    text_list: Vec<TextDesc>,
    quads: &mut Query<(&QuadSlot, &mut Node, &mut ImageNode, &mut Visibility), Without<TextSlot>>,
    texts: &mut Query<(&TextSlot, &mut Node, &mut Text, &mut TextFont, &mut TextColor, &mut Visibility), Without<QuadSlot>>,
) {
    let mut have = 0;
    for (slot, mut node, mut img, mut vis) in quads.iter_mut() {
        have = have.max(slot.0 + 1);
        match quad_list.get(slot.0) {
            Some(q) => {
                node.left = Val::Px(q.rect.min.x);
                node.top = Val::Px(q.rect.min.y);
                node.width = Val::Px(q.rect.width());
                node.height = Val::Px(q.rect.height());
                if img.image != q.image {
                    img.image = q.image.clone();
                }
                img.rect = q.uv;
                img.color = q.color;
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
    for i in have..quad_list.len() {
        let q = &quad_list[i];
        let mut img = ImageNode::new(q.image.clone()).with_color(q.color);
        img.rect = q.uv;
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(q.rect.min.x),
                top: Val::Px(q.rect.min.y),
                width: Val::Px(q.rect.width()),
                height: Val::Px(q.rect.height()),
                ..default()
            },
            img,
            ZIndex(i as i32),
            QuadSlot(i),
            ChildOf(root),
        ));
    }
    let mut have = 0;
    for (slot, mut node, mut text, mut font, mut color, mut vis) in texts.iter_mut() {
        have = have.max(slot.0 + 1);
        match text_list.get(slot.0) {
            Some(d) => {
                text.0.clone_from(&d.text);
                font.font_size = d.size;
                color.0 = d.color;
                node.top = Val::Px(d.pos.y);
                let x = match d.align {
                    H::Left => d.pos.x,
                    H::Center => d.pos.x - d.size * 0.27 * d.text.len() as f32,
                    H::Right => d.pos.x - d.size * 0.55 * d.text.len() as f32,
                };
                node.left = Val::Px(x);
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
    for i in have..text_list.len() {
        let d = &text_list[i];
        commands.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(d.pos.x), top: Val::Px(d.pos.y), ..default() },
            Text::new(d.text.clone()),
            TextFont { font_size: d.size, ..default() },
            TextColor(d.color),
            ZIndex(10_000 + i as i32),
            TextSlot(i),
            ChildOf(root),
        ));
    }
}

fn help_toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    assets: Res<AssetDir>,
    waw: Res<Waw>,
    bank: Option<Res<SoundBank>>,
    root: Query<Entity, With<HudRoot>>,
    mut q: Query<(&mut Text, &mut Visibility), With<HelpText>>,
    mut shown: Local<bool>,
) {
    if keys.just_pressed(KeyCode::KeyH) {
        *shown = !*shown;
    }
    let Ok((mut t, mut vis)) = q.single_mut() else {
        if let Ok(root) = root.single() {
            commands.spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(16.0), top: Val::Px(12.0), ..default() },
                Text::new(""),
                TextFont { font_size: 16.0, ..default() },
                TextColor(Color::srgba(0.85, 0.85, 0.8, 0.85)),
                ZIndex(20_000),
                Visibility::Hidden,
                HelpText,
                ChildOf(root),
            ));
        }
        return;
    };
    vis.set_if_neq(if *shown { Visibility::Inherited } else { Visibility::Hidden });
    if !*shown {
        return;
    }
    let source = match (&waw.install, &assets.root) {
        (Some(i), _) => i.root.display().to_string(),
        (None, Some(p)) => p.display().to_string(),
        (None, None) => "none - using built-in sounds".into(),
    };
    let assets_line = format!("Assets: {source}  ({} sounds, {} weapon files)", bank.map(|b| b.loaded_from_disk).unwrap_or(0), assets.weapon_overrides);
    t.0 = format!(
        "UNDEAD ROUNDS\n\
         Esc pause menu\n\
         WASD move, Shift sprint, Space jump, C crouch, Ctrl/Z prone\n\
         LMB fire, RMB aim, R reload, V/E knife\n\
         G/mouse 4 grenade (hold to cook)\n\
         1/2/Q/wheel switch weapon\n\
         F interact (hold at windows to rebuild)\n\
         -/= mouse sensitivity, [/] brightness\n\
         H toggle this help\n\
         {assets_line}"
    );
}
