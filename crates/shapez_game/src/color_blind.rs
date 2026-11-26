//! Aide daltonien (port de `hud/parts/color_blind_helper.js`) : nom de la couleur sous
//! le curseur (item d'un convoyeur, item en sortie d'un bâtiment, ou gisement) et
//! contour de la tuile.

use bevy::prelude::*;
use shapez_core::{Color as ShapeColor, Item, TilePos};
use shapez_sim::{
    building::{BuildingKind, Layer},
    map::MapResource,
};

use crate::{
    AppState,
    camera::{TILE_SIZE, tile_to_world},
    i18n::t,
    placement_ui::PlacementState,
    settings::Settings,
    sim::Sim,
    theme::{ThemedText, active},
    wires_ui::ActiveLayer,
};

#[derive(Debug, Default)]
pub struct ColorBlindPlugin;

impl Plugin for ColorBlindPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_label)
            .add_systems(Update, show_color_below.run_if(in_state(AppState::InGame)));
    }
}

#[derive(Component, Debug)]
struct ColorLabel;

/// `computeColorBelowTile`.
pub fn color_below(sim: &mut Sim, tile: TilePos) -> Option<ShapeColor> {
    let as_color = |item: Item| match item {
        Item::Color(c) => Some(c),
        _ => None,
    };
    match sim.buildings.building_at_layer(tile, Layer::Regular) {
        Some((id, building)) if building.kind != BuildingKind::Miner => {
            // Item du convoyeur sur cette tuile.
            if building.kind == BuildingKind::Belt
                && let Some(path) = sim.factory.path_of(id)
            {
                let mut start = 0;
                for belt in path.belts() {
                    if belt.id == id {
                        let end = start + belt.length;
                        return path
                            .items_with_progress()
                            .find(|&(p, _)| p >= start && p <= end)
                            .and_then(|(_, item)| as_color(item));
                    }
                    start += belt.length;
                }
                return None;
            }
            // Item en attente de sortie.
            sim.factory
                .ejector(id)
                .and_then(|e| e.slots.iter().find_map(|s| s.item.and_then(as_color)))
        }
        _ => match sim.map.resource_at(tile)? {
            MapResource::Color(c) => Some(c),
            MapResource::Shape(_) => None,
        },
    }
}

fn spawn_label(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        ColorLabel,
        ThemedText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            ..default()
        },
    ));
}

fn show_color_below(
    settings: Res<Settings>,
    layer: Res<ActiveLayer>,
    placement: Res<PlacementState>,
    mut sim: ResMut<Sim>,
    window: Single<&Window>,
    label: Single<(&mut Text, &mut Node), With<ColorLabel>>,
    mut gizmos: Gizmos,
) {
    let (mut text, mut node) = label.into_inner();
    let below = match (settings.color_blind, layer.0, placement.cursor_tile) {
        (true, Layer::Regular, Some(tile)) => {
            // Lecture seule : la carte ne génère au pire qu'un chunk manquant.
            color_below(sim.bypass_change_detection(), tile).map(|c| (tile, c))
        }
        _ => None,
    };
    let Some((tile, color)) = below else {
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };
    let name = t(&format!("ingame.colors.{}", color.js_name()));
    if text.0 != name {
        text.0 = name;
    }
    // Sous le curseur, comme `ingame_HUD_ColorBlindBelowTileHelper`.
    if let Some(cursor) = window.cursor_position() {
        node.left = px(cursor.x + 16.0);
        node.top = px(cursor.y + 16.0);
    }
    node.display = Display::Flex;
    gizmos.rect_2d(
        Isometry2d::from_translation(tile_to_world(tile.x as f32 + 0.5, tile.y as f32 + 0.5)),
        Vec2::splat(TILE_SIZE),
        active(&settings).color_blind_tile,
    );
}
