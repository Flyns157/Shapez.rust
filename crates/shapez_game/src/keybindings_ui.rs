//! Écran des contrôles (port de `states/keybindings.js`), ouvert depuis les paramètres :
//! une ligne par action, avec sa touche, un bouton pour la changer et un pour revenir à
//! la touche par défaut. Pour changer une touche, on clique puis on tape la nouvelle ;
//! Échap annule (`dialogs.editKeybinding`).

use bevy::{
    input::{
        ButtonState,
        keyboard::KeyboardInput,
        mouse::{MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    ui_widgets::{Activate, ActivateOnPress, Button},
};

use crate::{
    AppState,
    i18n::t,
    keybindings::{self, Action, Category, is_assignable, key_name},
    settings::Settings,
};

/// Hauteur de la liste défilante, en pixels.
const LIST_HEIGHT: f32 = 470.0;
/// Défilement d'un cran de molette.
const LINE_PIXELS: f32 = 40.0;

#[derive(Debug, Default)]
pub struct KeybindingsUiPlugin;

impl Plugin for KeybindingsUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Capturing>()
            .add_systems(OnEnter(AppState::Keybindings), spawn_screen)
            .add_systems(OnExit(AppState::Keybindings), stop_capture)
            .add_systems(
                Update,
                (capture_key, back_on_escape, scroll, refresh)
                    .chain()
                    .run_if(in_state(AppState::Keybindings)),
            )
            .add_observer(on_activate);
    }
}

/// Action dont on attend la nouvelle touche.
#[derive(Resource, Debug, Default, PartialEq)]
struct Capturing(Option<Action>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Change(Action),
    Reset(Action),
    ResetAll,
    Back,
}

/// Contenu de la liste, reconstruit quand une touche change.
#[derive(Component, Debug)]
struct List;

/// Invite affichée pendant la capture d'une touche.
#[derive(Component, Debug)]
struct Prompt;

/// Message de confirmation (`dialogs.keybindingsResetOk`).
#[derive(Component, Debug)]
struct Status;

fn label(text: impl Into<String>, size: f32) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
    )
}

fn button(action: Control, text: impl Into<String>, width: f32, enabled: bool) -> impl Bundle {
    let (background, color) = if enabled {
        (Color::srgb(0.2, 0.2, 0.25), Color::WHITE)
    } else {
        (Color::srgb(0.16, 0.16, 0.19), Color::srgb(0.45, 0.45, 0.5))
    };
    (
        action,
        Button,
        ActivateOnPress,
        Node {
            width: px(width),
            min_height: px(30),
            padding: UiRect::axes(px(8), px(2)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(background),
        children![(label(text, 15.0), TextColor(color))],
    )
}

fn spawn_screen(mut commands: Commands, mut capturing: ResMut<Capturing>) {
    capturing.0 = None;
    commands.spawn((
        DespawnOnExit(AppState::Keybindings),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(10),
            ..default()
        },
        children![
            label(t("keybindings.title"), 32.0),
            (
                label(t("keybindings.hint"), 14.0),
                TextColor(Color::srgb(0.75, 0.75, 0.8)),
            ),
            (
                List,
                Node {
                    width: px(760),
                    height: px(LIST_HEIGHT),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
            ),
            // Hauteur fixe : l'invite n'a pas à décaler l'écran quand elle apparaît.
            (
                Prompt,
                label("", 18.0),
                TextColor(Color::srgb(1.0, 0.8, 0.4)),
                TextLayout::justify(Justify::Center),
                Node {
                    height: px(48),
                    ..default()
                },
            ),
            (
                Status,
                label("", 15.0),
                TextColor(Color::srgb(0.6, 0.9, 0.6)),
                Node {
                    height: px(20),
                    ..default()
                },
            ),
            (
                Node {
                    column_gap: px(12),
                    ..default()
                },
                children![
                    button(
                        Control::ResetAll,
                        t("keybindings.resetKeybindings"),
                        260.0,
                        true
                    ),
                    button(Control::Back, t("mainMenu.back"), 200.0, true),
                ],
            ),
        ],
    ));
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    settings: Res<Settings>,
    capturing: Res<Capturing>,
    list: Single<Entity, With<List>>,
    mut prompt: Single<&mut Text, With<Prompt>>,
    mut built: Local<bool>,
    state: Res<State<AppState>>,
) {
    if state.is_changed() {
        *built = false;
    }
    if *built && !settings.is_changed() && !capturing.is_changed() {
        return;
    }
    *built = true;
    prompt.0 = match capturing.0 {
        Some(action) => format!(
            "{} : {}\n{}",
            t("dialogs.editKeybinding.title"),
            action.title(),
            t("dialogs.editKeybinding.desc")
        ),
        None => String::new(),
    };
    let overrides = keybindings::parse_overrides(&settings.keybindings);
    commands
        .entity(*list)
        .despawn_children()
        .with_children(|p| {
            for category in Category::ALL {
                p.spawn((
                    label(
                        t(&format!("keybindings.categoryLabels.{}", category.id())),
                        20.0,
                    ),
                    Node {
                        margin: UiRect::top(px(8)),
                        ..default()
                    },
                ));
                for action in Action::all().filter(|a| a.category() == category) {
                    let changed = overrides.contains_key(&action);
                    let key = overrides
                        .get(&action)
                        .copied()
                        .unwrap_or_else(|| action.default_key());
                    let waiting = capturing.0 == Some(action);
                    row(p, action, key, changed, waiting);
                }
            }
        });
}

/// Ligne « action  touche  [changer] [réinitialiser] » ; une touche changée est en
/// jaune (`.mapping.changed`).
fn row(p: &mut ChildSpawnerCommands, action: Action, key: KeyCode, changed: bool, waiting: bool) {
    p.spawn(Node {
        column_gap: px(10),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            label(action.title(), 16.0),
            Node {
                width: px(330),
                ..default()
            },
        ));
        let (text, color) = if waiting {
            ("…".to_owned(), Color::srgb(1.0, 0.8, 0.4))
        } else if changed {
            (keybindings::key_label(key), Color::srgb(1.0, 0.85, 0.3))
        } else {
            (keybindings::key_label(key), Color::WHITE)
        };
        row.spawn((
            label(text, 16.0),
            TextColor(color),
            Node {
                width: px(90),
                justify_content: JustifyContent::Center,
                ..default()
            },
            TextLayout::justify(Justify::Center),
        ));
        row.spawn(button(
            Control::Change(action),
            t("port.keybindings.change"),
            130.0,
            true,
        ));
        row.spawn(button(
            Control::Reset(action),
            t("port.keybindings.reset"),
            150.0,
            changed,
        ));
    });
}

/// Enregistre une touche dans les surcharges ; la touche par défaut retire la surcharge.
fn set_key(settings: &mut Settings, action: Action, key: KeyCode) {
    if key == action.default_key() {
        settings.keybindings.remove(action.id());
    } else {
        settings
            .keybindings
            .insert(action.id().to_owned(), key_name(key));
    }
}

fn on_activate(
    activate: On<Activate>,
    buttons: Query<&Control>,
    settings: Option<ResMut<Settings>>,
    mut capturing: ResMut<Capturing>,
    mut status: Query<&mut Text, With<Status>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let (Ok(&button), Some(mut settings)) = (buttons.get(activate.event_target()), settings) else {
        return;
    };
    for mut text in &mut status {
        text.0.clear();
    }
    match button {
        Control::Change(action) => capturing.0 = Some(action),
        Control::Reset(action) => {
            capturing.0 = None;
            if settings.keybindings.contains_key(action.id()) {
                settings.keybindings.remove(action.id());
            }
        }
        Control::ResetAll => {
            capturing.0 = None;
            if !settings.keybindings.is_empty() {
                settings.keybindings.clear();
            }
            for mut text in &mut status {
                text.0 = t("dialogs.keybindingsResetOk.desc");
            }
        }
        Control::Back => next.set(AppState::Settings),
    }
}

/// Pendant la capture, la prochaine touche assignable devient celle de l'action.
/// Échap annule ; Entrée et les modificateurs sont ignorés, comme dans le JS.
fn capture_key(
    mut input: MessageReader<KeyboardInput>,
    mut capturing: ResMut<Capturing>,
    mut settings: ResMut<Settings>,
) {
    let Some(action) = capturing.0 else {
        input.clear();
        return;
    };
    for event in input.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }
        if event.key_code == KeyCode::Escape {
            capturing.0 = None;
            return;
        }
        if is_assignable(event.key_code) {
            set_key(&mut settings, action, event.key_code);
            capturing.0 = None;
            return;
        }
    }
}

/// Échap revient aux paramètres, sauf s'il vient d'annuler une capture.
fn back_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    capturing: Res<Capturing>,
    mut next: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::Escape) && capturing.0.is_none() && !capturing.is_changed() {
        next.set(AppState::Settings);
    }
}

fn scroll(mut wheel: MessageReader<MouseWheel>, mut list: Single<&mut ScrollPosition, With<List>>) {
    for event in wheel.read() {
        let dy = match event.unit {
            MouseScrollUnit::Line => event.y * LINE_PIXELS,
            MouseScrollUnit::Pixel => event.y,
        };
        // La mise en page ramène la position dans les bornes de la liste.
        list.y = (list.y - dy).max(0.0);
    }
}

fn stop_capture(mut capturing: ResMut<Capturing>) {
    capturing.0 = None;
}

#[cfg(test)]
mod tests {
    use shapez_sim::building::BuildingKind;

    use super::*;

    #[test]
    fn default_key_clears_the_override() {
        let mut settings = Settings::default();
        set_key(&mut settings, Action::Pipette, KeyCode::KeyH);
        assert_eq!(settings.keybindings["pipette"], "KeyH");
        set_key(&mut settings, Action::Pipette, KeyCode::KeyQ);
        assert!(settings.keybindings.is_empty());

        let belt = Action::Building(BuildingKind::Belt);
        set_key(&mut settings, belt, KeyCode::Numpad1);
        assert_eq!(
            keybindings::parse_overrides(&settings.keybindings)[&belt],
            KeyCode::Numpad1
        );
    }
}
