//! Point d'entrée du jeu.

use bevy::prelude::*;
use shapez_game::GamePlugin;

fn main() -> AppExit {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "shapez-rs".into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_dir(),
                    ..default()
                }),
        )
        .add_plugins(GamePlugin)
        .run()
}

/// Dossier `assets/` à la racine du workspace (rempli par `cargo xtask import-assets`).
/// `SHAPEZ_ASSETS` permet de le déplacer, par exemple pour un binaire distribué.
fn asset_dir() -> String {
    std::env::var("SHAPEZ_ASSETS")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").to_string())
}
