//! Informations dessinées par-dessus certains bâtiments de la couche normale : valeur
//! d'un afficheur (`systems/display.js`), contenu d'un stockage (`systems/storage.js`),
//! débit d'un lecteur et témoins « non relié » / « désactivé » des filtres et du
//! peintre quadruple (`systems/item_processor_overlays.js`).

use std::collections::HashMap;

use bevy::prelude::*;
use shapez_core::{Color as ShapeColor, Item};
use shapez_sim::{
    building::{Building, BuildingKind, Variant},
    placement::BuildingId,
    units::SECOND,
    wires::is_truthy,
};

use crate::{
    AppState,
    camera::{TILE_SIZE, tile_to_world},
    i18n::{decimal, format_big_number},
    shape_textures::ShapeTextures,
    sim::Sim,
};

/// Au-dessus des bâtiments, sous le voile de la couche wires.
const OVERLAY_Z: f32 = 8.0;

/// Ce qu'affiche un élément ; il est recréé quand cela change.
#[derive(Debug, Clone, PartialEq)]
enum Look {
    Item(Item),
    Image(&'static str, Vec2),
    Text(String, [u8; 3]),
}

/// Élément posé sur un bâtiment, en coordonnées monde.
#[derive(Debug, Clone, PartialEq)]
struct Part {
    look: Look,
    pos: Vec2,
    z: f32,
    /// Témoin qui pulse (`smoothPulse`), de taille `TILE_SIZE × (0.7 + 0.2 × pulse)`.
    pulse: bool,
}

#[derive(Component, Debug)]
struct Pulse;

#[derive(Debug, Default)]
pub struct OverlaysPlugin;

impl Plugin for OverlaysPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (sync_overlays, animate_pulses)
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// `smoothPulse`.
fn smooth_pulse(seconds: f32) -> f32 {
    (seconds * 4.0).sin() * 0.5 + 0.5
}

fn center_of(building: &Building) -> Vec2 {
    let (min, (w, h)) = building.bounds();
    tile_to_world(min.x as f32 + w as f32 / 2.0, min.y as f32 + h as f32 / 2.0)
}

/// Pixel du JS (tuile de 32) vers l'unité monde.
fn px(v: f32) -> f32 {
    v / 32.0 * TILE_SIZE
}

/// Éléments voulus pour chaque bâtiment.
fn wanted_parts(sim: &Sim) -> HashMap<(BuildingId, u8), Part> {
    let mut out = HashMap::new();
    let part = |look, pos, z| Part {
        look,
        pos,
        z,
        pulse: false,
    };
    for (id, building) in sim.buildings.iter() {
        let origin = tile_to_world(
            building.origin.x as f32 + 0.5,
            building.origin.y as f32 + 0.5,
        );
        match building.kind {
            BuildingKind::Display => {
                let value = sim.wiring.input_value(id, 0);
                // `getDisplayItem` : vrai → blanc, faux et « sans couleur » → rien.
                let shown = match value {
                    Some(Item::Bool(true)) => Some(Item::Color(ShapeColor::White)),
                    Some(Item::Color(ShapeColor::Uncolored)) | Some(Item::Bool(false)) => None,
                    other => other,
                };
                if let Some(item) = shown {
                    out.insert((id, 0), part(Look::Item(item), origin, OVERLAY_Z));
                }
            }
            BuildingKind::Storage => {
                let Some(storage) = sim.factory.storage(id) else {
                    continue;
                };
                let Some(item) = storage.item else {
                    continue;
                };
                let center = center_of(building);
                let below = center - Vec2::new(0.0, px(22.5));
                out.insert((id, 0), part(Look::Item(item), center, OVERLAY_Z));
                out.insert(
                    (id, 1),
                    part(
                        Look::Image(
                            "sprites/misc/storage_overlay.png",
                            Vec2::new(px(30.0), px(15.0)),
                        ),
                        below,
                        OVERLAY_Z + 0.1,
                    ),
                );
                out.insert(
                    (id, 2),
                    part(
                        Look::Text(format_big_number(storage.count), [0x64, 0x66, 0x6e]),
                        below,
                        OVERLAY_Z + 0.2,
                    ),
                );
            }
            BuildingKind::Reader => {
                out.insert(
                    (id, 0),
                    part(
                        Look::Image("sprites/misc/reader_overlay.png", Vec2::splat(TILE_SIZE)),
                        origin,
                        OVERLAY_Z,
                    ),
                );
                let throughput = sim.factory.reader(id).map_or(0.0, |r| r.throughput());
                let text = decimal(f64::from(throughput), 1);
                out.insert(
                    (id, 1),
                    part(
                        Look::Text(text, [0x33, 0x34, 0x39]),
                        origin - Vec2::new(0.0, px(0.12 * 32.0 - 3.0)),
                        OVERLAY_Z + 0.1,
                    ),
                );
            }
            BuildingKind::Filter => {
                // `drawConnectedSlotRequirement`, `drawIfFalse: false`.
                if sim.wiring.input_value(id, 0).is_none() {
                    out.insert((id, 0), indicator(false, origin));
                }
            }
            BuildingKind::Painter if building.variant == Variant::Quad => {
                // `drawIfFalse: true` : témoin tant qu'aucun quart n'est activé.
                let values: Vec<_> = (0..4).map(|k| sim.wiring.input_value(id, k)).collect();
                let connected = values.iter().any(Option::is_some);
                if !values.iter().any(|v| is_truthy(*v)) {
                    out.insert((id, 0), indicator(connected, origin));
                }
            }
            _ => {}
        }
    }
    out
}

/// Témoin `processor_disabled` (relié, mais signal faux) ou `processor_disconnected`.
fn indicator(connected: bool, pos: Vec2) -> Part {
    let image = if connected {
        "sprites/misc/processor_disabled.png"
    } else {
        "sprites/misc/processor_disconnected.png"
    };
    Part {
        look: Look::Image(image, Vec2::splat(TILE_SIZE)),
        pos,
        z: OVERLAY_Z + 0.3,
        pulse: true,
    }
}

fn sync_overlays(
    mut commands: Commands,
    sim: Res<Sim>,
    assets: Res<AssetServer>,
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Local<HashMap<(BuildingId, u8), (Entity, Part)>>,
) {
    let mut wanted = wanted_parts(&sim);
    shown.retain(|key, (entity, part)| {
        let keep = wanted.get(key) == Some(part);
        if keep {
            wanted.remove(key);
        } else {
            commands.entity(*entity).try_despawn();
        }
        keep
    });
    for (key, part) in wanted {
        let transform = Transform::from_translation(part.pos.extend(part.z));
        let mut entity = match &part.look {
            Look::Text(text, [r, g, b]) => commands.spawn((
                Text2d::new(text.clone()),
                TextFont {
                    font_size: FontSize::Px(px(10.0)),
                    ..default()
                },
                TextColor(Color::srgb_u8(*r, *g, *b)),
                transform,
            )),
            Look::Image(path, size) => commands.spawn((
                Sprite {
                    image: assets.load(*path),
                    custom_size: Some(*size),
                    ..default()
                },
                transform,
            )),
            Look::Item(item) => {
                let (image, size) = match *item {
                    Item::Shape(shape) => (
                        textures.get_or_render(*sim.registry.get(shape), &mut images),
                        px(30.0),
                    ),
                    Item::Color(c) => (
                        assets.load(format!("sprites/wires/display/{}.png", c.js_name())),
                        TILE_SIZE,
                    ),
                    Item::Bool(b) => (
                        assets.load(if b {
                            "sprites/wires/boolean_true.png"
                        } else {
                            "sprites/wires/boolean_false.png"
                        }),
                        px(20.0),
                    ),
                };
                commands.spawn((
                    Sprite {
                        image,
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                    transform,
                ))
            }
        };
        entity.insert(DespawnOnExit(AppState::InGame));
        if part.pulse {
            entity.insert(Pulse);
        }
        let id = entity.id();
        shown.insert(key, (id, part));
    }
}

fn animate_pulses(sim: Res<Sim>, mut query: Query<&mut Sprite, With<Pulse>>) {
    let pulse = smooth_pulse(sim.time() as f32 / SECOND as f32);
    for mut sprite in &mut query {
        sprite.color = Color::WHITE.with_alpha(0.6 + 0.4 * pulse);
        sprite.custom_size = Some(Vec2::splat(TILE_SIZE * (0.7 + 0.2 * pulse)));
    }
}
