//! Convoyeurs dessinés sous les équilibreurs et les lecteurs de convoyeur (port de
//! `systems/belt_underlays.js`) : un morceau de convoyeur entier, ou seulement sa
//! moitié reliée à un voisin.

use std::collections::HashMap;

use bevy::prelude::*;
use shapez_core::{Direction, TilePos};
use shapez_sim::{
    building::{Building, BuildingKind, Variant, belt_variant},
    placement::{BuildingId, Buildings},
};

use crate::{
    AppState,
    buildings_view::{BELT_Z, direction_angle},
    camera::{TILE_SIZE, tile_to_world},
    sim::{Sim, SimEventMsg, SimEventsSet},
};

/// Côté pixel des sprites de convoyeur.
const SPRITE_PX: f32 = 192.0;

/// `enumClippedBeltUnderlayType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clip {
    Full,
    TopOnly,
    BottomOnly,
}

#[derive(Debug, Default)]
pub struct UnderlaysPlugin;

impl Plugin for UnderlaysPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            sync_underlays
                .after(SimEventsSet)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Sous-couches locales d'un bâtiment : `(position, direction)`.
fn underlays(building: &Building) -> Vec<(TilePos, Direction)> {
    match (building.kind, building.variant) {
        (BuildingKind::Balancer, Variant::Default) => vec![
            (TilePos::new(0, 0), Direction::Top),
            (TilePos::new(1, 0), Direction::Top),
        ],
        (BuildingKind::Balancer, _) | (BuildingKind::Reader, _) => {
            vec![(TilePos::ZERO, Direction::Top)]
        }
        _ => Vec::new(),
    }
}

/// Sortie d'un convoyeur, en coordonnées monde.
fn belt_output(belt: &Building) -> Direction {
    let local = match belt.rotation_variant {
        belt_variant::LEFT => Direction::Left,
        belt_variant::RIGHT => Direction::Right,
        _ => Direction::Top,
    };
    belt.local_direction_to_world(local)
}

/// `checkIsAcceptorConnected` : le voisin en `tile` accepte un item venant de `from`.
fn acceptor_connected(buildings: &Buildings, tile: TilePos, from: Direction) -> bool {
    let Some((_, b)) = buildings.building_at(tile) else {
        return false;
    };
    if b.kind == BuildingKind::Belt {
        return b.local_direction_to_world(Direction::Bottom) == from;
    }
    b.world_slots()
        .acceptors
        .iter()
        .any(|a| a.pos == tile && a.from == from)
}

/// `checkIsEjectorConnected` : le voisin en `tile` éjecte vers `to`.
fn ejector_connected(buildings: &Buildings, tile: TilePos, to: Direction) -> bool {
    let Some((_, b)) = buildings.building_at(tile) else {
        return false;
    };
    if b.kind == BuildingKind::Belt {
        return belt_output(b) == to;
    }
    b.world_slots()
        .ejectors
        .iter()
        .any(|e| e.pos == tile && e.to == to)
}

/// `computeBeltUnderlayType`.
fn clip(buildings: &Buildings, tile: TilePos, direction: Direction) -> Option<Clip> {
    let top = acceptor_connected(buildings, tile.step(direction), direction.inverted());
    let bottom = ejector_connected(buildings, tile.step(direction.inverted()), direction);
    match (top, bottom) {
        (true, true) => Some(Clip::Full),
        (true, false) => Some(Clip::TopOnly),
        (false, true) => Some(Clip::BottomOnly),
        (false, false) => None,
    }
}

fn sync_underlays(
    mut commands: Commands,
    mut events: MessageReader<SimEventMsg>,
    sim: Res<Sim>,
    assets: Res<AssetServer>,
    mut shown: Local<HashMap<(BuildingId, usize), Entity>>,
    mut ready: Local<bool>,
) {
    // Recalcul complet quand la carte change (`StaleAreaDetector` simplifié).
    let changed = events.read().count() > 0;
    if *ready && !changed {
        return;
    }
    *ready = true;
    for (_, entity) in shown.drain() {
        commands.entity(entity).try_despawn();
    }
    let image: Handle<Image> = assets.load("sprites/belt/built/forward_0.png");
    for (id, building) in sim.buildings.iter() {
        for (k, (pos, direction)) in underlays(building).into_iter().enumerate() {
            let tile = building.local_to_world(pos);
            let direction = building.local_direction_to_world(direction);
            let Some(clip) = clip(&sim.buildings, tile, direction) else {
                continue;
            };
            // Partie gardée du sprite (en pixels) et décalage local de son centre.
            let (rect, offset) = match clip {
                Clip::Full => (Rect::new(0.0, 0.0, SPRITE_PX, SPRITE_PX), 0.0),
                Clip::TopOnly => (Rect::new(0.0, 0.0, SPRITE_PX, SPRITE_PX / 2.0), 0.25),
                Clip::BottomOnly => (Rect::new(0.0, SPRITE_PX / 2.0, SPRITE_PX, SPRITE_PX), -0.25),
            };
            let height = rect.height() / SPRITE_PX * TILE_SIZE;
            let rotation = Quat::from_rotation_z(direction_angle(direction));
            let center = tile_to_world(tile.x as f32 + 0.5, tile.y as f32 + 0.5)
                + (rotation * Vec3::Y * offset * TILE_SIZE).truncate();
            let entity = commands
                .spawn((
                    DespawnOnExit(AppState::InGame),
                    Sprite {
                        image: image.clone(),
                        rect: Some(rect),
                        custom_size: Some(Vec2::new(TILE_SIZE, height)),
                        ..default()
                    },
                    Transform::from_translation(center.extend(BELT_Z)).with_rotation(rotation),
                ))
                .id();
            shown.insert((id, k), entity);
        }
    }
}
