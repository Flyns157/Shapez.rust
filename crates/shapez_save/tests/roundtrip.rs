//! Une partie sauvegardée puis rechargée continue exactement comme l'originale
//! (plan, §9.1 : « hash d'état identique après un aller-retour de sauvegarde »).

use shapez_core::{Rotation, TilePos};
use shapez_save::{SaveMeta, decode, encode};
use shapez_sim::{
    SimWorld, TICKS_PER_SECOND,
    building::{BuildingKind, Variant},
    map::MapResource,
    placement::{Placement, SimCommand, SimEvent},
    upgrades::UpgradeKind,
};

const SECOND: u32 = TICKS_PER_SECOND;

fn place(world: &mut SimWorld, kind: BuildingKind, x: i32, y: i32, rot: Rotation) {
    let tile = TilePos::new(x, y);
    let mut p = Placement::new(kind, tile, rot).with_variant(Variant::Default);
    if kind == BuildingKind::Belt {
        (p.rotation, p.rotation_variant) = world.buildings.optimal_belt(tile, rot);
    }
    world.apply(SimCommand::Place(p)).unwrap();
}

/// Graine et rangée avec un gisement de cercles à droite du hub.
fn circle_row() -> (u32, i32, i32) {
    for seed in 0..500 {
        let mut world = SimWorld::new(seed);
        for y in -2..=1 {
            for x in 8..=30 {
                if matches!(world.map.resource_at(TilePos::new(x, y)),
                    Some(MapResource::Shape(s)) if s.short_key() == "CuCuCuCu")
                {
                    return (seed, x, y);
                }
            }
        }
    }
    panic!("aucune graine ne convient");
}

/// Extracteur → convoyeurs → découpeur → hub, poubelle sur la 2e sortie ; un convoyeur
/// supprimé puis reposé laisse un trou dans l'arène, et un palier est acheté.
fn busy_world() -> SimWorld {
    let (seed, x, y) = circle_row();
    let mut world = SimWorld::new_game(seed);
    place(&mut world, BuildingKind::Miner, x, y, Rotation::R270);
    let cx = 5;
    for bx in (cx + 1..x).rev() {
        place(&mut world, BuildingKind::Belt, bx, y, Rotation::R270);
    }
    place(&mut world, BuildingKind::Cutter, cx, y, Rotation::R270);
    for bx in (2..cx).rev() {
        place(&mut world, BuildingKind::Belt, bx, y, Rotation::R270);
    }
    place(&mut world, BuildingKind::Trash, cx - 1, y - 1, Rotation::R0);
    world.advance(40 * SECOND);
    world
        .apply(SimCommand::Remove {
            tile: TilePos::new(cx + 2, y),
            layer: shapez_sim::building::Layer::Regular,
        })
        .unwrap();
    world.advance(SECOND);
    place(&mut world, BuildingKind::Belt, cx + 2, y, Rotation::R270);
    let rect = world
        .registry
        .intern(shapez_core::ShapeDef::from_short_key("RuRuRuRu").unwrap());
    world.hub.add_stored(rect, 300);
    world
        .apply(SimCommand::UnlockUpgrade(UpgradeKind::Miner))
        .unwrap();
    world.advance(25 * SECOND);
    world.drain_events().for_each(drop);
    world
}

/// État complet, comparable quel que soit l'ordre des tables de hachage.
fn state(world: &SimWorld) -> serde_json::Value {
    serde_json::to_value(world).unwrap()
}

#[test]
fn a_reloaded_game_continues_identically() {
    let mut original = busy_world();
    assert!(original.factory.item_count() > 0);
    let bytes = encode(&original, &SaveMeta::of(&original)).unwrap();
    let (meta, mut loaded) = decode(&bytes).unwrap();
    assert_eq!(meta.ticks, original.tick_count());
    assert_eq!(state(&loaded), state(&original));

    // Les vues reçoivent un `BuildingPlaced` par bâtiment.
    let announced = loaded
        .drain_events()
        .filter(|e| matches!(e, SimEvent::BuildingPlaced { .. }))
        .count();
    assert_eq!(announced, original.buildings.len());

    original.advance(90 * SECOND);
    loaded.advance(90 * SECOND);
    assert_eq!(state(&loaded), state(&original));
    assert!(original.hub.stored_shapes().values().sum::<u64>() > 0);
}

#[test]
fn compressed_saves_stay_small() {
    let world = busy_world();
    let bytes = encode(&world, &SaveMeta::of(&world)).unwrap();
    let json = serde_json::to_vec(&world).unwrap();
    assert!(
        bytes.len() * 4 < json.len(),
        "{} octets pour {} de JSON",
        bytes.len(),
        json.len()
    );
}

/// Sauvegarde de référence au format 1 : si la structure de la simulation change sans
/// migration, ce test casse. `SHAPEZ_WRITE_FIXTURES=1` la régénère.
#[test]
fn format_1_fixture_still_loads() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/format-1.shapezsave"
    );
    let expected = busy_world();
    if std::env::var_os("SHAPEZ_WRITE_FIXTURES").is_some() {
        let meta = SaveMeta {
            saved_at: 0,
            ..SaveMeta::of(&expected)
        };
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
        std::fs::write(path, encode(&expected, &meta).unwrap()).unwrap();
    }
    let (meta, world) = decode(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(meta.ticks, expected.tick_count());
    assert_eq!(state(&world), state(&expected));
}
