//! Couche wires côté jeu : bascule de couche (E), voile sombre, état des câbles,
//! valeurs des pins, levier (clic) et émetteur constant (clic pour ouvrir sa boîte de
//! dialogue, voir [`crate::signal_edit`]).
//! Port de `hud/parts/layer_preview.js`, `lever_toggle.js`, `constant_signal_edit.js`
//! et du rendu de `systems/wire.js` et `systems/wired_pins.js`.

use std::collections::HashMap;

use bevy::prelude::*;
use shapez_core::Item;
use shapez_sim::{
    building::{BuildingKind, Layer},
    hub_goals::Reward,
    placement::{BuildingId, SimCommand},
    wires::{WireState, is_truthy},
};

use crate::{
    AppState,
    buildings_view::{BuildingView, WIRES_OVERLAY_Z, WIRES_Z, wire_sprite_path},
    camera::{CameraSet, MainCamera, TILE_SIZE, tile_to_world},
    editing_ui::{EditingSet, Editor},
    keybindings::{Action, just_pressed},
    placement_ui::PlacementState,
    shape_textures::ShapeTextures,
    shop::OpenPanel,
    signal_edit::SignalEdit,
    sim::Sim,
};

/// Couche affichée et éditée (`root.currentLayer`).
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActiveLayer(pub Layer);

impl ActiveLayer {
    /// Visibilité des éléments de la couche wires.
    pub fn visibility(self) -> Visibility {
        if self.0 == Layer::Wires {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    }
}

/// Élément visible seulement sur la couche wires.
#[derive(Component, Debug)]
pub struct WiresLayer;

/// Systèmes d'entrée de la couche wires (avant la pose des bâtiments).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WiresUiSet;

#[derive(Component, Debug)]
struct Overlay;

/// Apparence actuelle d'un câble : (conflit, allumé, forme).
#[derive(Component, Debug, PartialEq, Eq)]
struct WireLook(bool, bool, u8);

/// Valeur affichée sur un pin.
#[derive(Component, Debug)]
struct PinMarker;

#[derive(Debug, Default)]
pub struct WiresUiPlugin;

impl Plugin for WiresUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveLayer>()
            .add_systems(OnEnter(AppState::InGame), spawn_overlay)
            .add_systems(
                OnExit(AppState::InGame),
                |mut layer: ResMut<ActiveLayer>| {
                    *layer = ActiveLayer::default();
                },
            )
            .add_systems(
                Update,
                (switch_layer, click_interactions)
                    .chain()
                    .in_set(WiresUiSet)
                    .after(CameraSet)
                    .after(EditingSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                PostUpdate,
                (
                    apply_visibility,
                    follow_camera,
                    update_wires,
                    update_pin_markers,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn spawn_overlay(mut commands: Commands, layer: Res<ActiveLayer>) {
    commands.spawn((
        Overlay,
        WiresLayer,
        layer.visibility(),
        DespawnOnExit(AppState::InGame),
        Sprite {
            color: Color::srgba(0.09, 0.11, 0.16, 0.72),
            custom_size: Some(Vec2::splat(200_000.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, WIRES_OVERLAY_Z),
    ));
}

/// Touche E (`switchLayers`), une fois les câbles débloqués.
fn switch_layer(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<Sim>,
    panel: Res<OpenPanel>,
    mut layer: ResMut<ActiveLayer>,
    mut placement: ResMut<PlacementState>,
) {
    if !just_pressed(&keys, Action::SwitchLayers)
        || panel.is_open()
        || !sim.hub.is_reward_unlocked(Reward::WiresPainterAndLevers)
    {
        return;
    }
    layer.0 = match layer.0 {
        Layer::Regular => Layer::Wires,
        Layer::Wires => Layer::Regular,
    };
    placement.selected = None;
}

/// Clic sur un levier (`HUDLeverToggle`) ou, sur la couche wires, sur un émetteur
/// constant (`HUDConstantSignalEdit`) : ils passent avant la pose de bâtiments.
#[allow(clippy::too_many_arguments)]
fn click_interactions(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<OpenPanel>,
    layer: Res<ActiveLayer>,
    placement: Res<PlacementState>,
    mut editor: ResMut<Editor>,
    mut signal_edit: ResMut<SignalEdit>,
    mut sim: ResMut<Sim>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if !mouse.just_pressed(MouseButton::Left) || panel.is_open() || editor.is_busy() || ctrl {
        return;
    }
    let Some(tile) = placement.cursor_tile else {
        return;
    };
    if let Some((_, lever)) = sim.buildings.building_at(tile)
        && lever.kind == BuildingKind::Lever
    {
        let on = lever.config == Some(Item::Bool(true));
        let _ = sim.apply(SimCommand::Configure {
            tile,
            layer: Layer::Regular,
            config: (!on).then_some(Item::Bool(true)),
        });
        editor.swallow = true;
        return;
    }
    if layer.0 != Layer::Wires {
        return;
    }
    if let Some((_, signal)) = sim.buildings.building_at_layer(tile, Layer::Wires)
        && signal.kind == BuildingKind::ConstantSignal
    {
        signal_edit.open(&sim, tile, false);
        editor.swallow = true;
    }
}

fn apply_visibility(layer: Res<ActiveLayer>, mut query: Query<&mut Visibility, With<WiresLayer>>) {
    if !layer.is_changed() {
        return;
    }
    for mut visibility in &mut query {
        *visibility = layer.visibility();
    }
}

fn follow_camera(
    camera: Single<&Transform, (With<MainCamera>, Without<Overlay>)>,
    mut overlay: Single<&mut Transform, With<Overlay>>,
) {
    overlay.translation.x = camera.translation.x;
    overlay.translation.y = camera.translation.y;
}

/// Sprite et opacité de chaque câble selon son réseau
/// (`getSpriteSetAndOpacityForWire`).
fn update_wires(
    mut commands: Commands,
    layer: Res<ActiveLayer>,
    sim: Res<Sim>,
    assets: Res<AssetServer>,
    mut wires: Query<(Entity, &BuildingView, &mut Sprite, Option<&WireLook>), With<WiresLayer>>,
) {
    if layer.0 != Layer::Wires {
        return;
    }
    for (entity, view, mut sprite, look) in &mut wires {
        let Some(building) = sim.buildings.get(view.0) else {
            continue;
        };
        if building.kind != BuildingKind::Wire {
            continue;
        }
        let (conflict, lit) = match sim.wiring.wire_state(view.0) {
            WireState::Unconnected => (false, false),
            WireState::Conflict => (true, true),
            WireState::Value(v) => (false, is_truthy(v)),
        };
        let wanted = WireLook(conflict, lit, building.rotation_variant);
        if look == Some(&wanted) {
            continue;
        }
        sprite.image = assets.load(wire_sprite_path(
            building.variant,
            building.rotation_variant,
            conflict,
        ));
        sprite.color = Color::WHITE.with_alpha(if lit { 1.0 } else { 0.5 });
        commands.entity(entity).insert(wanted);
    }
}

/// Petits marqueurs des valeurs portées par les pins (`WiredPinsSystem.drawChunk`).
#[allow(clippy::too_many_arguments)]
fn update_pin_markers(
    mut commands: Commands,
    layer: Res<ActiveLayer>,
    sim: Res<Sim>,
    assets: Res<AssetServer>,
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Local<HashMap<(BuildingId, usize), (Entity, Item)>>,
) {
    let mut wanted: HashMap<(BuildingId, usize), (Item, Vec2)> = HashMap::new();
    if layer.0 == Layer::Wires {
        for (id, building) in sim.buildings.iter() {
            for (k, pin) in building.world_pins().iter().enumerate() {
                let Some(value) = sim.wiring.pin_value(id, k) else {
                    continue;
                };
                let center = tile_to_world(pin.pos.x as f32 + 0.5, pin.pos.y as f32 + 0.5);
                let (dx, dy) = pin.direction.offset();
                let offset = Vec2::new(dx as f32, -(dy as f32)) * 9.1 / 32.0 * TILE_SIZE;
                wanted.insert((id, k), (value, center + offset));
            }
        }
    }
    shown.retain(|key, (entity, value)| {
        let keep = wanted.get(key).is_some_and(|(v, _)| v == value);
        if !keep {
            commands.entity(*entity).try_despawn();
        }
        keep
    });
    for (key, (value, pos)) in wanted {
        if shown.contains_key(&key) {
            continue;
        }
        let (image, size) = match value {
            Item::Bool(b) => (
                assets.load(if b {
                    "sprites/wires/boolean_true.png"
                } else {
                    "sprites/wires/boolean_false.png"
                }),
                9.0,
            ),
            Item::Color(c) => (
                assets.load(format!("sprites/colors/{}.png", c.js_name())),
                14.0,
            ),
            Item::Shape(id) => (
                textures.get_or_render(*sim.registry.get(id), &mut images),
                9.0,
            ),
        };
        let entity = commands
            .spawn((
                PinMarker,
                DespawnOnExit(AppState::InGame),
                Sprite {
                    image,
                    custom_size: Some(Vec2::splat(size / 32.0 * TILE_SIZE)),
                    ..default()
                },
                Transform::from_translation(pos.extend(WIRES_Z + 3.0)),
            ))
            .id();
        shown.insert(key, (entity, value));
    }
}
