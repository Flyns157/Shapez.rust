//! Les chemins dormants (§5.4) ne changent rien : deux mondes identiques, l'un avec et
//! l'autre sans, restent identiques tick après tick sous une suite pseudo-aléatoire de
//! constructions, suppressions, injections d'items et achats d'améliorations.

use shapez_core::{Color, Item, Rotation, ShapeDef, TilePos};
use shapez_sim::{
    SimWorld,
    building::{BuildingKind, Layer, Variant},
    placement::{Placement, SimCommand},
    upgrades::UpgradeKind,
};

/// Générateur congruentiel : la suite d'opérations est la même à chaque exécution.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }

    fn tile(&mut self) -> TilePos {
        TilePos::new(self.below(10) as i32 + 4, self.below(10) as i32 - 5)
    }

    fn rotation(&mut self) -> Rotation {
        [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270][self.below(4) as usize]
    }
}

const UPGRADE_SHAPES: [&str; 10] = [
    "--CuCu--",
    "CbRbRbCb:CwCwCwCw:WbWbWbWb",
    "CpCpCpCp",
    "Cu------",
    "CuCuCuCu",
    "CwCwCwCw:WbWbWbWb",
    "RuRuRuRu",
    "ScScScSc",
    "SrSrSrSr:CyCyCyCy",
    "SrSrSrSr:CyCyCyCy:SwSwSwSw",
];

/// Surtout des convoyeurs, et des bâtiments lents qui les font refouler.
const KINDS: [BuildingKind; 12] = [
    BuildingKind::Belt,
    BuildingKind::Belt,
    BuildingKind::Belt,
    BuildingKind::Belt,
    BuildingKind::Belt,
    BuildingKind::Balancer,
    BuildingKind::Trash,
    BuildingKind::Storage,
    BuildingKind::Rotater,
    BuildingKind::Rotater,
    BuildingKind::Cutter,
    BuildingKind::Miner,
];

/// Une opération, appliquée aux deux mondes à l'identique.
fn random_op(rng: &mut Lcg, world: &SimWorld) -> Option<SimCommand> {
    match rng.below(100) {
        0..=59 => {
            let kind = KINDS[rng.below(KINDS.len() as u32) as usize];
            let tile = rng.tile();
            let rot = rng.rotation();
            let mut p = Placement::new(kind, tile, rot).with_variant(Variant::Default);
            if kind == BuildingKind::Belt {
                (p.rotation, p.rotation_variant) = world.buildings.optimal_belt(tile, rot);
            }
            Some(SimCommand::Place(p))
        }
        60..=84 => Some(SimCommand::Remove {
            tile: rng.tile(),
            layer: Layer::Regular,
        }),
        85..=86 => Some(SimCommand::UnlockUpgrade(UpgradeKind::Belt)),
        _ => None,
    }
}

fn snapshot(world: &SimWorld) -> String {
    serde_json::to_string(world).expect("monde sérialisable")
}

#[test]
fn dormant_paths_change_nothing() {
    for seed in 0..4u64 {
        let mut rng = Lcg(seed);
        let mut fast = SimWorld::new_game(seed as u32);
        let mut reference = fast.clone();
        reference.factory.disable_dormancy();
        // De quoi acheter des paliers de convoyeurs : la vitesse change en cours de route.
        for world in [&mut fast, &mut reference] {
            for key in UPGRADE_SHAPES {
                let shape = world
                    .registry
                    .intern(ShapeDef::from_short_key(key).expect("forme valide"));
                world.hub.add_stored(shape, 1_000_000);
            }
        }
        let mut slept = 0;
        for tick in 0..3000 {
            if tick % 7 == 0
                && let Some(op) = random_op(&mut rng, &fast)
            {
                let a = fast.apply(op);
                let b = reference.apply(op);
                assert_eq!(a.is_ok(), b.is_ok(), "graine {seed}, tick {tick} : {op:?}");
            }
            {
                let tile = rng.tile();
                let item =
                    Item::Color([Color::Red, Color::Green, Color::Blue][rng.below(3) as usize]);
                assert_eq!(
                    fast.inject_item(tile, item),
                    reference.inject_item(tile, item)
                );
            }
            fast.tick();
            reference.tick();
            slept += fast.factory.asleep_paths();
            assert_eq!(
                snapshot(&fast),
                snapshot(&reference),
                "graine {seed}, tick {tick}"
            );
        }
        assert!(slept > 0, "graine {seed} : aucun chemin ne s'est endormi");
    }
}
