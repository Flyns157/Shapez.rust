//! Scénarios de bout en bout : débit des lignes de production et déterminisme.
//!
//! Les usines sont construites uniquement par des [`SimCommand`], comme le ferait le
//! joueur, sur une vraie carte générée.

use shapez_core::{Direction, Item, Rotation, ShapeDef, TilePos};
use shapez_sim::{
    SimWorld, TICKS_PER_SECOND,
    building::{BuildingKind, Variant, tunnel_variant},
    map::MapResource,
    placement::{Placement, SimCommand, SimEvent},
};

const SECOND: u32 = TICKS_PER_SECOND;

/// Pose un bâtiment ; les convoyeurs et tunnels sont orientés comme par l'interface.
fn place(
    world: &mut SimWorld,
    kind: BuildingKind,
    variant: Variant,
    x: i32,
    y: i32,
    rot: Rotation,
) {
    let tile = TilePos::new(x, y);
    let mut p = Placement::new(kind, tile, rot).with_variant(variant);
    match kind {
        BuildingKind::Belt => {
            (p.rotation, p.rotation_variant) = world.buildings.optimal_belt(tile, rot)
        }
        BuildingKind::UndergroundBelt => {
            (p.rotation, p.rotation_variant) = world.buildings.optimal_tunnel(tile, rot, variant);
        }
        _ => {}
    }
    world
        .apply(SimCommand::Place(p))
        .unwrap_or_else(|e| panic!("{kind:?} en {tile:?} : {e:?}"));
}

fn belt(world: &mut SimWorld, x: i32, y: i32, rot: Rotation) {
    place(world, BuildingKind::Belt, Variant::Default, x, y, rot);
}

/// Cherche une graine et une rangée y ∈ [-2, 1] (face au bord droit du hub) avec la
/// ressource voulue en x ∈ [`min_x`, 30], et rien entre elle et le hub qui gêne.
fn find_row_resource(min_x: i32, want: impl Fn(&MapResource) -> bool) -> (u32, i32, i32) {
    for seed in 0..500 {
        let mut world = SimWorld::new(seed);
        for y in -2..=1 {
            for x in min_x..=30 {
                if world
                    .map
                    .resource_at(TilePos::new(x, y))
                    .is_some_and(|r| want(&r))
                {
                    return (seed, x, y);
                }
            }
        }
    }
    panic!("aucune graine ne convient");
}

fn is_full_circle(r: &MapResource) -> bool {
    matches!(r, MapResource::Shape(s) if s.short_key() == "CuCuCuCu")
}

fn delivered(world: &SimWorld, key: &str) -> u64 {
    world
        .registry
        .find(&ShapeDef::from_short_key(key).unwrap())
        .map_or(0, |id| world.hub.stored(id))
}

/// Extracteur en (x, y) qui éjecte vers la gauche, puis convoyeurs jusqu'au hub.
fn miner_line_to_hub(seed: u32, x: i32, y: i32) -> SimWorld {
    let mut world = SimWorld::new_game(seed);
    place(
        &mut world,
        BuildingKind::Miner,
        Variant::Default,
        x,
        y,
        Rotation::R270,
    );
    for bx in (2..x).rev() {
        belt(&mut world, bx, y, Rotation::R270);
    }
    world
}

#[test]
fn miner_feeds_the_hub_at_its_nominal_rate() {
    let (seed, x, y) = find_row_resource(4, is_full_circle);
    let mut world = miner_line_to_hub(seed, x, y);
    // Une seule ligne : un chemin de convoyeurs qui livre au hub.
    assert_eq!(world.factory.paths().count(), 1);

    world.advance(60 * SECOND);
    let first = delivered(&world, "CuCuCuCu");
    world.advance(100 * SECOND);
    let steady = delivered(&world, "CuCuCuCu") - first;
    // 0,4 forme/s (2,5 s par extraction) : 40 formes en 100 s.
    assert_eq!(steady, 40);
}

#[test]
fn level_one_completes() {
    let (seed, x, y) = find_row_resource(4, is_full_circle);
    let mut world = miner_line_to_hub(seed, x, y);
    let mut completed = None;
    for _ in 0..200 * SECOND {
        world.tick();
        for e in world.drain_events() {
            if let SimEvent::LevelCompleted(done) = e {
                completed = Some(done.level);
            }
        }
        if completed.is_some() {
            break;
        }
    }
    assert_eq!(completed, Some(1));
    assert_eq!(world.hub.level, 2);
}

#[test]
fn cutter_splits_circles_and_trash_eats_the_rest() {
    let (seed, x, y) = find_row_resource(8, is_full_circle);
    let mut world = SimWorld::new_game(seed);
    // Extracteur → convoyeurs → découpeur (vers la gauche, 2 sorties) → hub.
    // Découpeur tourné à 270° : il occupe (cx, y) et (cx, y + 1), sortie gauche.
    place(
        &mut world,
        BuildingKind::Miner,
        Variant::Default,
        x,
        y,
        Rotation::R270,
    );
    let cx = 5;
    for bx in (cx + 1..x).rev() {
        belt(&mut world, bx, y, Rotation::R270);
    }
    place(
        &mut world,
        BuildingKind::Cutter,
        Variant::Default,
        cx,
        y,
        Rotation::R270,
    );
    // Sortie 0 en (cx, y) → hub ; sortie 1 en (cx, y - 1) → poubelle.
    for bx in (2..cx).rev() {
        belt(&mut world, bx, y, Rotation::R270);
    }
    place(
        &mut world,
        BuildingKind::Trash,
        Variant::Default,
        cx - 1,
        y - 1,
        Rotation::R0,
    );

    world.advance(120 * SECOND);
    let halves = delivered(&world, "----CuCu") + delivered(&world, "CuCu----");
    assert!(halves >= 30, "demi-cercles livrés : {halves}");
    assert_eq!(delivered(&world, "CuCuCuCu"), 0);
}

#[test]
fn tunnel_carries_items_under_obstacles() {
    let (seed, x, y) = find_row_resource(10, is_full_circle);
    let mut world = SimWorld::new_game(seed);
    place(
        &mut world,
        BuildingKind::Miner,
        Variant::Default,
        x,
        y,
        Rotation::R270,
    );
    for bx in (9..x).rev() {
        belt(&mut world, bx, y, Rotation::R270);
    }
    // Émetteur en 8, obstacle en 6..7, récepteur en 5.
    place(
        &mut world,
        BuildingKind::UndergroundBelt,
        Variant::Default,
        8,
        y,
        Rotation::R270,
    );
    place(
        &mut world,
        BuildingKind::Trash,
        Variant::Default,
        7,
        y,
        Rotation::R0,
    );
    place(
        &mut world,
        BuildingKind::Trash,
        Variant::Default,
        6,
        y,
        Rotation::R0,
    );
    place(
        &mut world,
        BuildingKind::UndergroundBelt,
        Variant::Default,
        5,
        y,
        Rotation::R90,
    );
    let receiver = world.buildings.building_at(TilePos::new(5, y)).unwrap().1;
    assert_eq!(receiver.rotation_variant, tunnel_variant::RECEIVER);
    for bx in (2..5).rev() {
        belt(&mut world, bx, y, Rotation::R270);
    }
    world.advance(60 * SECOND);
    assert!(delivered(&world, "CuCuCuCu") >= 15);
}

#[test]
fn balancer_splits_one_line_in_two() {
    let (seed, x, y) = find_row_resource(8, is_full_circle);
    let mut world = SimWorld::new_game(seed);
    place(
        &mut world,
        BuildingKind::Miner,
        Variant::Default,
        x,
        y,
        Rotation::R270,
    );
    for bx in (6..x).rev() {
        belt(&mut world, bx, y, Rotation::R270);
    }
    // Séparateur : entrée par la droite, sorties à gauche et en haut (vers y - 1).
    place(
        &mut world,
        BuildingKind::Balancer,
        Variant::Splitter,
        5,
        y,
        Rotation::R270,
    );
    let splitter = world.buildings.at(TilePos::new(5, y)).unwrap();
    for bx in (2..5).rev() {
        belt(&mut world, bx, y, Rotation::R270);
    }
    world.advance(80 * SECOND);
    let ejector = world.factory.ejector(splitter).unwrap();
    assert_eq!(ejector.slots.len(), 2);
    // La sortie latérale n'est reliée à rien : la moitié des formes y reste bloquée,
    // l'autre moitié continue vers le hub.
    let side = ejector
        .slots
        .iter()
        .find(|s| s.to != Direction::Left)
        .unwrap();
    assert!(side.item.is_some());
    assert!(delivered(&world, "CuCuCuCu") >= 15);
}

#[test]
fn removing_a_belt_splits_the_path_and_drops_its_items() {
    let (seed, x, y) = find_row_resource(10, is_full_circle);
    let mut world = miner_line_to_hub(seed, x, y);
    // On coupe la livraison au hub pour que les items s'accumulent.
    world
        .apply(SimCommand::Remove {
            tile: TilePos::new(2, y),
            layer: shapez_sim::building::Layer::Regular,
        })
        .unwrap();
    world.advance(120 * SECOND);
    let before = world.factory.item_count();
    world
        .apply(SimCommand::Remove {
            tile: TilePos::new(x - 3, y),
            layer: shapez_sim::building::Layer::Regular,
        })
        .unwrap();
    assert_eq!(world.factory.paths().count(), 2);
    let after = world.factory.item_count();
    assert!(after < before && after + 2 >= before, "{before} → {after}");
    for (_, path) in world.factory.paths() {
        path.check_integrity().unwrap();
    }
}

#[test]
fn processors_transform_items() {
    // Peintre, mélangeur, empileur et rotateur alimentés par des extracteurs.
    let (seed, x, y) = find_row_resource(8, |r| matches!(r, MapResource::Color(_)));
    let mut world = SimWorld::new_game(seed);
    let color = match world.map.resource_at(TilePos::new(x, y)).unwrap() {
        MapResource::Color(c) => c,
        MapResource::Shape(_) => unreachable!(),
    };
    // Extracteur de couleur → mélangeur (les deux entrées sur la même couleur ne
    // sont pas possibles avec une seule ligne : on vérifie la couleur qui sort).
    place(
        &mut world,
        BuildingKind::Miner,
        Variant::Default,
        x,
        y,
        Rotation::R270,
    );
    for bx in (3..x).rev() {
        belt(&mut world, bx, y, Rotation::R270);
    }
    place(
        &mut world,
        BuildingKind::Trash,
        Variant::Default,
        2,
        y,
        Rotation::R0,
    );
    world.advance(30 * SECOND);
    let colors: Vec<_> = world
        .factory
        .visible_items(&world.buildings)
        .into_iter()
        .map(|v| v.item)
        .collect();
    assert!(!colors.is_empty());
    assert!(colors.iter().all(|&i| i == Item::Color(color)));
}

/// Empreinte de tout l'état observable : positions des items et formes livrées.
fn fingerprint(world: &SimWorld) -> String {
    let mut out = format!(
        "t{} n{} L{}",
        world.tick_count(),
        world.factory.item_count(),
        world.hub.level
    );
    for v in world.factory.visible_items(&world.buildings) {
        out.push_str(&format!(" {:.4},{:.4}:{:?}", v.x, v.y, v.item));
    }
    out
}

#[test]
fn simulation_is_deterministic() {
    let (seed, x, y) = find_row_resource(10, is_full_circle);
    let build = || {
        let mut world = miner_line_to_hub(seed, x, y);
        place(
            &mut world,
            BuildingKind::Cutter,
            Variant::Default,
            6,
            y,
            Rotation::R270,
        );
        world
    };
    let mut a = build();
    let mut b = build();
    a.advance(90 * SECOND);
    for _ in 0..90 * SECOND {
        b.tick();
    }
    assert_eq!(fingerprint(&a), fingerprint(&b));
    let c = a.clone();
    a.advance(30 * SECOND);
    let mut c = c;
    c.advance(30 * SECOND);
    assert_eq!(fingerprint(&a), fingerprint(&c));
}

fn shape(world: &mut SimWorld, key: &str) -> Item {
    Item::Shape(
        world
            .registry
            .intern(ShapeDef::from_short_key(key).unwrap()),
    )
}

/// Items présents sur le chemin du convoyeur en `tile`.
fn items_on(world: &SimWorld, tile: TilePos) -> Vec<Item> {
    let id = world.buildings.at(tile).unwrap();
    world
        .factory
        .path_of(id)
        .unwrap()
        .items_with_progress()
        .map(|(_, i)| i)
        .collect()
}

/// Deux lignes de deux convoyeurs montent vers les entrées d'un bâtiment 2×1 posé
/// en (10, 10) ; la sortie part vers le haut depuis (10, 9).
fn two_input_rig(kind: BuildingKind) -> SimWorld {
    let mut world = SimWorld::new_game(0);
    for x in [10, 11] {
        belt(&mut world, x, 12, Rotation::R0);
        belt(&mut world, x, 11, Rotation::R0);
    }
    place(&mut world, kind, Variant::Default, 10, 10, Rotation::R0);
    belt(&mut world, 10, 9, Rotation::R0);
    belt(&mut world, 10, 8, Rotation::R0);
    world
}

#[test]
fn stacker_puts_the_right_input_on_top() {
    let mut world = two_input_rig(BuildingKind::Stacker);
    let lower = shape(&mut world, "RuRu----");
    let upper = shape(&mut world, "----CgCg");
    assert!(world.inject_item(TilePos::new(10, 12), lower));
    assert!(world.inject_item(TilePos::new(11, 12), upper));
    world.advance(20 * SECOND);
    let expected = shape(&mut world, "RuRuCgCg");
    let out = items_on(&world, TilePos::new(10, 8));
    assert_eq!(
        out,
        vec![expected],
        "{:?}",
        world.registry.get(match out[0] {
            Item::Shape(id) => id,
            _ => unreachable!(),
        })
    );
}

#[test]
fn mixer_mixes_colors() {
    use shapez_core::Color;
    let mut world = two_input_rig(BuildingKind::Mixer);
    assert!(world.inject_item(TilePos::new(10, 12), Item::Color(Color::Red)));
    assert!(world.inject_item(TilePos::new(11, 12), Item::Color(Color::Blue)));
    world.advance(20 * SECOND);
    assert_eq!(
        items_on(&world, TilePos::new(10, 8)),
        vec![Item::Color(Color::Purple)]
    );
}

#[test]
fn painter_paints_the_shape() {
    use shapez_core::Color;
    let mut world = SimWorld::new_game(0);
    // Peintre en (10, 10)-(11, 10) : forme par la gauche, couleur par le haut,
    // sortie à droite.
    belt(&mut world, 8, 10, Rotation::R90);
    belt(&mut world, 9, 10, Rotation::R90);
    belt(&mut world, 11, 8, Rotation::R180);
    belt(&mut world, 11, 9, Rotation::R180);
    place(
        &mut world,
        BuildingKind::Painter,
        Variant::Default,
        10,
        10,
        Rotation::R0,
    );
    belt(&mut world, 12, 10, Rotation::R90);
    belt(&mut world, 13, 10, Rotation::R90);
    let circle = shape(&mut world, "CuCuCuCu");
    assert!(world.inject_item(TilePos::new(8, 10), circle));
    assert!(world.inject_item(TilePos::new(11, 8), Item::Color(Color::Yellow)));
    // Un item de la mauvaise sorte reste bloqué devant le filtre de l'entrée.
    world.advance(20 * SECOND);
    let painted = shape(&mut world, "CyCyCyCy");
    assert_eq!(items_on(&world, TilePos::new(13, 10)), vec![painted]);
}

#[test]
fn slot_filters_block_wrong_items() {
    use shapez_core::Color;
    let mut world = two_input_rig(BuildingKind::Stacker);
    assert!(world.inject_item(TilePos::new(10, 12), Item::Color(Color::Red)));
    world.advance(10 * SECOND);
    // La couleur reste au bout de la ligne : l'empileur n'accepte que des formes.
    assert_eq!(
        items_on(&world, TilePos::new(10, 12)),
        vec![Item::Color(Color::Red)]
    );
}

#[test]
fn statistics_count_mined_and_delivered_shapes() {
    use shapez_sim::analytics::DataSource;
    let (seed, x, y) = find_row_resource(4, is_full_circle);
    let mut world = miner_line_to_hub(seed, x, y);
    world.advance(60 * SECOND);
    let circle = world
        .registry
        .find(&ShapeDef::from_short_key("CuCuCuCu").unwrap())
        .unwrap();
    // 0,4 forme/s : 4 formes par tranche de 10 s, extraites comme livrées.
    let a = &world.hub.analytics;
    assert_eq!(a.current_count(DataSource::Produced, circle), 4);
    assert_eq!(a.current_count(DataSource::Delivered, circle), 4);
}

#[test]
fn buying_a_miner_upgrade_speeds_up_extraction() {
    use shapez_sim::upgrades::UpgradeKind;
    let (seed, x, y) = find_row_resource(4, is_full_circle);
    let mut world = miner_line_to_hub(seed, x, y);
    assert_eq!(
        world.apply(SimCommand::UnlockUpgrade(UpgradeKind::Miner)),
        Err(shapez_sim::placement::CommandError::CannotAfford)
    );
    let rect = world
        .registry
        .intern(ShapeDef::from_short_key("RuRuRuRu").unwrap());
    world.hub.add_stored(rect, 300);
    world
        .apply(SimCommand::UnlockUpgrade(UpgradeKind::Miner))
        .unwrap();
    assert_eq!(world.hub.stored(rect), 0);
    assert!(world.drain_events().any(|e| matches!(
        e,
        SimEvent::UpgradePurchased {
            kind: UpgradeKind::Miner,
            level: 1
        }
    )));

    world.advance(60 * SECOND);
    let first = delivered(&world, "CuCuCuCu");
    world.advance(100 * SECOND);
    // ×1,5 : 0,6 forme/s.
    assert_eq!(delivered(&world, "CuCuCuCu") - first, 60);
}
