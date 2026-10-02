//! On-screen HUD: round counter, points (with floating +/- popups), ammo,
//! interaction prompts, power-up timers, crosshair/hitmarker, damage overlay,
//! banners and help. (Game over and pause screens live in `menu.rs`.)

use crate::audio::{AssetDir, SoundBank};
use crate::waw::Waw;
use crate::interact::Prompt;
use crate::player::{Health, Player, PlayerCtl};
use crate::weapons::{Gun, Loadout};
use crate::{ActivePowerups, Banner, Defs, GameState, PointsEvent, Round, Score};
use bevy::prelude::*;
use zm_core::rules;

#[derive(Component)]
struct RoundText;
#[derive(Component)]
struct PointsText;
#[derive(Component)]
struct PopupLayer;
#[derive(Component)]
struct Popup {
    ttl: f32,
    y: f32,
    positive: bool,
}
#[derive(Component)]
struct AmmoText;
#[derive(Component)]
struct WeaponText;
#[derive(Component)]
struct PromptText;
#[derive(Component)]
struct PowerupText;
#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct Crosshair;
#[derive(Component)]
struct HitMarker;
#[derive(Component)]
struct DamageOverlay;
/// Power-up icon from the install (0 = insta-kill, 1 = double points).
#[derive(Component)]
struct PowerupIcon(u8);

/// Parent of every HUD element; hidden outside gameplay.
#[derive(Component)]
struct HudRoot;
#[derive(Component)]
struct HelpText;

#[derive(Resource, Default)]
struct BannerState {
    ttl: f32,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BannerState>()
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (update_texts, popups, banner, crosshair, damage_overlay, help_toggle, hud_visibility, powerup_icons),
            );
    }
}

fn text(s: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font_size: size, ..default() }, TextColor(color))
}

fn abs(left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: left.map(Val::Px).unwrap_or(Val::Auto),
        right: right.map(Val::Px).unwrap_or(Val::Auto),
        top: top.map(Val::Px).unwrap_or(Val::Auto),
        bottom: bottom.map(Val::Px).unwrap_or(Val::Auto),
        ..default()
    }
}

fn spawn_hud(
    mut commands: Commands,
    waw: Res<Waw>,
    mut wimg: ResMut<crate::waw::WawImages>,
    mut images: ResMut<Assets<Image>>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
) {
    let root = commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            Visibility::Hidden,
            HudRoot,
        ))
        .id();
    // The game's power-up icons, bottom centre, shown while active.
    let icons: Vec<(u8, Handle<Image>)> = [(0u8, "specialty_instakill_zombies"), (1, "specialty_2x_zombies")]
        .into_iter()
        .filter_map(|(i, name)| wimg.get(&waw, &mut images, device.as_deref(), name, true, false).map(|h| (i, h)))
        .collect();
    if !icons.is_empty() {
        commands
            .spawn((
                ChildOf(root),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    bottom: Val::Percent(19.0),
                    justify_content: JustifyContent::Center,
                    column_gap: Val::Px(14.0),
                    ..default()
                },
            ))
            .with_children(|row| {
                for (i, h) in icons {
                    row.spawn((ImageNode::new(h), Node { width: Val::Px(56.0), height: Val::Px(56.0), ..default() }, Visibility::Hidden, PowerupIcon(i)));
                }
            });
    }
    let blood = Color::srgb(0.7, 0.05, 0.03);
    let pale = Color::srgb(0.92, 0.9, 0.82);

    // Damage overlay (below everything else).
    commands.spawn((
        ChildOf(root),
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgba(0.6, 0.0, 0.0, 0.0)),
        DamageOverlay,
    ));

    commands.spawn((
        ChildOf(root),abs(Some(40.0), None, None, Some(30.0)), text("", 96.0, blood), RoundText));
    commands.spawn((
        ChildOf(root),abs(None, Some(40.0), None, Some(120.0)), text("500", 40.0, Color::srgb(1.0, 0.85, 0.3)), PointsText));
    commands.spawn((
        ChildOf(root),
        Node { position_type: PositionType::Absolute, right: Val::Px(40.0), bottom: Val::Px(165.0), width: Val::Px(160.0), height: Val::Px(200.0), ..default() },
        PopupLayer,
    ));
    commands.spawn((
        ChildOf(root),abs(None, Some(40.0), None, Some(70.0)), text("", 34.0, pale), AmmoText));
    commands.spawn((
        ChildOf(root),abs(None, Some(40.0), None, Some(40.0)), text("", 22.0, Color::srgb(0.75, 0.75, 0.7)), WeaponText));

    // Centred column for prompt / power-ups / banner.
    let column = |commands: &mut Commands, top: Val, size: f32, color: Color, marker: (bool, bool, bool)| {
        let mut e = commands.spawn((ChildOf(root), Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top,
            justify_content: JustifyContent::Center,
            ..default()
        }));
        e.with_children(|p| {
            let mut c = p.spawn((text("", size, color), TextLayout::new_with_justify(JustifyText::Center)));
            if marker.0 {
                c.insert(PromptText);
            }
            if marker.1 {
                c.insert(PowerupText);
            }
            if marker.2 {
                c.insert(BannerText);
            }
        });
    };
    column(&mut commands, Val::Percent(68.0), 26.0, pale, (true, false, false));
    column(&mut commands, Val::Percent(85.0), 24.0, Color::srgb(0.5, 1.0, 0.5), (false, true, false));
    column(&mut commands, Val::Percent(18.0), 64.0, blood, (false, false, true));

    // Crosshair: four ticks around the centre.
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Crosshair,
        ))
        .with_children(|p| {
            p.spawn(Node { width: Val::Px(30.0), height: Val::Px(30.0), ..default() }).with_children(|c| {
                let tick = |c: &mut ChildSpawnerCommands, l: f32, t: f32, w: f32, h: f32| {
                    c.spawn((
                        Node { position_type: PositionType::Absolute, left: Val::Px(l), top: Val::Px(t), width: Val::Px(w), height: Val::Px(h), ..default() },
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
                    ));
                };
                tick(c, 14.0, 0.0, 2.0, 8.0);
                tick(c, 14.0, 22.0, 2.0, 8.0);
                tick(c, 0.0, 14.0, 8.0, 2.0);
                tick(c, 22.0, 14.0, 8.0, 2.0);
            });
        });
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|p| {
            p.spawn((text("X", 30.0, Color::srgba(1.0, 1.0, 1.0, 0.0)), HitMarker));
        });

    commands.spawn((
        ChildOf(root),
        abs(Some(16.0), None, Some(12.0), None),
        text("", 16.0, Color::srgba(0.85, 0.85, 0.8, 0.85)),
        HelpText,
    ));

}

#[allow(clippy::type_complexity)]
fn update_texts(
    (round, score, defs, pu, prompt): (Res<Round>, Res<Score>, Res<Defs>, Res<ActivePowerups>, Res<Prompt>),
    loadout: Option<Res<Loadout>>,
    gun: Res<Gun>,
    mut q: ParamSet<(
        Query<&mut Text, With<RoundText>>,
        Query<&mut Text, With<PointsText>>,
        Query<&mut Text, With<AmmoText>>,
        Query<&mut Text, With<WeaponText>>,
        Query<&mut Text, With<PromptText>>,
        Query<&mut Text, With<PowerupText>>,
    )>,
) {
    if let Ok(mut t) = q.p0().single_mut() {
        t.0 = if round.0.round == 0 { String::new() } else { round.0.round.to_string() };
    }
    if let Ok(mut t) = q.p1().single_mut() {
        t.0 = score.points.to_string();
    }
    if let Some(l) = loadout {
        let slot = l.current();
        let def = &defs.0[slot.def];
        if let Ok(mut t) = q.p2().single_mut() {
            t.0 = if gun.reload.is_some() { "reloading...".into() } else { format!("{}  /  {}", slot.clip, slot.reserve) };
        }
        if let Ok(mut t) = q.p3().single_mut() {
            let other = l.slots.iter().enumerate().filter(|(i, _)| *i != l.cur).map(|(_, s)| defs.0[s.def].name.clone()).collect::<Vec<_>>();
            t.0 = if other.is_empty() { def.name.clone() } else { format!("{}   |   {}", def.name, other.join(", ")) };
        }
    }
    if let Ok(mut t) = q.p4().single_mut() {
        t.0 = prompt.0.clone();
    }
    if let Ok(mut t) = q.p5().single_mut() {
        let mut parts = Vec::new();
        if pu.insta_kill > 0.0 {
            parts.push(format!("INSTA-KILL {:.0}", pu.insta_kill.ceil()));
        }
        if pu.double_points > 0.0 {
            parts.push(format!("DOUBLE POINTS {:.0}", pu.double_points.ceil()));
        }
        t.0 = parts.join("      ");
    }
}

fn popups(
    mut commands: Commands,
    time: Res<Time>,
    mut events: EventReader<PointsEvent>,
    layer: Query<Entity, With<PopupLayer>>,
    mut q: Query<(Entity, &mut Node, &mut TextColor, &mut Popup)>,
) {
    let Ok(layer) = layer.single() else { return };
    for ev in events.read() {
        let positive = ev.0 >= 0;
        let label = if positive { format!("+{}", ev.0) } else { ev.0.to_string() };
        let x = fastrand::f32() * 60.0;
        commands.entity(layer).with_children(|p| {
            p.spawn((
                Node { position_type: PositionType::Absolute, right: Val::Px(x), bottom: Val::Px(0.0), ..default() },
                Text::new(label),
                TextFont { font_size: 24.0, ..default() },
                TextColor(if positive { Color::srgb(1.0, 0.85, 0.3) } else { Color::srgb(0.9, 0.2, 0.15) }),
                Popup { ttl: 0.9, y: 0.0, positive },
            ));
        });
    }
    let dt = time.delta_secs();
    for (e, mut node, mut color, mut p) in &mut q {
        p.ttl -= dt;
        if p.ttl <= 0.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        p.y += dt * if p.positive { 90.0 } else { -40.0 };
        node.bottom = Val::Px(p.y.max(-30.0));
        color.0.set_alpha((p.ttl / 0.9).min(1.0));
    }
}

fn banner(
    time: Res<Time>,
    mut events: EventReader<Banner>,
    mut state: ResMut<BannerState>,
    mut q: Query<(&mut Text, &mut TextColor), With<BannerText>>,
) {
    let Ok((mut t, mut c)) = q.single_mut() else { return };
    for ev in events.read() {
        t.0 = ev.0.clone();
        state.ttl = 3.0;
    }
    state.ttl = (state.ttl - time.delta_secs()).max(0.0);
    c.0.set_alpha((state.ttl / 0.8).min(1.0));
}

#[allow(clippy::type_complexity)]
fn crosshair(
    gun: Res<Gun>,
    player: Query<&PlayerCtl, With<Player>>,
    state: Res<State<GameState>>,
    mut cross: Query<&mut Visibility, With<Crosshair>>,
    mut hit: Query<(&mut TextColor, &mut TextFont), With<HitMarker>>,
) {
    let ads = player.single().map(|p| p.ads).unwrap_or(0.0);
    if let Ok(mut v) = cross.single_mut() {
        *v = if ads > 0.5 || *state.get() != GameState::Playing { Visibility::Hidden } else { Visibility::Inherited };
    }
    if let Ok((mut c, mut f)) = hit.single_mut() {
        let a = (gun.hitmarker / 0.12).clamp(0.0, 1.0);
        c.0 = if gun.headmarker { Color::srgba(1.0, 0.3, 0.2, a) } else { Color::srgba(1.0, 1.0, 1.0, a) };
        f.font_size = 26.0 + a * 6.0;
    }
}

fn damage_overlay(health: Res<Health>, mut q: Query<&mut BackgroundColor, With<DamageOverlay>>) {
    if let Ok(mut bg) = q.single_mut() {
        let missing = 1.0 - (health.hp / rules::PLAYER_MAX_HEALTH).clamp(0.0, 1.0);
        let a = (missing * 0.45 + health.flash * 0.25).min(0.7);
        bg.0 = Color::srgba(0.55, 0.0, 0.0, a);
    }
}

/// Shows active power-up icons, blinking during their last five seconds.
fn powerup_icons(time: Res<Time>, pu: Res<ActivePowerups>, mut q: Query<(&PowerupIcon, &mut Visibility, &mut Node)>) {
    for (icon, mut vis, mut node) in &mut q {
        let left = if icon.0 == 0 { pu.insta_kill } else { pu.double_points };
        let blink = left < 5.0 && (time.elapsed_secs() * 6.0).sin() < 0.0;
        *vis = if left > 0.0 && !blink { Visibility::Inherited } else { Visibility::Hidden };
        // Hidden icons take no space so the others stay centred.
        node.display = if left > 0.0 { Display::Flex } else { Display::None };
    }
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

fn help_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    assets: Res<AssetDir>,
    waw: Res<Waw>,
    bank: Option<Res<SoundBank>>,
    mut shown: Local<Option<bool>>,
    mut q: Query<&mut Text, With<HelpText>>,
) {
    let visible = shown.get_or_insert(true);
    if keys.just_pressed(KeyCode::KeyH) {
        *visible = !*visible;
    }
    // Auto-hide after a while the first time.
    if time.elapsed_secs() > 25.0 && time.elapsed_secs() - time.delta_secs() <= 25.0 {
        *visible = false;
    }
    let Ok(mut t) = q.single_mut() else { return };
    if !*visible {
        t.0 = "H - help".into();
        return;
    }
    let source = match (&waw.install, &assets.root) {
        (Some(i), _) => i.root.display().to_string(),
        (None, Some(p)) => p.display().to_string(),
        (None, None) => "none - using built-in sounds".into(),
    };
    let assets_line = format!(
        "Assets: {source}  ({} sounds, {} weapon files)",
        bank.map(|b| b.loaded_from_disk).unwrap_or(0),
        assets.weapon_overrides
    );
    t.0 = format!(
        "UNDEAD ROUNDS\n\
         Esc pause menu\n\
         WASD move, Shift sprint, Space jump\n\
         LMB fire, RMB aim, R reload, V/E knife\n\
         1/2/Q/wheel switch weapon\n\
         F interact (hold at windows to rebuild)\n\
         -/= mouse sensitivity, [/] brightness\n\
         H toggle this help\n\
         {assets_line}"
    );
}
