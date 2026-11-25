//! Placement des bâtiments : sélection, fantôme, rotation (R), variantes (T), pipette
//! (Q), pose au clic, glisser de convoyeurs, suppression (X ou clic droit).
//! Port simplifié de `hud/parts/building_placer.js` et `building_placer_logic.js`.

use bevy::prelude::*;
use shapez_core::{Direction, Rotation, TilePos};
use shapez_sim::{
    building::{Building, BuildingKind},
    placement::{Placement, SimCommand},
};

use crate::{
    AppState,
    buildings_view::building_sprite,
    camera::{CameraSet, world_to_tile},
    sim::Sim,
};

/// Barre d'outils : touche, bâtiment (ordre et touches du JS).
pub const TOOLBAR: [(KeyCode, BuildingKind, &str); 10] = [
    (KeyCode::Digit1, BuildingKind::Belt, "Convoyeur"),
    (KeyCode::Digit2, BuildingKind::Balancer, "Equilibreur"),
    (KeyCode::Digit3, BuildingKind::UndergroundBelt, "Tunnel"),
    (KeyCode::Digit4, BuildingKind::Miner, "Extracteur"),
    (KeyCode::Digit5, BuildingKind::Cutter, "Decoupeur"),
    (KeyCode::Digit6, BuildingKind::Rotater, "Rotateur"),
    (KeyCode::Digit7, BuildingKind::Stacker, "Empileur"),
    (KeyCode::Digit8, BuildingKind::Mixer, "Melangeur"),
    (KeyCode::Digit9, BuildingKind::Painter, "Peintre"),
    (KeyCode::Digit0, BuildingKind::Trash, "Poubelle"),
];

/// État du placement en cours.
#[derive(Resource, Debug, Default, Clone)]
pub struct PlacementState {
    /// Bâtiment sélectionné, `None` = mode navigation.
    pub selected: Option<BuildingKind>,
    pub variant_index: usize,
    pub rotation: Rotation,
    /// Tuile sous le curseur.
    pub cursor_tile: Option<TilePos>,
    /// Dernière tuile posée pendant un glisser.
    drag_last: Option<TilePos>,
}

impl PlacementState {
    fn placement_at(&self, sim: &Sim, tile: TilePos) -> Option<Placement> {
        let kind = self.selected?;
        let variant = kind.variants()[self.variant_index % kind.variants().len()];
        let mut p = Placement::new(kind, tile, self.rotation).with_variant(variant);
        match kind {
            BuildingKind::Belt => {
                (p.rotation, p.rotation_variant) = sim.buildings.optimal_belt(tile, self.rotation);
            }
            BuildingKind::UndergroundBelt => {
                (p.rotation, p.rotation_variant) =
                    sim.buildings.optimal_tunnel(tile, self.rotation, variant);
            }
            _ => {}
        }
        Some(p)
    }
}

#[derive(Component, Debug)]
struct Ghost;

#[derive(Component, Debug)]
struct ToolbarText;

#[derive(Debug, Default)]
pub struct PlacementUiPlugin;

impl Plugin for PlacementUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacementState>()
            .add_systems(OnEnter(AppState::InGame), spawn_toolbar)
            .add_systems(
                OnExit(AppState::InGame),
                |mut state: ResMut<PlacementState>| {
                    *state = PlacementState::default();
                },
            )
            .add_systems(
                Update,
                (
                    track_cursor,
                    selection_keys,
                    place_and_remove,
                    update_ghost,
                    update_toolbar,
                )
                    .chain()
                    .after(CameraSet)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn track_cursor(
    mut state: ResMut<PlacementState>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera2d>>,
) {
    let (camera, global) = *camera;
    state.cursor_tile = window
        .cursor_position()
        .and_then(|c| camera.viewport_to_world_2d(global, c).ok())
        .map(|w| {
            let t = world_to_tile(w).floor();
            TilePos::new(t.x as i32, t.y as i32)
        });
}

fn selection_keys(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<Sim>,
    mut state: ResMut<PlacementState>,
) {
    for (key, kind, _) in TOOLBAR {
        if keys.just_pressed(key) {
            // Retaper la même touche désélectionne, comme dans le JS.
            if state.selected == Some(kind) {
                state.selected = None;
            } else {
                state.selected = Some(kind);
                state.variant_index = 0;
            }
        }
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::KeyR) {
        state.rotation = if shift {
            state.rotation.rotate_ccw()
        } else {
            state.rotation.rotate_cw()
        };
    }
    if keys.just_pressed(KeyCode::KeyT)
        && let Some(kind) = state.selected
    {
        state.variant_index = (state.variant_index + 1) % kind.variants().len();
    }
    // Pipette : reprend le bâtiment sous le curseur.
    if keys.just_pressed(KeyCode::KeyQ) {
        let picked = state
            .cursor_tile
            .and_then(|t| sim.buildings.building_at(t))
            .map(|(_, b)| *b)
            .filter(|b| b.kind.is_rotatable());
        match picked {
            Some(b) => {
                state.selected = Some(b.kind);
                state.variant_index = b
                    .kind
                    .variants()
                    .iter()
                    .position(|v| *v == b.variant)
                    .unwrap_or(0);
                state.rotation = b.rotation;
            }
            None => state.selected = None,
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        state.selected = None;
    }
}

fn place_and_remove(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut sim: ResMut<Sim>,
    mut state: ResMut<PlacementState>,
) {
    let Some(tile) = state.cursor_tile else {
        return;
    };

    // `just_pressed` compte aussi : un clic très bref peut commencer et finir dans la
    // même image, auquel cas `pressed` est déjà faux.
    let held = |b: MouseButton| mouse.pressed(b) || mouse.just_pressed(b);
    if held(MouseButton::Right) || keys.pressed(KeyCode::KeyX) {
        let _ = sim.apply(SimCommand::Remove { tile });
        state.drag_last = None;
        return;
    }
    if !held(MouseButton::Left) {
        state.drag_last = None;
        return;
    }
    if state.selected.is_none() {
        return;
    }
    let Some(last) = state
        .drag_last
        .filter(|_| !mouse.just_pressed(MouseButton::Left))
    else {
        if let Some(p) = state.placement_at(&sim, tile) {
            let _ = sim.apply(SimCommand::Place(p));
        }
        state.drag_last = Some(tile);
        return;
    };
    if last == tile {
        return;
    }

    // Glisser : on avance tuile par tuile (X puis Y) et chaque convoyeur pointe vers
    // le suivant, comme le tracé de convoyeurs du JS.
    let is_belt = state.selected == Some(BuildingKind::Belt);
    let mut current = last;
    while current != tile {
        let dir = if current.x != tile.x {
            if tile.x > current.x {
                Direction::Right
            } else {
                Direction::Left
            }
        } else if tile.y > current.y {
            Direction::Bottom
        } else {
            Direction::Top
        };
        if is_belt {
            state.rotation = dir.to_rotation();
            // Réoriente la tuile précédente vers la nouvelle.
            if sim
                .buildings
                .building_at(current)
                .is_some_and(|(_, b)| b.kind == BuildingKind::Belt)
                && let Some(p) = state.placement_at(&sim, current)
            {
                let _ = sim.apply(SimCommand::Place(p));
            }
        }
        current = current.step(dir);
        if let Some(p) = state.placement_at(&sim, current) {
            let _ = sim.apply(SimCommand::Place(p));
        }
    }
    state.drag_last = Some(tile);
}

#[allow(clippy::type_complexity)]
fn update_ghost(
    mut commands: Commands,
    state: Res<PlacementState>,
    sim: Res<Sim>,
    assets: Res<AssetServer>,
    mut ghost: Query<(Entity, &mut Sprite, &mut Transform), With<Ghost>>,
) {
    let placement = state.cursor_tile.and_then(|t| state.placement_at(&sim, t));
    let Some(p) = placement else {
        for (entity, ..) in &ghost {
            commands.entity(entity).despawn();
        }
        return;
    };
    let building = Building {
        kind: p.kind,
        variant: p.variant,
        rotation_variant: p.rotation_variant,
        origin: p.origin,
        rotation: p.rotation,
        original_rotation: p.original_rotation,
    };
    let (mut sprite, transform) = building_sprite(&building, &assets, 50.0);
    sprite.color = if sim.buildings.can_place(&p).is_ok() {
        Color::WHITE.with_alpha(0.6)
    } else {
        Color::srgba(1.0, 0.3, 0.3, 0.6)
    };
    match ghost.single_mut() {
        Ok((_, mut s, mut t)) => {
            *s = sprite;
            *t = transform;
        }
        Err(_) => {
            commands.spawn((Ghost, DespawnOnExit(AppState::InGame), sprite, transform));
        }
    }
}

fn spawn_toolbar(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        ToolbarText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(12),
            ..default()
        },
    ));
}

fn update_toolbar(state: Res<PlacementState>, mut text: Single<&mut Text, With<ToolbarText>>) {
    let mut s = String::new();
    for (i, (_, kind, name)) in TOOLBAR.iter().enumerate() {
        let key = (i + 1) % 10;
        if state.selected == Some(*kind) {
            s.push_str(&format!("[{key} {name}]  "));
        } else {
            s.push_str(&format!("{key} {name}  "));
        }
    }
    if let Some(kind) = state.selected {
        let variant = kind.variants()[state.variant_index % kind.variants().len()];
        s.push_str(&format!(
            "\n{variant:?} - rotation {} (R, T = variante, Q = pipette, X/clic droit = supprimer)",
            state.rotation.degrees()
        ));
    } else {
        s.push_str(
            "\nClic gauche + glisser : deplacer la carte. Q = pipette, X/clic droit = supprimer",
        );
    }
    if text.0 != s {
        text.0 = s;
    }
}
