//! Front end: main menu, options, pause menu, game-over screen and the
//! loading screen. Uses the install's own menu art and music when available.
//!
//! Keyboard: Up/Down select, Enter activate, Left/Right change a value,
//! Esc back/resume. The mouse works too.

use crate::settings::UserSettings;
use crate::waw::{Waw, WawImages};
use crate::{GameState, Round, Score};
use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapKind {
    /// The real Nacht der Untoten, read from the install.
    Nacht,
    /// The original procedural bunker (works without an install).
    Bunker,
}

impl MapKind {
    pub fn title(self) -> &'static str {
        match self {
            MapKind::Nacht => "NACHT DER UNTOTEN",
            MapKind::Bunker => "THE BUNKER (PROTOTYPE)",
        }
    }
}

/// The map the current/next session uses.
#[derive(Resource, Debug, Clone, Copy)]
pub struct CurrentMap(pub MapKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Page {
    #[default]
    Main,
    Options,
    Pause,
    GameOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Setting {
    Sensitivity,
    Fov,
    Master,
    Music,
    Sfx,
    Brightness,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Play(MapKind),
    Options,
    Quit,
    Back,
    Resume,
    Restart,
    ToMenu,
    Adjust(Setting),
}

#[derive(Component)]
struct MenuItem {
    action: Action,
    index: usize,
    enabled: bool,
}

#[derive(Component)]
struct ItemLabel(usize);

#[derive(Component)]
struct ItemBar(usize);

/// Root of whatever menu page is on screen.
#[derive(Component)]
struct MenuLayer;

#[derive(Component)]
struct LoadingLayer;

#[derive(Component)]
pub struct MenuMusic;

#[derive(Resource, Default)]
struct MenuNav {
    page: Page,
    /// Page to return to from Options.
    options_from: Page,
    selected: usize,
    count: usize,
    dirty: bool,
}

/// Fonts loaded from the system when available (Bevy's default otherwise).
#[derive(Resource, Default, Clone)]
pub struct UiFonts {
    pub title: Handle<Font>,
    pub body: Handle<Font>,
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuNav>()
            .add_systems(PreStartup, load_fonts)
            .add_systems(OnEnter(GameState::MainMenu), (enter_page(Page::Main), start_menu_music))
            .add_systems(OnExit(GameState::MainMenu), stop_menu_music)
            .add_systems(OnEnter(GameState::Paused), enter_page(Page::Pause))
            .add_systems(OnEnter(GameState::GameOver), enter_page(Page::GameOver))
            .add_systems(OnEnter(GameState::Playing), clear_menu)
            .add_systems(OnEnter(GameState::Loading), (clear_menu, spawn_loading_screen))
            .add_systems(OnExit(GameState::Loading), despawn_loading_screen)
            .add_systems(Update, pause_on_escape.run_if(in_state(GameState::Playing)))
            .add_systems(
                Update,
                (rebuild_menu, menu_input, item_visuals)
                    .chain()
                    .run_if(in_state(GameState::MainMenu).or(in_state(GameState::Paused)).or(in_state(GameState::GameOver))),
            )
            .add_systems(Update, (apply_window_settings, menu_music_volume));
    }
}

fn load_fonts(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let mut load = |names: &[&str]| -> Handle<Font> {
        let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into()).join("Fonts");
        for n in names {
            if let Ok(bytes) = std::fs::read(dir.join(n)) {
                if let Ok(f) = Font::try_from_bytes(bytes) {
                    return fonts.add(f);
                }
            }
        }
        Handle::default()
    };
    let title = load(&["impact.ttf", "bahnschrift.ttf", "arialbd.ttf"]);
    let body = load(&["bahnschrift.ttf", "segoeui.ttf", "arial.ttf"]);
    commands.insert_resource(UiFonts { title, body });
}

fn enter_page(page: Page) -> impl Fn(ResMut<MenuNav>) {
    move |mut nav: ResMut<MenuNav>| {
        nav.page = page;
        nav.selected = 0;
        nav.dirty = true;
    }
}

fn clear_menu(mut commands: Commands, q: Query<Entity, With<MenuLayer>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn start_menu_music(mut commands: Commands, waw: Res<Waw>, settings: Res<UserSettings>, mut sources: ResMut<Assets<AudioSource>>, existing: Query<(), With<MenuMusic>>) {
    if !existing.is_empty() {
        return;
    }
    let Some(src) = crate::waw::load_wav(&waw, "sound/Stream/Music/Mission/zombie/mx_splash_screen.wav") else { return };
    commands.spawn((
        AudioPlayer::new(sources.add(src)),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(settings.music())),
        MenuMusic,
    ));
}

fn stop_menu_music(mut commands: Commands, q: Query<Entity, With<MenuMusic>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn menu_music_volume(settings: Res<UserSettings>, mut q: Query<&mut AudioSink, With<MenuMusic>>) {
    if settings.is_changed() {
        for mut s in &mut q {
            s.set_volume(Volume::Linear(settings.music()));
        }
    }
}

fn pause_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut next: ResMut<NextState<GameState>>,
) {
    let lost_focus = !crate::autopilot_enabled() && windows.single().is_ok_and(|w| !w.focused);
    if keys.just_pressed(KeyCode::Escape) || lost_focus {
        next.set(GameState::Paused);
    }
}

fn setting_label(s: Setting, u: &UserSettings) -> String {
    match s {
        Setting::Sensitivity => format!("MOUSE SENSITIVITY   {:.1}", u.sensitivity * 1000.0),
        Setting::Fov => format!("FIELD OF VIEW   {:.0}", u.fov),
        Setting::Master => format!("MASTER VOLUME   {:.0}%", u.master_volume * 100.0),
        Setting::Music => format!("MUSIC VOLUME   {:.0}%", u.music_volume * 100.0),
        Setting::Sfx => format!("EFFECTS VOLUME   {:.0}%", u.sfx_volume * 100.0),
        Setting::Brightness => format!("BRIGHTNESS   {:.1}", 15.0 - u.exposure_ev),
        Setting::Fullscreen => format!("FULLSCREEN   {}", if u.fullscreen { "ON" } else { "OFF" }),
    }
}

fn adjust(s: Setting, u: &mut UserSettings, dir: f32) {
    let step = |v: f32, d: f32, lo: f32, hi: f32| (v + d * dir).clamp(lo, hi);
    match s {
        Setting::Sensitivity => u.sensitivity = step(u.sensitivity, 0.0002, 0.0004, 0.02),
        Setting::Fov => u.fov = step(u.fov, 5.0, 55.0, 100.0),
        Setting::Master => u.master_volume = step(u.master_volume, 0.1, 0.0, 1.0),
        Setting::Music => u.music_volume = step(u.music_volume, 0.1, 0.0, 1.0),
        Setting::Sfx => u.sfx_volume = step(u.sfx_volume, 0.1, 0.0, 1.0),
        Setting::Brightness => u.exposure_ev = step(u.exposure_ev, -0.5, 3.0, 14.0),
        Setting::Fullscreen => u.fullscreen = !u.fullscreen,
    }
    // Round away float noise so the labels stay tidy.
    u.master_volume = (u.master_volume * 10.0).round() / 10.0;
    u.music_volume = (u.music_volume * 10.0).round() / 10.0;
    u.sfx_volume = (u.sfx_volume * 10.0).round() / 10.0;
}

/// (label, action, enabled) for each item of a page.
fn page_items(page: Page, waw: &Waw, u: &UserSettings) -> Vec<(String, Action, bool)> {
    let s = |x: &str| x.to_string();
    match page {
        Page::Main => vec![
            (s("SOLO - NACHT DER UNTOTEN"), Action::Play(MapKind::Nacht), waw.available()),
            (s("SOLO - THE BUNKER (PROTOTYPE)"), Action::Play(MapKind::Bunker), true),
            (s("OPTIONS"), Action::Options, true),
            (s("QUIT"), Action::Quit, true),
        ],
        Page::Pause => vec![
            (s("RESUME"), Action::Resume, true),
            (s("RESTART"), Action::Restart, true),
            (s("OPTIONS"), Action::Options, true),
            (s("QUIT TO MAIN MENU"), Action::ToMenu, true),
            (s("QUIT GAME"), Action::Quit, true),
        ],
        Page::GameOver => vec![(s("PLAY AGAIN"), Action::Restart, true), (s("MAIN MENU"), Action::ToMenu, true)],
        Page::Options => {
            let mut v: Vec<(String, Action, bool)> = [
                Setting::Sensitivity,
                Setting::Fov,
                Setting::Master,
                Setting::Music,
                Setting::Sfx,
                Setting::Brightness,
                Setting::Fullscreen,
            ]
            .into_iter()
            .map(|st| (setting_label(st, u), Action::Adjust(st), true))
            .collect();
            v.push((s("BACK"), Action::Back, true));
            v
        }
    }
}

/// Full-screen, centred, cropped background image.
fn spawn_backdrop(p: &mut ChildSpawnerCommands, image: Option<Handle<Image>>, dim: f32) {
    if let Some(img) = image {
        p.spawn(Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            overflow: Overflow::clip(),
            ..default()
        })
        .with_children(|c| {
            c.spawn((ImageNode::new(img), Node { width: Val::Vw(100.0), height: Val::Vw(100.0), flex_shrink: 0.0, ..default() }));
        });
    }
    p.spawn((
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, dim)),
    ));
}

#[allow(clippy::too_many_arguments)]
fn rebuild_menu(
    mut commands: Commands,
    mut nav: ResMut<MenuNav>,
    layers: Query<Entity, With<MenuLayer>>,
    waw: Res<Waw>,
    settings: Res<UserSettings>,
    fonts: Res<UiFonts>,
    round: Res<Round>,
    score: Res<Score>,
    current: Res<crate::menu::CurrentMap>,
    (mut wimg, mut images, device): (ResMut<WawImages>, ResMut<Assets<Image>>, Option<Res<RenderDevice>>),
) {
    if !nav.dirty {
        return;
    }
    nav.dirty = false;
    for e in &layers {
        commands.entity(e).despawn();
    }
    let page = nav.page;
    let items = page_items(page, &waw, &settings);
    nav.count = items.len();
    nav.selected = nav.selected.min(items.len().saturating_sub(1));
    if !items[nav.selected].2 {
        nav.selected = items.iter().position(|i| i.2).unwrap_or(0);
    }

    let in_game = matches!(page, Page::Pause | Page::GameOver) || (page == Page::Options && nav.options_from != Page::Main);
    let backdrop = if in_game { None } else { wimg.get(&waw, &mut images, device.as_deref(), "loadscreen_zombie1", true, false) };
    let pale = Color::srgb(0.92, 0.9, 0.84);
    let blood = Color::srgb(0.75, 0.06, 0.03);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(if in_game { Color::NONE } else { Color::BLACK }),
            GlobalZIndex(10),
            MenuLayer,
        ))
        .with_children(|root| {
            spawn_backdrop(root, backdrop, if in_game { 0.72 } else { 0.35 });
            // Left-hand dark column for readability.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    width: Val::Percent(48.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::left(Val::Percent(7.0)),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, if in_game { 0.0 } else { 0.55 })),
            ))
            .with_children(|col| {
                let (title, subtitle) = match page {
                    Page::Main => ("UNDEAD ROUNDS".to_string(), "ZOMBIES".to_string()),
                    Page::Options => ("OPTIONS".to_string(), String::new()),
                    Page::Pause => ("PAUSED".to_string(), current.0.title().to_string()),
                    Page::GameOver => {
                        let r = round.0.round.max(1);
                        (
                            "GAME OVER".to_string(),
                            format!(
                                "You survived {r} round{}\nKills {}   Headshots {}   Points {}",
                                if r == 1 { "" } else { "s" },
                                score.kills,
                                score.headshots,
                                score.points
                            ),
                        )
                    }
                };
                col.spawn((
                    Text::new(title),
                    TextFont { font: fonts.title.clone(), font_size: if page == Page::Main { 84.0 } else { 64.0 }, ..default() },
                    TextColor(if page == Page::GameOver { blood } else { pale }),
                ));
                if !subtitle.is_empty() {
                    col.spawn((
                        Text::new(subtitle),
                        TextFont { font: fonts.body.clone(), font_size: 24.0, ..default() },
                        TextColor(if page == Page::Main { blood } else { Color::srgb(0.75, 0.73, 0.68) }),
                        Node { margin: UiRect::bottom(Val::Px(36.0)), ..default() },
                    ));
                } else {
                    col.spawn(Node { height: Val::Px(36.0), ..default() });
                }
                for (i, (label, action, enabled)) in items.iter().enumerate() {
                    col.spawn((
                        Button,
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(12.0),
                            padding: UiRect::axes(Val::Px(4.0), Val::Px(5.0)),
                            ..default()
                        },
                        MenuItem { action: *action, index: i, enabled: *enabled },
                    ))
                    .with_children(|b| {
                        b.spawn((Node { width: Val::Px(5.0), height: Val::Px(26.0), ..default() }, BackgroundColor(Color::NONE), ItemBar(i)));
                        b.spawn((
                            Text::new(label.clone()),
                            TextFont { font: fonts.body.clone(), font_size: 28.0, ..default() },
                            TextColor(pale),
                            ItemLabel(i),
                        ));
                    });
                }
            });

            // Status line: where the assets come from.
            if page == Page::Main {
                let (msg, color) = match (&waw.install, &waw.error) {
                    (Some(i), None) => (format!("World at War: {}", i.root.display()), Color::srgba(0.8, 0.8, 0.75, 0.7)),
                    (_, Some(e)) => (e.clone(), Color::srgb(0.95, 0.4, 0.3)),
                    _ => (String::new(), Color::NONE),
                };
                root.spawn((
                    Node { position_type: PositionType::Absolute, left: Val::Px(20.0), bottom: Val::Px(14.0), ..default() },
                    Text::new(format!("{msg}\nUp/Down + Enter, or use the mouse")),
                    TextFont { font: fonts.body.clone(), font_size: 15.0, ..default() },
                    TextColor(color),
                ));
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut nav: ResMut<MenuNav>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut current: ResMut<CurrentMap>,
    mut settings: ResMut<UserSettings>,
    mut exit: EventWriter<AppExit>,
    items: Query<(&MenuItem, &Interaction), Changed<Interaction>>,
    all: Query<&MenuItem>,
) {
    if nav.dirty || nav.count == 0 {
        return;
    }
    let enabled = |i: usize| all.iter().any(|m| m.index == i && m.enabled);
    let mut activate: Option<(Action, f32)> = None;

    for (item, interaction) in &items {
        match interaction {
            Interaction::Hovered if item.enabled => nav.selected = item.index,
            Interaction::Pressed if item.enabled => {
                nav.selected = item.index;
                activate = Some((item.action, 1.0));
            }
            _ => {}
        }
    }
    let n = nav.count;
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) {
        for k in 1..=n {
            let i = (nav.selected + k) % n;
            if enabled(i) {
                nav.selected = i;
                break;
            }
        }
    }
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
        for k in 1..=n {
            let i = (nav.selected + n - k) % n;
            if enabled(i) {
                nav.selected = i;
                break;
            }
        }
    }
    let selected_action = all.iter().find(|m| m.index == nav.selected).map(|m| m.action);
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) || keys.just_pressed(KeyCode::Space) {
        activate = selected_action.map(|a| (a, 1.0));
    }
    if let Some(Action::Adjust(s)) = selected_action {
        if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) {
            activate = Some((Action::Adjust(s), 1.0));
        }
        if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) {
            activate = Some((Action::Adjust(s), -1.0));
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        activate = match nav.page {
            Page::Options => Some((Action::Back, 1.0)),
            Page::Pause => Some((Action::Resume, 1.0)),
            _ => None,
        };
    }

    let Some((action, dir)) = activate else { return };
    match action {
        Action::Play(map) => {
            current.0 = map;
            next.set(GameState::Loading);
        }
        Action::Options => {
            nav.options_from = nav.page;
            nav.page = Page::Options;
            nav.selected = 0;
            nav.dirty = true;
        }
        Action::Back => {
            nav.page = nav.options_from;
            nav.selected = 0;
            nav.dirty = true;
            settings.save();
        }
        Action::Quit => {
            exit.write(AppExit::Success);
        }
        Action::Resume => next.set(GameState::Playing),
        Action::Restart => next.set(GameState::Loading),
        Action::ToMenu => {
            if *state.get() != GameState::MainMenu {
                next.set(GameState::MainMenu);
            }
        }
        Action::Adjust(s) => {
            adjust(s, &mut settings, dir);
            nav.dirty = true;
        }
    }
}

fn item_visuals(
    nav: Res<MenuNav>,
    items: Query<&MenuItem>,
    mut labels: Query<(&ItemLabel, &mut TextColor)>,
    mut bars: Query<(&ItemBar, &mut BackgroundColor)>,
) {
    for (l, mut c) in &mut labels {
        let enabled = items.iter().any(|m| m.index == l.0 && m.enabled);
        c.0 = if !enabled {
            Color::srgba(0.6, 0.6, 0.6, 0.35)
        } else if l.0 == nav.selected {
            Color::srgb(1.0, 0.98, 0.9)
        } else {
            Color::srgb(0.72, 0.7, 0.65)
        };
    }
    for (b, mut bg) in &mut bars {
        bg.0 = if b.0 == nav.selected { Color::srgb(0.75, 0.06, 0.03) } else { Color::NONE };
    }
}

fn apply_window_settings(settings: Res<UserSettings>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if !settings.is_changed() {
        return;
    }
    let Ok(mut w) = windows.single_mut() else { return };
    let want = if settings.fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed };
    if w.mode != want {
        w.mode = want;
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_loading_screen(
    mut commands: Commands,
    current: Res<CurrentMap>,
    waw: Res<Waw>,
    fonts: Res<UiFonts>,
    mut wimg: ResMut<WawImages>,
    mut images: ResMut<Assets<Image>>,
    device: Option<Res<RenderDevice>>,
) {
    let backdrop = match current.0 {
        MapKind::Nacht => wimg.get(&waw, &mut images, device.as_deref(), "loadscreen_zombie1", true, false),
        MapKind::Bunker => wimg.get(&waw, &mut images, device.as_deref(), "menu_background_coop", true, false),
    };
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(20),
            LoadingLayer,
        ))
        .with_children(|root| {
            spawn_backdrop(root, backdrop, 0.2);
            root.spawn((
                Node { position_type: PositionType::Absolute, left: Val::Percent(6.0), bottom: Val::Percent(8.0), flex_direction: FlexDirection::Column, ..default() },
            ))
            .with_children(|c| {
                c.spawn((
                    Text::new(current.0.title()),
                    TextFont { font: fonts.title.clone(), font_size: 56.0, ..default() },
                    TextColor(Color::srgb(0.92, 0.9, 0.84)),
                ));
                c.spawn((
                    Text::new("LOADING..."),
                    TextFont { font: fonts.body.clone(), font_size: 22.0, ..default() },
                    TextColor(Color::srgb(0.75, 0.06, 0.03)),
                    LoadingStatus,
                ));
            });
        });
}

/// Text under the map title on the loading screen.
#[derive(Component)]
pub struct LoadingStatus;

fn despawn_loading_screen(mut commands: Commands, q: Query<Entity, With<LoadingLayer>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}
