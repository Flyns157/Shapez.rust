//! Rendu des bâtiments : une entité sprite par bâtiment, synchronisée à partir des
//! événements de la simulation (plan, §3.5 `BuildingRenderPlugin`).

use std::collections::HashMap;

use bevy::prelude::*;
use shapez_sim::{
    building::{Building, BuildingKind, Variant, belt_variant, dimensions, tunnel_variant},
    placement::{BuildingId, SimEvent},
};

use crate::{
    AppState,
    camera::{TILE_SIZE, tile_to_world},
    sim::{SimEventMsg, SimEventsSet},
};

/// Profondeur des convoyeurs (au-dessus des chunks et des ressources) ; les items
/// passent au-dessus, puis les autres bâtiments (ordre de `MapChunkView`).
pub const BELT_Z: f32 = 5.0;
pub const ITEM_Z: f32 = 6.0;
const BUILDING_Z: f32 = 7.0;

fn z_of(building: &Building) -> f32 {
    if building.kind == BuildingKind::Belt {
        BELT_Z
    } else {
        BUILDING_Z
    }
}

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

/// Chemin du sprite dans `assets/` (importé par `cargo xtask import-assets`).
pub fn sprite_path(kind: BuildingKind, variant: Variant, rotation_variant: u8) -> String {
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
        Variant::Default | Variant::Tier2 => return None,
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
    (
        Sprite {
            image: assets.load(sprite_path(
                building.kind,
                building.variant,
                building.rotation_variant,
            )),
            custom_size: Some(Vec2::new(w as f32, h as f32) * TILE_SIZE),
            ..default()
        },
        Transform::from_translation(center.extend(z)).with_rotation(Quat::from_rotation_z(angle)),
    )
}

fn sync_buildings(
    mut commands: Commands,
    mut events: MessageReader<SimEventMsg>,
    mut entities: ResMut<BuildingEntities>,
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
                let entity = commands.spawn((sprite, transform)).id();
                entities.0.insert(id, entity);
            }
            SimEvent::BuildingRemoved { id, .. } => {
                if let Some(entity) = entities.0.remove(&id) {
                    commands.entity(entity).despawn();
                }
            }
            SimEvent::LevelCompleted(_) => {}
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
