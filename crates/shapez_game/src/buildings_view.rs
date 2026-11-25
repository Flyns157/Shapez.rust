//! Rendu des bâtiments : une entité sprite par bâtiment, synchronisée à partir des
//! événements de la simulation (plan, §3.5 `BuildingRenderPlugin`).

use std::collections::HashMap;

use bevy::prelude::*;
use shapez_core::{Direction, Item};
use shapez_sim::{
    building::{
        Building, BuildingKind, Layer, PinKind, Variant, belt_variant, dimensions, pins,
        tunnel_variant, wire_type,
    },
    placement::{BuildingId, SimEvent},
};

use crate::{
    AppState,
    camera::{TILE_SIZE, tile_to_world},
    sim::{SimEventMsg, SimEventsSet},
    wires_ui::{ActiveLayer, WiresLayer},
};

/// Profondeur des convoyeurs (au-dessus des chunks et des ressources) ; les items
/// passent au-dessus, puis les autres bâtiments (ordre de `MapChunkView`).
pub const BELT_Z: f32 = 5.0;
pub const ITEM_Z: f32 = 6.0;
const BUILDING_Z: f32 = 7.0;
/// Voile sombre de la couche wires, puis câbles et bâtiments de cette couche.
pub const WIRES_OVERLAY_Z: f32 = 10.0;
pub const WIRES_Z: f32 = 12.0;
const PIN_Z: f32 = 0.5;

fn z_of(building: &Building) -> f32 {
    if building.kind.layer() == Layer::Wires {
        WIRES_Z
    } else if building.kind == BuildingKind::Belt {
        BELT_Z
    } else {
        BUILDING_Z
    }
}

/// Bâtiment de la simulation représenté par cette entité.
#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingView(pub BuildingId);

/// Correspondance simulation → entités de rendu.
#[derive(Resource, Debug, Default)]
pub struct BuildingEntities(pub HashMap<BuildingId, Entity>);

#[derive(Debug, Default)]
pub struct BuildingsViewPlugin;

impl Plugin for BuildingsViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingEntities>()
            .add_systems(OnExit(AppState::InGame), despawn_all)
            .add_systems(
                Update,
                sync_buildings
                    .after(SimEventsSet)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Sprite d'un câble selon sa couleur, sa forme et l'état de son réseau
/// (`wireSprites` : `first`, `second` ou `conflict`).
pub fn wire_sprite_path(variant: Variant, shape: u8, conflict: bool) -> String {
    let set = if conflict {
        "conflict"
    } else if variant == Variant::Second {
        "second"
    } else {
        "first"
    };
    let shape = match shape {
        wire_type::TURN => "turn",
        wire_type::SPLIT => "split",
        wire_type::CROSS => "cross",
        _ => "forward",
    };
    format!("sprites/wires/sets/{set}_{shape}.png")
}

/// Chemin du sprite dans `assets/` (importé par `cargo xtask import-assets`).
pub fn sprite_path(kind: BuildingKind, variant: Variant, rotation_variant: u8) -> String {
    if kind == BuildingKind::Wire {
        return wire_sprite_path(variant, rotation_variant, false);
    }
    let name = match kind {
        BuildingKind::Belt => match rotation_variant {
            belt_variant::LEFT => "belt_left".to_string(),
            belt_variant::RIGHT => "belt_right".to_string(),
            _ => "belt_top".to_string(),
        },
        BuildingKind::UndergroundBelt => {
            let base = if rotation_variant == tunnel_variant::RECEIVER {
                "underground_belt_exit"
            } else {
                "underground_belt_entry"
            };
            if variant == Variant::Tier2 {
                format!("{base}-tier2")
            } else {
                base.to_string()
            }
        }
        _ => match variant_suffix(variant) {
            Some(suffix) => format!("{}-{suffix}", kind.js_id()),
            None => kind.js_id().to_string(),
        },
    };
    format!("sprites/buildings/{name}.png")
}

fn variant_suffix(variant: Variant) -> Option<&'static str> {
    Some(match variant {
        Variant::Default | Variant::Tier2 | Variant::Second => return None,
        Variant::Or => "or",
        Variant::Not => "not",
        Variant::Xor => "xor",
        Variant::Merger => "merger",
        Variant::MergerInverse => "merger-inverse",
        Variant::Splitter => "splitter",
        Variant::SplitterInverse => "splitter-inverse",
        Variant::Quad => "quad",
        Variant::Ccw => "ccw",
        Variant::Rotate180 => "rotate180",
        Variant::Mirrored => "mirrored",
        Variant::Double => "double",
        Variant::Chainable => "chainable",
    })
}

/// Sprite, taille et transformation d'un bâtiment.
pub fn building_sprite(building: &Building, assets: &AssetServer, z: f32) -> (Sprite, Transform) {
    let (w, h) = dimensions(building.kind, building.variant);
    let (min, (bw, bh)) = building.bounds();
    let center = tile_to_world(
        min.x as f32 + bw as f32 / 2.0,
        min.y as f32 + bh as f32 / 2.0,
    );
    let angle = -(f32::from(building.rotation.degrees())).to_radians();
    let path = if building.kind == BuildingKind::Lever && building.config == Some(Item::Bool(true))
    {
        "sprites/wires/lever_on.png".to_string()
    } else {
        sprite_path(building.kind, building.variant, building.rotation_variant)
    };
    (
        Sprite {
            image: assets.load(path),
            custom_size: Some(Vec2::new(w as f32, h as f32) * TILE_SIZE),
            ..default()
        },
        Transform::from_translation(center.extend(z)).with_rotation(Quat::from_rotation_z(angle)),
    )
}

/// Angle de rendu d'une direction (sens horaire depuis le haut).
pub fn direction_angle(direction: Direction) -> f32 {
    -(f32::from(direction.to_rotation().degrees())).to_radians()
}

/// Pins dessinés par le bâtiment lui-même (`getRenderPins`) : seuls ceux du levier
/// parmi les bâtiments actuels, les portes les ont dans leur sprite.
fn renders_pins(kind: BuildingKind) -> bool {
    matches!(kind, BuildingKind::Lever)
}

fn sync_buildings(
    mut commands: Commands,
    mut events: MessageReader<SimEventMsg>,
    mut entities: ResMut<BuildingEntities>,
    layer: Res<ActiveLayer>,
    assets: Res<AssetServer>,
) {
    for SimEventMsg(event) in events.read() {
        match *event {
            SimEvent::BuildingPlaced { id, building }
            | SimEvent::BuildingChanged { id, building } => {
                if let Some(old) = entities.0.remove(&id) {
                    commands.entity(old).despawn();
                }
                let (sprite, transform) = building_sprite(&building, &assets, z_of(&building));
                let mut entity = commands.spawn((BuildingView(id), sprite, transform));
                if building.kind.layer() == Layer::Wires {
                    entity.insert((WiresLayer, layer.visibility()));
                }
                if renders_pins(building.kind) {
                    let (w, h) = dimensions(building.kind, building.variant);
                    entity.with_children(|parent| {
                        for pin in pins(building.kind, building.variant) {
                            // Position locale (bâtiment non tourné), Y vers le haut.
                            let offset = Vec2::new(
                                pin.pos.x as f32 + 0.5 - w as f32 / 2.0,
                                -(pin.pos.y as f32 + 0.5 - h as f32 / 2.0),
                            ) * TILE_SIZE;
                            let image = match pin.kind {
                                PinKind::Ejector => "sprites/wires/logical_ejector.png",
                                PinKind::Acceptor => "sprites/wires/logical_acceptor.png",
                            };
                            parent.spawn((
                                WiresLayer,
                                layer.visibility(),
                                Sprite {
                                    image: assets.load(image),
                                    custom_size: Some(Vec2::splat(TILE_SIZE + 2.0)),
                                    ..default()
                                },
                                Transform::from_translation(
                                    offset.extend(WIRES_Z + PIN_Z - z_of(&building)),
                                )
                                .with_rotation(
                                    Quat::from_rotation_z(direction_angle(pin.direction)),
                                ),
                            ));
                        }
                    });
                }
                entities.0.insert(id, entity.id());
            }
            SimEvent::BuildingRemoved { id, .. } => {
                if let Some(entity) = entities.0.remove(&id) {
                    commands.entity(entity).despawn();
                }
            }
            SimEvent::LevelCompleted(_) | SimEvent::UpgradePurchased { .. } => {}
        }
    }
}

fn despawn_all(mut commands: Commands, mut entities: ResMut<BuildingEntities>) {
    for (_, entity) in entities.0.drain() {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_names_match_res_raw() {
        assert_eq!(
            sprite_path(BuildingKind::Cutter, Variant::Quad, 0),
            "sprites/buildings/cutter-quad.png"
        );
        assert_eq!(
            sprite_path(BuildingKind::Belt, Variant::Default, belt_variant::LEFT),
            "sprites/buildings/belt_left.png"
        );
        assert_eq!(
            sprite_path(
                BuildingKind::UndergroundBelt,
                Variant::Tier2,
                tunnel_variant::RECEIVER
            ),
            "sprites/buildings/underground_belt_exit-tier2.png"
        );
        assert_eq!(
            sprite_path(BuildingKind::Balancer, Variant::SplitterInverse, 0),
            "sprites/buildings/balancer-splitter-inverse.png"
        );
    }
}
