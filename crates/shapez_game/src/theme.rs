//! Thèmes clair et sombre (`game/themes/*.json`, `applyGameTheme`) : couleurs de la
//! carte en partie et du texte du HUD posé directement sur la carte. Les menus gardent
//! leur fond sombre quel que soit le thème.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use shapez_core::Color as ShapeColor;
use shapez_sim::map::MapResource;

use crate::{AppState, settings::Settings};

/// Thème choisi dans les paramètres (`THEMES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeId {
    /// Le jeu d'origine démarre en clair ; le portage garde le sombre, pour lequel son
    /// HUD a été conçu.
    #[default]
    Dark,
    Light,
}

impl ThemeId {
    pub const ALL: [Self; 2] = [Self::Dark, Self::Light];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn colors(self) -> &'static Theme {
        match self {
            Self::Dark => &DARK,
            Self::Light => &LIGHT,
        }
    }
}

/// Les couleurs reprises du thème JS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// `map.background`.
    pub background: Color,
    /// `map.gridRegular`.
    pub grid: Color,
    /// `map.resources` : forme, rouge, vert, bleu.
    pub resources: [[u8; 3]; 4],
    /// `map.chunkOverview.empty`.
    pub overview_empty: [u8; 3],
    /// `map.colorBlindPickerTile`.
    pub color_blind_tile: Color,
    /// Texte du HUD sur la carte (le CSS du JS : clair en sombre, foncé en clair).
    pub hud_text: Color,
}

impl Theme {
    pub fn resource_color(&self, resource: MapResource) -> [u8; 3] {
        let [shape, red, green, blue] = self.resources;
        match resource {
            MapResource::Shape(_) => shape,
            MapResource::Color(ShapeColor::Red) => red,
            MapResource::Color(ShapeColor::Green) => green,
            MapResource::Color(_) => blue,
        }
    }
}

pub const DARK: Theme = Theme {
    background: Color::srgb_u8(0x3e, 0x3f, 0x47),
    grid: Color::srgba(1.0, 1.0, 1.0, 0.02),
    resources: [
        [0x5d, 0x5f, 0x6a],
        [0x85, 0x4f, 0x56],
        [0x66, 0x79, 0x64],
        [0x5e, 0x7c, 0xa4],
    ],
    overview_empty: [0x44, 0x48, 0x56],
    color_blind_tile: Color::srgba(1.0, 1.0, 1.0, 0.5),
    hud_text: Color::WHITE,
};

pub const LIGHT: Theme = Theme {
    background: Color::srgb_u8(0xec, 0xee, 0xf2),
    grid: Color::srgb_u8(0xe3, 0xe7, 0xea),
    resources: [
        [0xe0, 0xe2, 0xe8],
        [0xf3, 0xbc, 0xb6],
        [0xcf, 0xef, 0xa0],
        [0xb2, 0xe0, 0xfa],
    ],
    overview_empty: [0xa6, 0xaf, 0xbb],
    color_blind_tile: Color::srgba(50.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0, 0.4),
    hud_text: Color::srgb_u8(0x39, 0x3c, 0x45),
};

/// Thème actif ; change avec les paramètres.
pub fn active(settings: &Settings) -> &'static Theme {
    settings.theme.colors()
}

/// Texte posé sur la carte, dont la couleur suit le thème.
#[derive(Component, Debug, Default)]
pub struct ThemedText;

#[derive(Debug, Default)]
pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(DARK.background))
            .add_systems(Update, (clear_color, themed_text));
    }
}

/// Fond de la carte en partie, fond sombre ailleurs.
fn clear_color(
    settings: Res<Settings>,
    state: Res<State<AppState>>,
    mut clear: ResMut<ClearColor>,
) {
    let wanted = match state.get() {
        AppState::InGame => active(&settings).background,
        _ => DARK.background,
    };
    if clear.0 != wanted {
        clear.0 = wanted;
    }
}

fn themed_text(settings: Res<Settings>, mut texts: Query<(Ref<ThemedText>, &mut TextColor)>) {
    let color = active(&settings).hud_text;
    for (marker, mut text) in &mut texts {
        if (marker.is_added() || settings.is_changed()) && text.0 != color {
            text.0 = color;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_ids_round_trip_like_the_js_keys() {
        for theme in ThemeId::ALL {
            let json = serde_json::to_string(&theme).unwrap();
            assert_eq!(json, format!("\"{}\"", theme.id()));
            assert_eq!(serde_json::from_str::<ThemeId>(&json).unwrap(), theme);
        }
    }
}
