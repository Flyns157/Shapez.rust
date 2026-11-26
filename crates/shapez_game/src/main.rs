//! Point d'entrée du jeu.

use bevy::prelude::*;
use shapez_game::{GamePlugin, asset_dir};

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
