//! Repères (M par défaut pour en poser un, clic pour y aller, Maj+clic pour le retirer) et formes
//! épinglées (objectif courant, formes de plan, formes choisies dans le shop) :
//! `hud/parts/waypoints.js` et `hud/parts/pinned_shapes.js`, simplifiés.

use bevy::{
    prelude::*,
    ui_widgets::{Activate, ActivateOnPress, Button},
};
use shapez_core::{ShapeDef, ShapeId};
use shapez_sim::{
    editing::{BLUEPRINT_SHAPE, blueprints_unlocked},
    marks::Waypoint,
    upgrades::UpgradeKind,
};

use crate::{
    AppState,
    camera::{MainCamera, Zoom, tile_to_world, world_to_tile},
    i18n::t,
    keybindings::{Action, just_pressed, tk},
    shape_textures::ShapeTextures,
    sim::Sim,
    theme::ThemedText,
};

#[derive(Debug, Default)]
pub struct MarksUiPlugin;

impl Plugin for MarksUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_panel)
            .add_observer(on_activate)
            .add_systems(
                Update,
                (add_waypoint, update_panel)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Component, Debug)]
struct MarksPanel;

#[derive(Component, Debug, Default, PartialEq)]
struct Shown(Vec<String>);

/// `None` : le hub.
#[derive(Component, Debug, Clone, Copy)]
struct WaypointButton(Option<usize>);

#[derive(Component, Debug, Clone, Copy)]
struct PinnedRow(ShapeId);

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        MarksPanel,
        Shown::default(),
        Node {
            position_type: PositionType::Absolute,
            top: px(70),
            right: px(12),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::End,
            row_gap: px(4),
            ..default()
        },
    ));
}

fn add_waypoint(
    keys: Res<ButtonInput<KeyCode>>,
    zoom: Res<Zoom>,
    camera: Single<&Transform, With<MainCamera>>,
    mut sim: ResMut<Sim>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if ctrl || !just_pressed(&keys, Action::CreateMarker) {
        return;
    }
    let center = world_to_tile(camera.translation.truncate());
    let label = sim.marks.fresh_label();
    sim.marks.waypoints.push(Waypoint {
        label,
        x: center.x,
        y: center.y,
        zoom: zoom.0,
    });
}

fn on_activate(
    activate: On<Activate>,
    waypoints: Query<&WaypointButton>,
    pinned: Query<&PinnedRow>,
    keys: Res<ButtonInput<KeyCode>>,
    sim: Option<ResMut<Sim>>,
    mut zoom: ResMut<Zoom>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
) {
    let Some(mut sim) = sim else {
        return;
    };
    let target = activate.event_target();
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if let Ok(&WaypointButton(index)) = waypoints.get(target) {
        match index {
            Some(i) if shift => {
                if i < sim.marks.waypoints.len() {
                    sim.marks.waypoints.remove(i);
                }
            }
            Some(i) => {
                if let Some(w) = sim.marks.waypoints.get(i) {
                    for mut t in &mut camera {
                        t.translation = tile_to_world(w.x, w.y).extend(t.translation.z);
                    }
                    zoom.0 = w.zoom;
                }
            }
            None => {
                for mut t in &mut camera {
                    t.translation = tile_to_world(0.0, 0.0).extend(t.translation.z);
                }
            }
        }
    } else if let Ok(&PinnedRow(shape)) = pinned.get(target)
        && shift
    {
        sim.marks.toggle_pin(shape);
    }
}

/// Quantité demandée pour une forme par le prochain palier d'une amélioration.
fn upgrade_need(sim: &Sim, shape: ShapeId) -> Option<u64> {
    let key = sim.registry.get(shape).short_key();
    UpgradeKind::ALL.into_iter().find_map(|kind| {
        sim.hub
            .next_tier(kind)?
            .required
            .iter()
            .find(|(k, _)| *k == key)
            .map(|&(_, amount)| amount)
    })
}

fn label(text: impl Into<String>, size: f32) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
    )
}

fn update_panel(
    mut commands: Commands,
    sim: Res<Sim>,
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    panel: Single<(Entity, &mut Shown), With<MarksPanel>>,
) {
    // Lignes de formes : (forme, texte, épinglée par le joueur).
    let goal = sim.hub.goal();
    let mut rows: Vec<(ShapeId, String, bool)> = vec![(
        goal.shape,
        format!(
            "{} / {}{}",
            sim.hub.goal_delivered(),
            goal.required,
            if goal.throughput_only { " /s" } else { "" }
        ),
        false,
    )];
    if blueprints_unlocked(&sim)
        && let Some(id) = sim
            .registry
            .find(&ShapeDef::from_short_key(BLUEPRINT_SHAPE).expect("forme de plan valide"))
    {
        rows.push((id, sim.hub.stored(id).to_string(), false));
    }
    for &shape in &sim.marks.pinned {
        let stored = sim.hub.stored(shape);
        let text = match upgrade_need(&sim, shape) {
            Some(need) => format!("{stored} / {need}"),
            None => stored.to_string(),
        };
        rows.push((shape, text, true));
    }
    let mut model: Vec<String> = sim
        .marks
        .waypoints
        .iter()
        .map(|w| w.label.clone())
        .collect();
    model.extend(rows.iter().map(|(s, t, p)| format!("{s:?}:{t}:{p}")));
    let (entity, mut shown) = panel.into_inner();
    if shown.0 == model {
        return;
    }
    shown.0 = model;

    let waypoints: Vec<String> = sim
        .marks
        .waypoints
        .iter()
        .map(|w| w.label.clone())
        .collect();
    let rows: Vec<(ShapeId, Handle<Image>, String, bool)> = rows
        .into_iter()
        .map(|(id, text, pinned)| {
            let handle = textures.get_or_render(*sim.registry.get(id), &mut images);
            (id, handle, text, pinned)
        })
        .collect();

    let button_bg = BackgroundColor(Color::srgba(0.15, 0.15, 0.2, 0.85));
    commands
        .entity(entity)
        .despawn_children()
        .with_children(|p| {
            p.spawn(label(
                format!(
                    "{} {}",
                    t("ingame.waypoints.waypoints"),
                    tk("port.marks.help")
                ),
                14.0,
            ))
            .insert(ThemedText);
            let mut entries = vec![(None, t("ingame.waypoints.hub"))];
            entries.extend(waypoints.into_iter().enumerate().map(|(i, l)| (Some(i), l)));
            for (index, text) in entries {
                p.spawn((
                    WaypointButton(index),
                    Button,
                    ActivateOnPress,
                    Node {
                        padding: UiRect::axes(px(10), px(4)),
                        ..default()
                    },
                    button_bg,
                    children![label(text, 16.0)],
                ));
            }
            p.spawn((
                label(t("port.marks.pinned"), 14.0),
                ThemedText,
                Node {
                    margin: UiRect::top(px(10)),
                    ..default()
                },
            ));
            for (id, handle, text, pinned) in rows {
                let mut row = p.spawn((
                    Node {
                        align_items: AlignItems::Center,
                        column_gap: px(6),
                        ..default()
                    },
                    children![
                        (label(text, 16.0), ThemedText),
                        (
                            ImageNode::new(handle),
                            Node {
                                width: px(32),
                                height: px(32),
                                ..default()
                            },
                        ),
                    ],
                ));
                if pinned {
                    row.insert((PinnedRow(id), Button, ActivateOnPress));
                }
            }
        });
}
