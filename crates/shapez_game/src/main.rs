//! Point d'entrée du jeu.

// Pas de fenêtre de console sous Windows pour les builds de distribution.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use bevy::prelude::*;
use shapez_game::{GamePlugin, asset_dir};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> AppExit {
    app().run()
}

/// Dans le navigateur, les traductions et les polices sont téléchargées avant de
/// construire l'application, qui les lit au démarrage (voir `files.rs`).
#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_futures::spawn_local(async {
        shapez_game::files::preload(shapez_game::startup_asset_files()).await;
        app().run();
    });
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "shapez-rs".into(),
                    // Web : le canevas de `web/index.html`, à la taille de la page.
                    canvas: Some("#game".into()),
                    fit_canvas_to_parent: true,
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: asset_dir(),
                // Pas de fichiers `.meta` à côté des assets : sur le web, chaque requête
                // de `.meta` manquant finirait en 404.
                meta_check: bevy::asset::AssetMetaCheck::Never,
                ..default()
            }),
    )
    .add_plugins(GamePlugin);
    app
}
