//! Compare la couche wires au code JS d'origine (`tools/js-oracle/wires.mjs`).
//!
//! Le générateur fait tourner `WireSystem`, `LogicGateSystem`, `LeverSystem` et
//! `ConstantSignalSystem` du jeu sur des circuits aléatoires (portes, transistors,
//! analyseurs, comparateurs, processeurs virtuels, signaux de formes) ; à chaque tick,
//! toutes les sorties de chaque porte et l'état du réseau de chaque câble doivent être
//! identiques.

use serde::Deserialize;
use shapez_core::{Color, Item, Rotation, ShapeDef, ShapeRegistry, TilePos};
use shapez_sim::{
    SimWorld,
    building::{BuildingKind, Layer, PinKind, Variant},
    placement::{Placement, SimCommand},
    wires::WireState,
};

#[derive(Deserialize)]
struct Golden {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    buildings: Vec<Spec>,
    ops: Vec<Op>,
    frames: Vec<Frame>,
}

#[derive(Deserialize, Clone)]
struct Spec {
    kind: String,
    variant: String,
    x: i32,
    y: i32,
    rot: u16,
    #[serde(default)]
    on: bool,
    #[serde(default)]
    signal: String,
}

#[derive(Deserialize)]
struct Op {
    t: usize,
    op: String,
    x: Option<i32>,
    y: Option<i32>,
    layer: Option<String>,
    building: Option<Spec>,
    signal: Option<String>,
}

#[derive(Deserialize)]
struct Frame {
    gates: String,
    wires: String,
}

fn decode(code: &str, registry: &mut ShapeRegistry) -> Option<Item> {
    match code {
        "" => None,
        "0" => Some(Item::Bool(false)),
        "1" => Some(Item::Bool(true)),
        c if c.len() > 1 => Some(Item::Shape(
            registry.intern(ShapeDef::from_short_key(c).expect("forme valide")),
        )),
        c => Some(Item::Color(
            Color::from_shortcode(c.chars().next().unwrap()).expect("couleur connue"),
        )),
    }
}

fn encode(item: Option<Item>, registry: &ShapeRegistry) -> String {
    match item {
        None => String::new(),
        Some(Item::Bool(b)) => if b { "1" } else { "0" }.to_string(),
        Some(Item::Color(c)) => c.shortcode().to_string(),
        Some(Item::Shape(id)) => registry.get(id).short_key(),
    }
}

fn placement(spec: &Spec, registry: &mut ShapeRegistry) -> Placement {
    let kind = match spec.kind.as_str() {
        "wire" => BuildingKind::Wire,
        "wire_tunnel" => BuildingKind::WireTunnel,
        "logic_gate" => BuildingKind::LogicGate,
        "transistor" => BuildingKind::Transistor,
        "lever" => BuildingKind::Lever,
        "constant_signal" => BuildingKind::ConstantSignal,
        "analyzer" => BuildingKind::Analyzer,
        "comparator" => BuildingKind::Comparator,
        "virtual_processor" => BuildingKind::VirtualProcessor,
        k => panic!("bâtiment inconnu : {k}"),
    };
    let variant = match spec.variant.as_str() {
        "default" | "first" => Variant::Default,
        "second" => Variant::Second,
        "or" => Variant::Or,
        "not" => Variant::Not,
        "xor" => Variant::Xor,
        "mirrored" => Variant::Mirrored,
        "rotater" => Variant::Rotate,
        "stacker" => Variant::Stack,
        "painter" => Variant::Paint,
        "unstacker" => Variant::Unstack,
        v => panic!("variante inconnue : {v}"),
    };
    let rotation = Rotation::from_degrees(spec.rot).expect("angle droit");
    let config = match kind {
        BuildingKind::Lever => spec.on.then_some(Item::Bool(true)),
        BuildingKind::ConstantSignal => decode(&spec.signal, registry),
        _ => None,
    };
    Placement::new(kind, TilePos::new(spec.x, spec.y), rotation)
        .with_variant(variant)
        .with_config(config)
}

/// Bâtiments de la couche wires d'un type donné, ligne par ligne.
fn sorted_wires_layer(world: &SimWorld, keep: impl Fn(BuildingKind) -> bool) -> Vec<String> {
    let mut found: Vec<_> = world
        .buildings
        .iter()
        .filter(|(_, b)| b.kind.layer() == Layer::Wires && keep(b.kind))
        .map(|(id, b)| (b.origin.y, b.origin.x, id, b.kind))
        .collect();
    found.sort_by_key(|&(y, x, ..)| (y, x));
    found
        .into_iter()
        .map(|(.., id, kind)| {
            if kind == BuildingKind::Wire {
                match world.wiring.wire_state(id) {
                    WireState::Unconnected => "-".to_string(),
                    WireState::Conflict => "x".to_string(),
                    WireState::Value(v) => encode(v, &world.registry),
                }
            } else {
                // Tous les pins émetteurs, dans l'ordre.
                let building = world.buildings.get(id).unwrap();
                building
                    .world_pins()
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.kind == PinKind::Ejector)
                    .map(|(k, _)| encode(world.wiring.pin_value(id, k), &world.registry))
                    .collect::<Vec<_>>()
                    .join("/")
            }
        })
        .collect()
}

#[test]
fn wires_match_js() {
    let json = match std::env::var("SHAPEZ_GOLDEN_WIRES") {
        Ok(path) => std::fs::read_to_string(path).expect("fichier golden lisible"),
        Err(_) => include_str!("golden/wires.json").to_string(),
    };
    let golden: Golden = serde_json::from_str(&json).expect("JSON golden valide");
    assert!(!golden.cases.is_empty());

    for (c, case) in golden.cases.iter().enumerate() {
        let mut world = SimWorld::new(0);
        for spec in &case.buildings {
            let p = placement(spec, &mut world.registry);
            world
                .apply(SimCommand::Place(p))
                .unwrap_or_else(|e| panic!("cas {c} : pose refusée {e:?}"));
        }
        let mut ops = case.ops.iter().peekable();
        for (t, frame) in case.frames.iter().enumerate() {
            while let Some(op) = ops.next_if(|op| op.t == t) {
                match op.op.as_str() {
                    "toggle" => {
                        let tile = TilePos::new(op.x.unwrap(), op.y.unwrap());
                        let id = world.buildings.at(tile).expect("levier");
                        let on = world.buildings.get(id).unwrap().config == Some(Item::Bool(true));
                        world
                            .apply(SimCommand::Configure {
                                tile,
                                layer: Layer::Regular,
                                config: (!on).then_some(Item::Bool(true)),
                            })
                            .unwrap();
                    }
                    "remove" => {
                        let layer = match op.layer.as_deref() {
                            Some("regular") => Layer::Regular,
                            _ => Layer::Wires,
                        };
                        let tile = TilePos::new(op.x.unwrap(), op.y.unwrap());
                        world.apply(SimCommand::Remove { tile, layer }).unwrap();
                    }
                    "place" => {
                        let spec = op.building.as_ref().unwrap();
                        let p = placement(spec, &mut world.registry);
                        world.apply(SimCommand::Place(p)).unwrap();
                    }
                    "signal" => {
                        let config = decode(op.signal.as_deref().unwrap(), &mut world.registry);
                        let tile = TilePos::new(op.x.unwrap(), op.y.unwrap());
                        world
                            .apply(SimCommand::Configure {
                                tile,
                                layer: Layer::Wires,
                                config,
                            })
                            .unwrap();
                    }
                    other => panic!("opération inconnue : {other}"),
                }
            }
            world.tick();
            let gates = sorted_wires_layer(&world, |k| {
                matches!(
                    k,
                    BuildingKind::LogicGate
                        | BuildingKind::Transistor
                        | BuildingKind::Analyzer
                        | BuildingKind::Comparator
                        | BuildingKind::VirtualProcessor
                )
            })
            .join(",");
            let wires = sorted_wires_layer(&world, |k| k == BuildingKind::Wire).join(",");
            assert_eq!(gates, frame.gates, "cas {c}, tick {t} : portes");
            assert_eq!(wires, frame.wires, "cas {c}, tick {t} : câbles");
        }
    }
}
