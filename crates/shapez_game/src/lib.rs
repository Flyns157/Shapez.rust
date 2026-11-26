//! Couche Bevy du jeu. La logique vit dans `shapez_sim` ; ce crate ne fait
//! qu'afficher, lire les entrées et piloter le cycle de vie (plan, §3.1 et §3.5).

use bevy::prelude::*;

pub mod audio;
pub mod buildings_view;
pub mod camera;
pub mod color_blind;
pub mod editing_ui;
pub mod gallery;
pub mod i18n;
pub mod items_view;
pub mod main_menu;
pub mod map_render;
pub mod marks_ui;
pub mod menu;
pub mod overlays;
pub mod placement_ui;
pub mod saves;
pub mod settings;
pub mod settings_ui;
pub mod shape_textures;
pub mod shop;
pub mod signal_edit;
pub mod sim;
pub mod state;
pub mod theme;
pub mod underlays;
pub mod wires_ui;

pub use state::AppState;

/// Regroupe tous les plugins du jeu.
#[derive(Debug, Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // Les paramètres sont lus avant tout : la langue en dépend.
        let settings_path = settings::Settings::default_path();
        let settings = settings_path
            .as_deref()
            .map(settings::Settings::load)
            .unwrap_or_default();
        app.add_plugins((
            i18n::I18nPlugin {
                asset_dir: asset_dir(),
                preferred: settings.language.clone(),
            },
            settings::SettingsPlugin {
                settings,
                path: settings_path,
            },
            audio::AudioPlugin {
                asset_dir: asset_dir(),
            },
            settings_ui::SettingsUiPlugin,
            theme::ThemePlugin,
            color_blind::ColorBlindPlugin,
            state::AppStatePlugin,
            sim::SimPlugin,
            menu::MenuPlugin,
            main_menu::MainMenuPlugin,
        ))
        .add_plugins((
            shape_textures::ShapeTexturesPlugin,
            gallery::GalleryPlugin,
            camera::CameraPlugin,
            map_render::MapRenderPlugin,
            buildings_view::BuildingsViewPlugin,
            items_view::ItemsViewPlugin,
            placement_ui::PlacementUiPlugin,
            shop::PanelsPlugin,
            saves::SavesPlugin,
            editing_ui::EditingUiPlugin,
            marks_ui::MarksUiPlugin,
            wires_ui::WiresUiPlugin,
            overlays::OverlaysPlugin,
            underlays::UnderlaysPlugin,
            signal_edit::SignalEditPlugin,
        ))
        .add_systems(Startup, spawn_camera);
    }
}

/// Dossier des assets, dans cet ordre :
/// - `SHAPEZ_ASSETS`, s'il est défini ;
/// - `assets/` à côté de l'exécutable, comme dans les archives de `cargo xtask dist` ;
/// - `assets/` à la racine du workspace (rempli par `cargo xtask import-assets`).
pub fn asset_dir() -> String {
    if let Ok(dir) = std::env::var("SHAPEZ_ASSETS") {
        return dir;
    }
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("assets")))
        .filter(|dir| dir.join("sprites").is_dir());
    match beside_exe {
        Some(dir) => dir.to_string_lossy().into_owned(),
        None => concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").to_string(),
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
