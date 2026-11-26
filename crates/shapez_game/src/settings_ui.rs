//! Écran des paramètres (port partiel de `states/settings.js`) : langue, volumes et
//! plein écran. Les volumes et le plein écran s'appliquent tout de suite, la langue au
//! prochain lancement (`restartRequired`).

use bevy::{
    prelude::*,
    ui_widgets::{Activate, ActivateOnPress, Button},
};

use crate::{
    AppState,
    i18n::{LANGUAGES, language_by_id, t, tf},
    settings::{LaunchLanguage, Settings, VOLUME_STEP, clamp_volume},
    theme::ThemeId,
};

#[derive(Debug, Default)]
pub struct SettingsUiPlugin;

impl Plugin for SettingsUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Settings), spawn_screen)
            .add_systems(
                Update,
                (back_on_escape, refresh).run_if(in_state(AppState::Settings)),
            )
            .add_observer(on_activate);
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Language(i32),
    SoundVolume(i32),
    MusicVolume(i32),
    Fullscreen,
    Theme(i32),
    ColorBlind,
    Back,
}

/// Conteneur des lignes, reconstruit quand les paramètres changent.
#[derive(Component, Debug)]
struct Rows;

/// Langues proposées, `None` = langue du système.
fn language_choices() -> Vec<Option<&'static str>> {
    std::iter::once(None)
        .chain(LANGUAGES.iter().map(|l| Some(l.id)))
        .collect()
}

fn language_label(id: Option<&str>) -> String {
    match id.and_then(language_by_id) {
        Some(language) => language.name.to_owned(),
        None => t("port.settings.systemLanguage"),
    }
}

fn percent_label(volume: f32) -> String {
    tf(
        "settings.rangeSliderPercentage",
        &[("amount", &((volume * 100.0).round() as i32).to_string())],
    )
}

fn spawn_screen(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::Settings),
        Rows,
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(14),
            ..default()
        },
    ));
}

fn label(text: impl Into<String>, size: f32) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
    )
}

fn button(action: Action, text: impl Into<String>, width: f32) -> impl Bundle {
    (
        action,
        Button,
        ActivateOnPress,
        Node {
            width: px(width),
            min_height: px(40),
            padding: UiRect::axes(px(10), px(4)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgb(0.2, 0.2, 0.25)),
        children![label(text, 18.0)],
    )
}

/// Ligne « titre : [-] valeur [+] ».
fn stepper(p: &mut ChildSpawnerCommands, title: String, value: String, less: Action, more: Action) {
    p.spawn(Node {
        column_gap: px(10),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            label(title, 18.0),
            Node {
                width: px(220),
                ..default()
            },
        ));
        row.spawn(button(less, "<", 44.0));
        row.spawn((
            label(value, 18.0),
            Node {
                width: px(220),
                justify_content: JustifyContent::Center,
                ..default()
            },
            TextLayout::justify(Justify::Center),
        ));
        row.spawn(button(more, ">", 44.0));
    });
}

/// Ligne « titre : [activé / désactivé] ».
fn toggle(p: &mut ChildSpawnerCommands, title: String, on: bool, action: Action) {
    p.spawn(Node {
        column_gap: px(10),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            label(title, 18.0),
            Node {
                width: px(220),
                ..default()
            },
        ));
        row.spawn(button(
            action,
            t(if on {
                "port.settings.on"
            } else {
                "port.settings.off"
            }),
            328.0,
        ));
    });
}

fn refresh(
    mut commands: Commands,
    settings: Res<Settings>,
    rows: Single<Entity, With<Rows>>,
    mut built: Local<bool>,
    state: Res<State<AppState>>,
    launch: Res<LaunchLanguage>,
) {
    if state.is_changed() {
        *built = false;
    }
    if *built && !settings.is_changed() {
        return;
    }
    *built = true;
    let restart = settings.language != launch.0;
    commands
        .entity(*rows)
        .despawn_children()
        .with_children(|p| {
            p.spawn(label(t("settings.title"), 36.0));
            stepper(
                p,
                t("settings.labels.language.title"),
                language_label(settings.language.as_deref()),
                Action::Language(-1),
                Action::Language(1),
            );
            stepper(
                p,
                t("settings.labels.soundVolume.title"),
                percent_label(settings.sound_volume),
                Action::SoundVolume(-1),
                Action::SoundVolume(1),
            );
            stepper(
                p,
                t("settings.labels.musicVolume.title"),
                percent_label(settings.music_volume),
                Action::MusicVolume(-1),
                Action::MusicVolume(1),
            );
            toggle(
                p,
                t("settings.labels.fullscreen.title"),
                settings.fullscreen,
                Action::Fullscreen,
            );
            stepper(
                p,
                t("settings.labels.theme.title"),
                t(&format!(
                    "settings.labels.theme.themes.{}",
                    settings.theme.id()
                )),
                Action::Theme(-1),
                Action::Theme(1),
            );
            toggle(
                p,
                t("settings.labels.enableColorBlindHelper.title"),
                settings.color_blind,
                Action::ColorBlind,
            );
            if restart {
                p.spawn((
                    label(t("dialogs.restartRequired.text"), 16.0),
                    TextColor(Color::srgb(1.0, 0.8, 0.4)),
                ));
            }
            p.spawn(button(Action::Back, t("mainMenu.back"), 240.0));
        });
}

fn step(volume: f32, direction: i32) -> f32 {
    clamp_volume(volume + direction as f32 * VOLUME_STEP)
}

fn on_activate(
    activate: On<Activate>,
    actions: Query<&Action>,
    settings: Option<ResMut<Settings>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let (Ok(&action), Some(mut settings)) = (actions.get(activate.event_target()), settings) else {
        return;
    };
    match action {
        Action::Language(direction) => {
            let choices = language_choices();
            let index = choices
                .iter()
                .position(|c| *c == settings.language.as_deref())
                .unwrap_or(0) as i32;
            let len = choices.len() as i32;
            let next_index = (index + direction).rem_euclid(len) as usize;
            settings.language = choices[next_index].map(str::to_owned);
        }
        Action::SoundVolume(direction) => {
            settings.sound_volume = step(settings.sound_volume, direction);
        }
        Action::MusicVolume(direction) => {
            settings.music_volume = step(settings.music_volume, direction);
        }
        Action::Fullscreen => settings.fullscreen = !settings.fullscreen,
        Action::Theme(direction) => {
            let themes = ThemeId::ALL;
            let index = themes
                .iter()
                .position(|&t| t == settings.theme)
                .unwrap_or(0) as i32;
            let next_index = (index + direction).rem_euclid(themes.len() as i32) as usize;
            settings.theme = themes[next_index];
        }
        Action::ColorBlind => settings.color_blind = !settings.color_blind,
        Action::Back => next.set(AppState::MainMenu),
    }
}

fn back_on_escape(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::MainMenu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_steps_stay_in_range() {
        assert_eq!(step(1.0, 1), 1.0);
        assert_eq!(step(0.0, -1), 0.0);
        assert!((step(0.5, 1) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn language_choices_start_with_the_system_language() {
        let choices = language_choices();
        assert_eq!(choices[0], None);
        assert_eq!(choices.len(), LANGUAGES.len() + 1);
    }
}
