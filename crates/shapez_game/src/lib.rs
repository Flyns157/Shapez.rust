//! Couche Bevy du jeu. La logique vit dans `shapez_sim` ; ce crate ne fait
//! qu'afficher, lire les entrées et piloter le cycle de vie (plan, §3.1 et §3.5).

use bevy::prelude::*;

pub mod buildings_view;
pub mod camera;
pub mod editing_ui;
pub mod gallery;
pub mod items_view;
pub mod main_menu;
pub mod map_render;
pub mod marks_ui;
pub mod menu;
pub mod placement_ui;
pub mod saves;
pub mod shape_textures;
pub mod shop;
pub mod sim;
pub mod state;
pub mod wires_ui;

pub use state::AppState;

/// Regroupe tous les plugins du jeu.
#[derive(Debug, Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
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
        ))
        .add_systems(Startup, spawn_camera);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
