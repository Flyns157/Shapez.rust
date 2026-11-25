//! Scénarios de la couche wires : délai des portes, orientation des câbles, réglages
//! copiés par les plans et sauvegarde des pins.

use shapez_core::{Item, Rotation, TilePos};
use shapez_sim::{
    SimWorld,
    building::{BuildingKind, Layer, Variant, wire_type},
    editing::Blueprint,
    placement::{Placement, SimCommand},
    wires::WireState,
};

fn place(world: &mut SimWorld, p: Placement) {
    world.apply(SimCommand::Place(p)).expect("pose possible");
}

fn wire(x: i32, y: i32) -> Placement {
    Placement::new(BuildingKind::Wire, TilePos::new(x, y), Rotation::R0)
}

fn wire_at(world: &SimWorld, x: i32, y: i32) -> WireState {
    let id = world
        .buildings
        .at_layer(TilePos::new(x, y), Layer::Wires)
        .expect("câble");
    world.wiring.wire_state(id)
}

/// Levier en (0, 2) → câbles (0, 1) → porte NON en (0, 0) orientée vers le haut, qui
/// lit par le bas → câble de sortie en (0, -1).
fn lever_not_circuit() -> SimWorld {
    let mut world = SimWorld::new(0);
    place(
        &mut world,
        Placement::new(BuildingKind::Lever, TilePos::new(0, 2), Rotation::R0),
    );
    place(&mut world, wire(0, 1));
    place(
        &mut world,
        Placement::new(BuildingKind::LogicGate, TilePos::new(0, 0), Rotation::R0)
            .with_variant(Variant::Not),
    );
    place(&mut world, wire(0, -1));
    world
}

fn toggle(world: &mut SimWorld, on: bool) {
    world
        .apply(SimCommand::Configure {
            tile: TilePos::new(0, 2),
            layer: Layer::Regular,
            config: on.then_some(Item::Bool(true)),
        })
        .unwrap();
}

#[test]
fn gate_output_reaches_its_network_one_tick_later() {
    let mut world = lever_not_circuit();
    let off = WireState::Value(Some(Item::Bool(false)));
    let on = WireState::Value(Some(Item::Bool(true)));

    world.tick();
    assert_eq!(wire_at(&world, 0, 1), off);
    // La porte vient d'écrire sa sortie ; son réseau la verra au tick suivant.
    assert_eq!(wire_at(&world, 0, -1), WireState::Value(None));
    world.tick();
    assert_eq!(wire_at(&world, 0, -1), on);

    toggle(&mut world, true);
    world.tick();
    assert_eq!(wire_at(&world, 0, 1), on);
    assert_eq!(wire_at(&world, 0, -1), on, "encore l'ancienne sortie");
    world.tick();
    assert_eq!(wire_at(&world, 0, -1), off);
}

#[test]
fn two_senders_with_different_values_conflict() {
    let mut world = SimWorld::new(0);
    // Deux émetteurs constants face à face, reliés par un câble.
    place(
        &mut world,
        Placement::new(
            BuildingKind::ConstantSignal,
            TilePos::new(0, 0),
            Rotation::R90,
        )
        .with_config(Some(Item::Bool(true))),
    );
    place(&mut world, wire(1, 0));
    place(
        &mut world,
        Placement::new(
            BuildingKind::ConstantSignal,
            TilePos::new(2, 0),
            Rotation::R270,
        )
        .with_config(Some(Item::Bool(false))),
    );
    world.tick();
    assert_eq!(wire_at(&world, 1, 0), WireState::Conflict);
    world
        .apply(SimCommand::Configure {
            tile: TilePos::new(2, 0),
            layer: Layer::Wires,
            config: Some(Item::Bool(true)),
        })
        .unwrap();
    world.tick();
    assert_eq!(
        wire_at(&world, 1, 0),
        WireState::Value(Some(Item::Bool(true)))
    );
}

#[test]
fn wires_take_their_shape_from_neighbours() {
    let mut world = SimWorld::new(0);
    place(&mut world, wire(0, 0));
    place(&mut world, wire(1, 0));
    place(&mut world, wire(0, 1));
    let corner = world
        .buildings
        .building_at_layer(TilePos::ZERO, Layer::Wires)
        .unwrap()
        .1;
    // Voisins à droite et en bas : un virage, rotation 0.
    assert_eq!(
        (corner.rotation, corner.rotation_variant),
        (Rotation::R0, wire_type::TURN)
    );
    // Un câble d'une autre couleur ne se raccorde pas.
    place(&mut world, wire(-1, 0).with_variant(Variant::Second));
    let corner = world
        .buildings
        .building_at_layer(TilePos::ZERO, Layer::Wires)
        .unwrap()
        .1;
    assert_eq!(corner.rotation_variant, wire_type::TURN);
    // Un câble et un bâtiment de la couche normale partagent la même tuile.
    place(
        &mut world,
        Placement::new(BuildingKind::Trash, TilePos::ZERO, Rotation::R0),
    );
    assert!(world.buildings.at(TilePos::ZERO).is_some());
    assert!(
        world
            .buildings
            .at_layer(TilePos::ZERO, Layer::Wires)
            .is_some()
    );
}

#[test]
fn blueprints_keep_lever_state_and_saves_keep_pins() {
    let mut world = lever_not_circuit();
    toggle(&mut world, true);
    world.advance(3);

    let lever = world.buildings.at(TilePos::new(0, 2)).unwrap();
    let blueprint = Blueprint::from_buildings(&world, &[lever]).unwrap();
    let copy: Vec<_> = blueprint.placements_at(TilePos::new(10, 10)).collect();
    assert_eq!(copy[0].config, Some(Item::Bool(true)));

    let json = serde_json::to_string(&world).unwrap();
    let mut loaded: SimWorld = serde_json::from_str(&json).unwrap();
    let gate = loaded
        .buildings
        .at_layer(TilePos::ZERO, Layer::Wires)
        .unwrap();
    assert_eq!(loaded.wiring.pin_value(gate, 0), Some(Item::Bool(false)));
    loaded.tick();
    world.tick();
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&world).unwrap()
    );
}
