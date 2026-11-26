//! Les items d'un rectangle (`visible_items_in`, utilisé par le rendu) sont exactement
//! ceux de l'usine entière (`visible_items`) dont le convoyeur ou le bâtiment est dans
//! le rectangle.

use shapez_core::{Color, Item, Rotation, TilePos};
use shapez_sim::{
    SimWorld,
    building::{BuildingKind, Variant},
    factory::VisibleItem,
    placement::{Placement, SimCommand},
};

fn place(world: &mut SimWorld, kind: BuildingKind, x: i32, y: i32, rot: Rotation) {
    let tile = TilePos::new(x, y);
    let mut p = Placement::new(kind, tile, rot).with_variant(Variant::Default);
    if kind == BuildingKind::Belt {
        (p.rotation, p.rotation_variant) = world.buildings.optimal_belt(tile, rot);
    }
    world.apply(SimCommand::Place(p)).unwrap();
}

/// Un anneau de 40 convoyeurs avec des virages, et une ligne qui traverse un pivoteur.
fn world() -> SimWorld {
    let mut world = SimWorld::new(3);
    for i in 0..10 {
        place(&mut world, BuildingKind::Belt, 100 + i, 100, Rotation::R90);
        place(&mut world, BuildingKind::Belt, 110, 100 + i, Rotation::R180);
        place(&mut world, BuildingKind::Belt, 110 - i, 110, Rotation::R270);
        place(&mut world, BuildingKind::Belt, 100, 110 - i, Rotation::R0);
    }
    for x in 0..8 {
        place(&mut world, BuildingKind::Belt, 120 + x, 105, Rotation::R90);
    }
    place(&mut world, BuildingKind::Rotater, 128, 105, Rotation::R90);
    for _ in 0..600 {
        world.inject_item(TilePos::new(101, 100), Item::Color(Color::Red));
        world.inject_item(TilePos::new(120, 105), Item::Color(Color::Blue));
        world.tick();
    }
    world
}

fn key(v: &VisibleItem) -> (i64, i64, String) {
    (
        (v.x * 1000.0).round() as i64,
        (v.y * 1000.0).round() as i64,
        format!("{:?}", v.item),
    )
}

#[test]
fn rect_query_matches_the_whole_factory() {
    let world = world();
    let all = world.factory.visible_items(&world.buildings);
    assert!(all.len() > 20, "{} items seulement", all.len());

    let mut everything: Vec<_> = world
        .factory
        .visible_items_in(
            &world.buildings,
            TilePos::new(90, 90),
            TilePos::new(140, 120),
        )
        .iter()
        .map(key)
        .collect();
    let mut expected: Vec<_> = all.iter().map(key).collect();
    everything.sort();
    expected.sort();
    assert_eq!(everything, expected);

    // Moitié gauche de l'anneau : chaque item est à moins d'une tuile de son convoyeur.
    let (min, max) = (TilePos::new(100, 100), TilePos::new(104, 110));
    let part = world.factory.visible_items_in(&world.buildings, min, max);
    assert!(!part.is_empty() && part.len() < all.len());
    for v in &part {
        assert!(
            v.x >= 99.0 && v.x <= 106.0 && v.y >= 99.0 && v.y <= 112.0,
            "{v:?}"
        );
        assert!(all.iter().any(|a| key(a) == key(v)));
    }
}
