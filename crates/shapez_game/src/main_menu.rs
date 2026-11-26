//! Menu principal : continuer la dernière partie, en commencer une, charger ou
//! supprimer une sauvegarde, ouvrir la galerie des formes.

use bevy::{
    prelude::*,
    ui_widgets::{Activate, ActivateOnPress, Button},
};
use shapez_save::{decode, storage::SaveEntry};
use shapez_sim::TICKS_PER_SECOND;

use crate::{
    AppState,
    i18n::{t, tf},
    saves::Saves,
    sim::NextGame,
};

/// Nombre de sauvegardes listées.
const LISTED_SAVES: usize = 8;

#[derive(Debug, Default)]
pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(OnEnter(AppState::MainMenu), (reset_menu_state, spawn_menu))
            .add_observer(on_activate)
            .add_systems(Update, refresh_saves.run_if(in_state(AppState::MainMenu)));
    }
}

/// État du menu : liste à reconstruire, suppression en attente de confirmation,
/// message d'erreur.
#[derive(Resource, Debug, Default)]
struct MenuState {
    dirty: bool,
    pending_delete: Option<String>,
    status: String,
}

#[derive(Component, Debug, Clone)]
enum MenuButton {
    Continue(String),
    NewGame,
    Gallery,
    Settings,
    Load(String),
    Delete(String),
}

#[derive(Component, Debug)]
struct SaveList;

fn reset_menu_state(mut state: ResMut<MenuState>) {
    *state = MenuState {
        dirty: true,
        ..default()
    };
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

fn button(action: MenuButton, text: impl Into<String>, width: f32, color: Color) -> impl Bundle {
    (
        action,
        Button,
        ActivateOnPress,
        Node {
            width: px(width),
            min_height: px(44),
            padding: UiRect::axes(px(12), px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(color),
        children![label(text, 18.0)],
    )
}

const BUTTON: Color = Color::srgb(0.2, 0.2, 0.25);
const ACCENT: Color = Color::srgb(0.2, 0.45, 0.3);
const DANGER: Color = Color::srgb(0.55, 0.2, 0.2);

fn spawn_menu(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::MainMenu),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(16),
            ..default()
        },
        children![
            label("shapez-rs", 48.0),
            (
                SaveList,
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(10),
                    ..default()
                },
            ),
        ],
    ));
}

/// Durée de jeu lisible (« 1 h 05 min », « 12 min »).
fn play_time(ticks: u64) -> String {
    let minutes = ticks / (u64::from(TICKS_PER_SECOND) * 60);
    if minutes >= 60 {
        tf(
            "port.time.hoursMinutes",
            &[
                ("hours", &(minutes / 60).to_string()),
                ("minutes", &format!("{:02}", minutes % 60)),
            ],
        )
    } else {
        tf("port.time.minutes", &[("minutes", &minutes.to_string())])
    }
}

fn describe(entry: &SaveEntry) -> String {
    match &entry.meta {
        Ok(meta) => format!(
            "{} - {} - {}",
            entry.name,
            tf("mainMenu.savegameLevel", &[("x", &meta.level.to_string())]),
            play_time(meta.ticks)
        ),
        Err(_) => tf("port.menu.unreadable", &[("name", &entry.name)]),
    }
}

fn refresh_saves(
    mut commands: Commands,
    mut state: ResMut<MenuState>,
    saves: Res<Saves>,
    list: Single<Entity, With<SaveList>>,
) {
    if !state.dirty {
        return;
    }
    state.dirty = false;
    let entries = saves.0.as_ref().map(|d| d.list()).unwrap_or_default();
    let latest = entries.iter().find(|e| e.meta.is_ok()).cloned();
    let pending = state.pending_delete.clone();
    let status = state.status.clone();

    commands
        .entity(*list)
        .despawn_children()
        .with_children(|p| {
            if let Some(latest) = &latest {
                p.spawn(button(
                    MenuButton::Continue(latest.name.clone()),
                    format!("{} ({})", t("mainMenu.continue"), describe(latest)),
                    420.0,
                    ACCENT,
                ));
            }
            p.spawn(button(
                MenuButton::NewGame,
                t("mainMenu.newGame"),
                420.0,
                BUTTON,
            ));
            p.spawn(button(
                MenuButton::Gallery,
                t("port.menu.shapes"),
                420.0,
                BUTTON,
            ));
            p.spawn(button(
                MenuButton::Settings,
                t("settings.title"),
                420.0,
                BUTTON,
            ));
            if !status.is_empty() {
                p.spawn((label(status, 16.0), TextColor(Color::srgb(1.0, 0.5, 0.45))));
            }
            if !entries.is_empty() {
                p.spawn(label(t("port.menu.savegames"), 22.0));
            }
            for entry in entries.iter().take(LISTED_SAVES) {
                let confirming = pending.as_deref() == Some(entry.name.as_str());
                p.spawn(Node {
                    column_gap: px(8),
                    ..default()
                })
                .with_children(|row| {
                    if entry.meta.is_ok() {
                        row.spawn(button(
                            MenuButton::Load(entry.name.clone()),
                            describe(entry),
                            420.0,
                            BUTTON,
                        ));
                    } else {
                        row.spawn((
                            Node {
                                width: px(420),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            children![label(describe(entry), 18.0)],
                        ));
                    }
                    row.spawn(button(
                        MenuButton::Delete(entry.name.clone()),
                        t(if confirming {
                            "port.menu.confirm"
                        } else {
                            "dialogs.buttons.delete"
                        }),
                        130.0,
                        if confirming { DANGER } else { BUTTON },
                    ));
                });
            }
            if let Some(dir) = &saves.0 {
                p.spawn((
                    label(
                        tf(
                            "port.menu.folder",
                            &[("path", &dir.root().display().to_string())],
                        ),
                        14.0,
                    ),
                    TextColor(Color::srgb(0.6, 0.6, 0.6)),
                ));
            }
        });
}

fn on_activate(
    activate: On<Activate>,
    buttons: Query<&MenuButton>,
    saves: Res<Saves>,
    mut state: ResMut<MenuState>,
    mut next_game: ResMut<NextGame>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Ok(action) = buttons.get(activate.event_target()) else {
        return;
    };
    let dir = saves.0.as_ref();
    match action {
        MenuButton::NewGame => {
            *next_game = NextGame::New;
            next.set(AppState::InGame);
        }
        MenuButton::Gallery => next.set(AppState::ShapeGallery),
        MenuButton::Settings => next.set(AppState::Settings),
        MenuButton::Continue(name) | MenuButton::Load(name) => {
            let loaded = dir
                .ok_or_else(|| t("port.menu.noSaveFolder"))
                .and_then(|d| d.read(name).map_err(|e| e.to_string()))
                .and_then(|bytes| decode(&bytes).map_err(|e| e.to_string()));
            match loaded {
                Ok((_, world)) => {
                    *next_game = NextGame::Load {
                        name: name.clone(),
                        world: Box::new(world),
                    };
                    next.set(AppState::InGame);
                }
                Err(e) => {
                    state.status = tf("port.menu.loadFailed", &[("name", name), ("error", &e)]);
                    state.dirty = true;
                }
            }
        }
        MenuButton::Delete(name) => {
            // Deux clics : le premier arme la suppression, le second l'exécute.
            if state.pending_delete.as_deref() == Some(name.as_str()) {
                state.pending_delete = None;
                if let Some(Err(e)) = dir.map(|d| d.delete(name)) {
                    state.status = tf(
                        "port.menu.deleteFailed",
                        &[("name", name), ("error", &e.to_string())],
                    );
                }
            } else {
                state.pending_delete = Some(name.clone());
            }
            state.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_time_is_readable() {
        crate::i18n::tests::use_test_translations();
        assert_eq!(play_time(0), "0 min");
        assert_eq!(play_time(60 * 60 * 12), "12 min");
        assert_eq!(play_time(60 * 60 * 65), "1 h 05 min");
    }
}
