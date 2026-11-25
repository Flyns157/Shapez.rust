//! Boîte de dialogue des émetteurs constants (port de `hud/parts/constant_signal_edit.js`).
//!
//! Elle s'ouvre quand on pose un émetteur (Echap le retire alors) ou quand on clique
//! sur un émetteur de la couche câbles. On choisit une valeur parmi les propositions,
//! ou on tape un code (`1`, `false`, `red`, `CuCuCuCu`…) validé par Entrée.

use bevy::{
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
    ui_widgets::{Activate, ActivateOnPress, Button},
};
use shapez_core::{Color as ShapeColor, Item, ShapeDef, TilePos};
use shapez_sim::{
    building::{BuildingKind, Layer},
    editing::BLUEPRINT_SHAPE,
    placement::{SimCommand, SimEvent},
    wires::{parse_signal, signal_code},
};

use crate::{
    AppState,
    placement_ui::PlacementState,
    shape_textures::ShapeTextures,
    sim::{Sim, SimEventMsg, SimEventsSet},
};

/// Longueur maximale du code tapé (un code de 4 couches fait 35 caractères).
const MAX_CODE: usize = 40;

/// Émetteur en cours d'édition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Target {
    tile: TilePos,
    /// Émetteur tout juste posé : Echap le retire (`deleteOnCancel`).
    delete_on_cancel: bool,
}

/// État de la boîte de dialogue.
#[derive(Resource, Debug, Default)]
pub struct SignalEdit {
    target: Option<Target>,
    text: String,
    invalid: bool,
}

impl SignalEdit {
    pub fn is_open(&self) -> bool {
        self.target.is_some()
    }

    /// Ouvre la boîte pour l'émetteur posé en `tile`.
    pub fn open(&mut self, sim: &Sim, tile: TilePos, delete_on_cancel: bool) {
        let current = sim
            .buildings
            .building_at_layer(tile, Layer::Wires)
            .and_then(|(_, b)| b.config);
        self.target = Some(Target {
            tile,
            delete_on_cancel,
        });
        self.text = current.map_or_else(String::new, |v| signal_code(v, &sim.registry));
        self.invalid = false;
    }

    fn close(&mut self) {
        *self = Self::default();
    }
}

#[derive(Component, Debug)]
struct Dialog;

/// Contenu affiché, pour ne reconstruire qu'en cas de changement.
#[derive(Component, Debug, Default, PartialEq)]
struct Shown(Vec<String>);

/// Proposition cliquable.
#[derive(Component, Debug, Clone, Copy)]
struct Choice(Item);

#[derive(Debug, Default)]
pub struct SignalEditPlugin;

impl Plugin for SignalEditPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SignalEdit>()
            .add_systems(OnEnter(AppState::InGame), spawn_dialog)
            .add_systems(OnExit(AppState::InGame), |mut edit: ResMut<SignalEdit>| {
                edit.close();
            })
            .add_systems(PreUpdate, block_game_input.after(InputSystems))
            .add_systems(
                Update,
                (
                    open_on_placement.after(SimEventsSet),
                    type_code,
                    update_dialog,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_observer(on_choice);
    }
}

/// Tant que la boîte est ouverte, le jeu ne voit ni le clavier ni les clics (les
/// boutons de l'interface passent par le picking).
fn block_game_input(
    edit: Res<SignalEdit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut letters: ResMut<ButtonInput<Key>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    if edit.is_open() {
        keys.reset_all();
        letters.reset_all();
        mouse.reset_all();
    }
}

/// Un émetteur posé à la main ouvre la boîte (`entityManuallyPlaced`).
fn open_on_placement(
    mut events: MessageReader<SimEventMsg>,
    placement: Res<PlacementState>,
    sim: Res<Sim>,
    mut edit: ResMut<SignalEdit>,
) {
    for SimEventMsg(event) in events.read() {
        if let SimEvent::BuildingPlaced { building, .. } = event
            && building.kind == BuildingKind::ConstantSignal
            && placement.selected == Some(BuildingKind::ConstantSignal)
        {
            edit.open(&sim, building.origin, true);
        }
    }
}

/// Saisie du code, Entrée pour valider, Echap pour annuler.
fn type_code(
    mut input: MessageReader<KeyboardInput>,
    mut edit: ResMut<SignalEdit>,
    mut sim: ResMut<Sim>,
) {
    let Some(target) = edit.target else {
        input.clear();
        return;
    };
    // L'émetteur a disparu entre-temps (annulation, suppression…).
    if sim
        .buildings
        .building_at_layer(target.tile, Layer::Wires)
        .is_none_or(|(_, b)| b.kind != BuildingKind::ConstantSignal)
    {
        edit.close();
        return;
    }
    for event in input.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        match &event.logical_key {
            Key::Enter => {
                match parse_signal(&edit.text, &mut sim.registry) {
                    Some(value) => {
                        configure(&mut sim, target.tile, value);
                        edit.close();
                    }
                    None => edit.invalid = true,
                }
                return;
            }
            Key::Escape => {
                if target.delete_on_cancel {
                    let _ = sim.apply(SimCommand::Remove {
                        tile: target.tile,
                        layer: Layer::Wires,
                    });
                }
                edit.close();
                return;
            }
            Key::Backspace => {
                edit.text.pop();
                edit.invalid = false;
            }
            _ => {
                if let Some(text) = &event.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if edit.text.chars().count() < MAX_CODE {
                            edit.text.push(c);
                        }
                    }
                    edit.invalid = false;
                }
            }
        }
    }
}

fn configure(sim: &mut Sim, tile: TilePos, value: Item) {
    let _ = sim.apply(SimCommand::Configure {
        tile,
        layer: Layer::Wires,
        config: Some(value),
    });
}

/// Propositions, dans l'ordre du JS : faux, vrai, les couleurs, la forme des plans,
/// la forme du niveau et les formes épinglées.
fn choices(sim: &mut Sim) -> Vec<Item> {
    let mut items = vec![Item::Bool(false), Item::Bool(true)];
    items.extend(ShapeColor::ALL.map(Item::Color));
    let blueprint = ShapeDef::from_short_key(BLUEPRINT_SHAPE).expect("forme valide");
    items.push(Item::Shape(sim.registry.intern(blueprint)));
    items.push(Item::Shape(sim.hub.goal().shape));
    items.extend(sim.marks.pinned.iter().map(|&id| Item::Shape(id)));
    let mut seen = Vec::new();
    items.retain(|item| {
        let fresh = !seen.contains(item);
        seen.push(*item);
        fresh
    });
    items
}

fn on_choice(
    activate: On<Activate>,
    choices: Query<&Choice>,
    sim: Option<ResMut<Sim>>,
    mut edit: ResMut<SignalEdit>,
) {
    let Ok(&Choice(value)) = choices.get(activate.event_target()) else {
        return;
    };
    let (Some(mut sim), Some(target)) = (sim, edit.target) else {
        return;
    };
    configure(&mut sim, target.tile, value);
    edit.close();
}

fn spawn_dialog(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        Dialog,
        Shown::default(),
        Node {
            position_type: PositionType::Absolute,
            top: percent(25),
            left: percent(20),
            right: percent(20),
            flex_direction: FlexDirection::Column,
            row_gap: px(12),
            padding: UiRect::all(px(16)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.08, 0.08, 0.1, 0.96)),
        GlobalZIndex(20),
    ));
}

fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

#[allow(clippy::type_complexity)]
fn update_dialog(
    mut commands: Commands,
    edit: Res<SignalEdit>,
    mut sim: ResMut<Sim>,
    assets: Res<AssetServer>,
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    dialog: Single<(Entity, &mut Node, &mut Shown), With<Dialog>>,
) {
    let (entity, mut node, mut shown) = dialog.into_inner();
    let display = if edit.is_open() {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != display {
        node.display = display;
    }
    if !edit.is_open() {
        return;
    }
    let items = choices(&mut sim);
    let mut model: Vec<String> = items
        .iter()
        .map(|&i| signal_code(i, &sim.registry))
        .collect();
    model.push(edit.text.clone());
    model.push(edit.invalid.to_string());
    if shown.0 == model {
        return;
    }
    shown.0 = model;

    let icons: Vec<(Item, Handle<Image>)> = items
        .into_iter()
        .map(|item| {
            let handle = match item {
                Item::Bool(b) => assets.load(if b {
                    "sprites/wires/boolean_true.png"
                } else {
                    "sprites/wires/boolean_false.png"
                }),
                Item::Color(c) => assets.load(format!("sprites/colors/{}.png", c.js_name())),
                Item::Shape(id) => textures.get_or_render(*sim.registry.get(id), &mut images),
            };
            (item, handle)
        })
        .collect();
    commands.entity(entity).despawn_children();
    commands.entity(entity).with_children(|p| {
        p.spawn(text("Signal constant", 22.0, Color::WHITE));
        p.spawn(text(
            "Cliquez sur une valeur, ou tapez un code puis Entree (Echap : annuler).",
            15.0,
            Color::srgb(0.75, 0.75, 0.8),
        ));
        p.spawn(Node {
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(8),
            row_gap: px(8),
            ..default()
        })
        .with_children(|row| {
            for (item, handle) in icons {
                row.spawn((
                    Choice(item),
                    Button,
                    ActivateOnPress,
                    Node {
                        width: px(44),
                        height: px(44),
                        padding: UiRect::all(px(4)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.2, 0.24)),
                ))
                .with_children(|b| {
                    b.spawn((
                        ImageNode::new(handle),
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                    ));
                });
            }
        });
        p.spawn(text(format!("Code : {}_", edit.text), 18.0, Color::WHITE));
        p.spawn(text(
            "1 / 0, true / false, red, green, blue, yellow, purple, cyan, white, uncolored, \
             ou une forme (CuCuCuCu, Sr------:Cg------)",
            14.0,
            Color::srgb(0.6, 0.6, 0.65),
        ));
        if edit.invalid {
            p.spawn(text("Code invalide", 16.0, Color::srgb(0.95, 0.4, 0.4)));
        }
    });
}
