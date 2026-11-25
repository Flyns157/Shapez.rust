//! Bâtiments reliés aux câbles : stockage, filtre, lecteur de convoyeur et peintre
//! quadruple piloté par signaux (comportements de `storage.js`, `filter.js`,
//! `belt_reader.js` et du `painterQuad` de `item_processor.js`).

use shapez_core::{Color, Item, Rotation, ShapeDef, TilePos};
use shapez_sim::{
    SimWorld, TICKS_PER_SECOND,
    building::{BuildingKind, Layer, Variant},
    placement::{BuildingId, Placement, SimCommand},
};

fn place(world: &mut SimWorld, p: Placement) -> BuildingId {
    let p = if p.kind == BuildingKind::Belt {
        let (rotation, variant) = world.buildings.optimal_belt(p.origin, p.rotation);
        Placement {
            rotation,
            rotation_variant: variant,
            ..p
        }
    } else {
        p
    };
    world
        .apply(SimCommand::Place(p))
        .expect("pose possible")
        .expect("bâtiment posé")
}

fn at(kind: BuildingKind, x: i32, y: i32, rot: Rotation) -> Placement {
    Placement::new(kind, TilePos::new(x, y), rot)
}

/// Émetteur constant de la couche wires.
fn signal(world: &mut SimWorld, x: i32, y: i32, rot: Rotation, value: Option<Item>) {
    place(
        world,
        at(BuildingKind::ConstantSignal, x, y, rot).with_config(value),
    );
}

fn set_signal(world: &mut SimWorld, x: i32, y: i32, value: Option<Item>) {
    world
        .apply(SimCommand::Configure {
            tile: TilePos::new(x, y),
            layer: Layer::Wires,
            config: value,
        })
        .unwrap();
}

/// Items présents sur le chemin du convoyeur posé en (x, y).
fn items_on(world: &SimWorld, x: i32, y: i32) -> Vec<Item> {
    let id = world.buildings.at(TilePos::new(x, y)).expect("convoyeur");
    world
        .factory
        .path_of(id)
        .expect("chemin")
        .items_with_progress()
        .map(|(_, item)| item)
        .collect()
}

const RED: Item = Item::Color(Color::Red);
const GREEN: Item = Item::Color(Color::Green);

#[test]
fn storage_keeps_one_kind_of_item_and_reports_it_on_its_pins() {
    let mut world = SimWorld::new(0);
    // Stockage 2×2 en (0, 0), alimenté par le bas en (0, 2) ; rien ne sort en haut.
    let storage = place(&mut world, at(BuildingKind::Storage, 0, 0, Rotation::R0));
    place(&mut world, at(BuildingKind::Belt, 0, 2, Rotation::R0));
    for _ in 0..4 {
        world.inject_item(TilePos::new(0, 2), RED);
        world.advance(TICKS_PER_SECOND);
    }
    // Deux items attendent dans les slots de sortie, les deux autres sont stockés.
    assert_eq!(world.factory.storage(storage).unwrap().count, 2);
    assert_eq!(world.wiring.pin_value(storage, 0), Some(RED));
    assert_eq!(world.wiring.pin_value(storage, 1), Some(Item::Bool(false)));

    // Une autre couleur est refusée et reste sur le convoyeur.
    world.inject_item(TilePos::new(0, 2), GREEN);
    world.advance(2 * TICKS_PER_SECOND);
    assert_eq!(items_on(&world, 0, 2), [GREEN]);
    assert_eq!(world.factory.storage(storage).unwrap().count, 2);
}

/// Filtre en (0, 0) alimenté par le bas, sortie « passe » en haut vers (0, -1),
/// sortie « rejet » à droite vers (2, 0) ; son pin lit l'émetteur posé à gauche.
fn filter_world(value: Option<Item>) -> SimWorld {
    let mut world = SimWorld::new(0);
    place(&mut world, at(BuildingKind::Filter, 0, 0, Rotation::R0));
    place(&mut world, at(BuildingKind::Belt, 0, 1, Rotation::R0));
    for y in [-1, -2, -3] {
        place(&mut world, at(BuildingKind::Belt, 0, y, Rotation::R0));
    }
    for x in [2, 3, 4] {
        place(&mut world, at(BuildingKind::Belt, x, 0, Rotation::R90));
    }
    signal(&mut world, -1, 0, Rotation::R90, value);
    world
}

#[test]
fn filter_lets_matching_items_through_and_rejects_the_others() {
    let mut world = filter_world(Some(RED));
    for item in [RED, GREEN, RED] {
        world.inject_item(TilePos::new(0, 1), item);
        world.advance(TICKS_PER_SECOND);
    }
    world.advance(3 * TICKS_PER_SECOND);
    assert_eq!(items_on(&world, 0, -1), [RED, RED]);
    assert_eq!(items_on(&world, 2, 0), [GREEN]);
}

#[test]
fn filter_with_a_true_signal_passes_everything_and_without_signal_blocks() {
    let mut world = filter_world(Some(Item::Bool(true)));
    world.inject_item(TilePos::new(0, 1), GREEN);
    world.advance(3 * TICKS_PER_SECOND);
    assert_eq!(items_on(&world, 0, -1), [GREEN]);

    set_signal(&mut world, -1, 0, None);
    world.inject_item(TilePos::new(0, 1), RED);
    world.advance(3 * TICKS_PER_SECOND);
    assert_eq!(
        items_on(&world, 0, 1),
        [RED],
        "le filtre sans signal refuse tout"
    );
}

#[test]
fn belt_reader_reports_the_last_item_and_recent_traffic() {
    let mut world = SimWorld::new(0);
    let reader = place(&mut world, at(BuildingKind::Reader, 0, 0, Rotation::R0));
    place(&mut world, at(BuildingKind::Belt, 0, 1, Rotation::R0));
    for y in [-1, -2, -3] {
        place(&mut world, at(BuildingKind::Belt, 0, y, Rotation::R0));
    }
    world.advance(2 * TICKS_PER_SECOND);
    assert_eq!(world.wiring.pin_value(reader, 0), Some(Item::Bool(false)));
    assert_eq!(world.wiring.pin_value(reader, 1), None);

    world.inject_item(TilePos::new(0, 1), GREEN);
    world.advance(TICKS_PER_SECOND);
    assert_eq!(world.wiring.pin_value(reader, 0), Some(Item::Bool(true)));
    assert_eq!(world.wiring.pin_value(reader, 1), Some(GREEN));
    world.advance(TICKS_PER_SECOND);
    assert_eq!(items_on(&world, 0, -1), [GREEN], "l'item continue sa route");

    world.advance(TICKS_PER_SECOND);
    assert_eq!(world.wiring.pin_value(reader, 0), Some(Item::Bool(false)));
    assert_eq!(world.wiring.pin_value(reader, 1), Some(GREEN));
}

#[test]
fn quad_painter_only_paints_quarters_with_a_true_signal() {
    let mut world = SimWorld::new(0);
    // Peintre quadruple en (0..3, 0) : forme par la gauche, couleurs par le bas,
    // signaux sous chaque quart (couche wires), sortie en haut de (0, 0).
    place(
        &mut world,
        at(BuildingKind::Painter, 0, 0, Rotation::R0).with_variant(Variant::Quad),
    );
    place(&mut world, at(BuildingKind::Belt, -1, 0, Rotation::R90));
    for x in 0..4 {
        place(&mut world, at(BuildingKind::Belt, x, 1, Rotation::R0));
    }
    for y in [-1, -2, -3] {
        place(&mut world, at(BuildingKind::Belt, 0, y, Rotation::R0));
    }
    signal(&mut world, 0, 1, Rotation::R0, Some(Item::Bool(true)));
    signal(&mut world, 1, 1, Rotation::R0, Some(Item::Bool(false)));
    signal(&mut world, 2, 1, Rotation::R0, Some(Item::Bool(true)));

    let circle = |world: &mut SimWorld, key: &str| {
        Item::Shape(
            world
                .registry
                .intern(ShapeDef::from_short_key(key).unwrap()),
        )
    };
    let shape = circle(&mut world, "CuCuCuCu");
    world.inject_item(TilePos::new(-1, 0), shape);
    world.inject_item(TilePos::new(0, 1), RED);
    world.inject_item(TilePos::new(1, 1), RED);
    world.advance(TICKS_PER_SECOND);
    // Le quart 2 est activé mais n'a pas de couleur : rien ne sort.
    assert!(items_on(&world, 0, -1).is_empty());
    assert_eq!(
        items_on(&world, 1, 1),
        [RED],
        "quart 1 désactivé : couleur refusée"
    );

    world.inject_item(TilePos::new(2, 1), GREEN);
    world.advance(3 * TICKS_PER_SECOND);
    let painted = circle(&mut world, "CrCuCgCu");
    assert_eq!(items_on(&world, 0, -1), [painted]);
}

#[test]
fn clearing_items_empties_belts_and_buildings_but_not_storage() {
    let mut world = filter_world(Some(RED));
    let storage = place(&mut world, at(BuildingKind::Storage, 6, 0, Rotation::R0));
    place(&mut world, at(BuildingKind::Belt, 6, 2, Rotation::R0));
    for _ in 0..4 {
        world.inject_item(TilePos::new(6, 2), GREEN);
        for item in [RED, GREEN] {
            world.inject_item(TilePos::new(0, 1), item);
        }
        world.advance(TICKS_PER_SECOND / 2);
    }
    world.inject_item(TilePos::new(0, 1), RED);
    world.inject_item(TilePos::new(6, 2), GREEN);
    let stored = world.factory.storage(storage).unwrap().count as usize;
    assert!(world.factory.item_count() > stored + 2);

    let all: Vec<_> = world.buildings.iter().map(|(id, _)| id).collect();
    world.clear_items(&all);
    assert_eq!(world.factory.item_count(), stored);
    assert_eq!(
        world.factory.storage(storage).unwrap().count as usize,
        stored
    );
    // L'usine repart normalement.
    world.inject_item(TilePos::new(0, 1), RED);
    world.advance(3 * TICKS_PER_SECOND);
    assert_eq!(items_on(&world, 0, -1), [RED]);
}
