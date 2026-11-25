//! Annulation, sélection de masse et plans.

use shapez_core::{Rotation, ShapeDef, TilePos};
use shapez_sim::{
    SimWorld,
    building::{BuildingKind, Variant},
    editing::{BLUEPRINT_SHAPE, Blueprint, History, blueprint_cost},
    placement::{CommandError, Placement, SimCommand},
};

fn placement(kind: BuildingKind, x: i32, y: i32, rot: Rotation) -> SimCommand {
    SimCommand::Place(Placement::new(kind, TilePos::new(x, y), rot).with_variant(Variant::Default))
}

/// Bâtiments du monde, sous une forme comparable.
fn layout(world: &SimWorld) -> Vec<String> {
    let mut out: Vec<String> = world
        .buildings
        .iter()
        .map(|(_, b)| format!("{b:?}"))
        .collect();
    out.sort();
    out
}

fn line(world: &mut SimWorld, history: &mut History) {
    let commands = (5..10).map(|x| placement(BuildingKind::Belt, x, 5, Rotation::R90));
    let tx = world.apply_all(commands);
    assert_eq!(tx.placed.len(), 5);
    history.record(tx);
}

#[test]
fn undo_and_redo_a_placement() {
    let mut world = SimWorld::new_game(1);
    let mut history = History::default();
    let empty = layout(&world);
    line(&mut world, &mut history);
    let built = layout(&world);
    assert!(history.undo(&mut world));
    assert_eq!(layout(&world), empty);
    assert!(history.redo(&mut world));
    assert_eq!(layout(&world), built);
    assert!(!history.redo(&mut world));
    assert!(history.undo(&mut world));
    assert!(!history.undo(&mut world));
    assert_eq!(layout(&world), empty);
}

#[test]
fn undoing_a_building_restores_the_belts_it_replaced() {
    let mut world = SimWorld::new_game(1);
    let mut history = History::default();
    line(&mut world, &mut history);
    let belts = layout(&world);
    let tx = world.apply_all([placement(BuildingKind::Trash, 7, 5, Rotation::R0)]);
    assert_eq!((tx.placed.len(), tx.removed.len()), (1, 1));
    history.record(tx);
    history.undo(&mut world);
    assert_eq!(layout(&world), belts);
}

#[test]
fn mass_delete_and_undo() {
    let mut world = SimWorld::new_game(1);
    let mut history = History::default();
    line(&mut world, &mut history);
    world.apply_all([placement(BuildingKind::Cutter, 6, 7, Rotation::R0)]);
    let before = layout(&world);
    // Le rectangle touche aussi le hub, qui n'est jamais sélectionné.
    let selection = world.buildings_in_rect(TilePos::new(10, 8), TilePos::new(-1, 5));
    assert_eq!(selection.len(), 6);
    let tx = world.remove_all(&selection);
    assert_eq!(tx.removed.len(), 6);
    assert_eq!(world.buildings.len(), 1);
    history.record(tx);
    history.undo(&mut world);
    assert_eq!(layout(&world), before);
}

#[test]
fn blueprint_cost_follows_the_js_formula() {
    let mut world = SimWorld::new_game(1);
    let mut history = History::default();
    line(&mut world, &mut history);
    let one = world.buildings_in_rect(TilePos::new(5, 5), TilePos::new(5, 5));
    assert_eq!(Blueprint::from_buildings(&world, &one).unwrap().cost(), 4);
    let five = world.buildings_in_rect(TilePos::new(5, 5), TilePos::new(9, 5));
    assert_eq!(Blueprint::from_buildings(&world, &five).unwrap().cost(), 20);
    // Valeurs du JS (`findNiceIntegerValue(4 * Math.pow(n, 1.1))`).
    for (n, cost) in [
        (1, 4),
        (2, 8),
        (3, 13),
        (5, 20),
        (10, 50),
        (37, 200),
        (100, 620),
        (1000, 7000),
    ] {
        assert_eq!(blueprint_cost(n), cost, "{n} bâtiments");
    }
}

#[test]
fn paste_a_rotated_blueprint() {
    let mut world = SimWorld::new_game(1);
    let mut history = History::default();
    line(&mut world, &mut history);
    let ids = world.buildings_in_rect(TilePos::new(5, 5), TilePos::new(9, 5));
    let mut blueprint = Blueprint::from_buildings(&world, &ids).unwrap();
    // Ancrage au centre moyen : (7, 5), donc des origines de -2 à 2.
    let origins: Vec<i32> = blueprint
        .placements_at(TilePos::new(0, 0))
        .map(|p| p.origin.x)
        .collect();
    assert_eq!(origins, [-2, -1, 0, 1, 2]);

    assert_eq!(
        world.place_blueprint(&blueprint, TilePos::new(20, 20)),
        Err(CommandError::CannotAfford)
    );
    let shape = world
        .registry
        .intern(ShapeDef::from_short_key(BLUEPRINT_SHAPE).unwrap());
    world.hub.add_stored(shape, 30);

    blueprint.rotate_cw();
    let tx = world
        .place_blueprint(&blueprint, TilePos::new(20, 20))
        .unwrap();
    assert_eq!(tx.placed.len(), 5);
    assert_eq!(world.hub.stored(shape), 10);
    // Ligne verticale de x = 20, y = 18..22, tournée vers le bas.
    for y in 18..=22 {
        let (_, b) = world.buildings.building_at(TilePos::new(20, y)).unwrap();
        assert_eq!(b.rotation, Rotation::R180);
    }
    // Recoller au même endroit : tout est bloqué par des convoyeurs remplaçables,
    // donc tout se repose ; un bâtiment non remplaçable, lui, bloque.
    world.apply_all([placement(BuildingKind::Trash, 30, 30, Rotation::R0)]);
    let single = world.buildings_in_rect(TilePos::new(30, 30), TilePos::new(30, 30));
    let trash = Blueprint::from_buildings(&world, &single).unwrap();
    assert!(!trash.can_place_any(&world, TilePos::new(30, 30)));
    assert!(matches!(
        world.place_blueprint(&trash, TilePos::new(30, 30)),
        Err(CommandError::Blocked { .. })
    ));
}

#[test]
fn merged_steps_undo_as_one_action() {
    let mut world = SimWorld::new_game(1);
    let mut history = History::default();
    let empty = layout(&world);
    // Un glisser : chaque tuile est une étape, et la précédente est reposée tournée.
    let mut tx = world.apply_all([placement(BuildingKind::Belt, 5, 5, Rotation::R90)]);
    tx.merge(world.apply_all([
        placement(BuildingKind::Belt, 5, 5, Rotation::R180),
        placement(BuildingKind::Belt, 5, 6, Rotation::R180),
    ]));
    assert_eq!((tx.placed.len(), tx.removed.len()), (2, 0));
    history.record(tx);
    history.undo(&mut world);
    assert_eq!(layout(&world), empty);
}
