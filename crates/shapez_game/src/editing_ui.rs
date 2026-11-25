//! Édition de masse : annuler et rétablir (Ctrl+Z, Ctrl+Y), sélection au Ctrl+glisser
//! (Maj pour ajouter), suppression (Suppr ou X), copier (Ctrl+C), couper (Ctrl+X),
//! coller (Ctrl+V) avec rotation (R) ; port simplifié de `hud/parts/mass_selector.js`
//! et `hud/parts/blueprint_placer.js`.
//!
//! Les raccourcis à lettre suivent la touche logique : Ctrl+Z annule aussi sur un
//! clavier AZERTY.

use bevy::{input::keyboard::Key, prelude::*};
use shapez_core::TilePos;
use shapez_sim::{
    editing::{BLUEPRINT_SHAPE, Blueprint, History, Transaction, blueprints_unlocked},
    placement::{BuildingId, CommandError},
};

use crate::{
    AppState,
    buildings_view::building_sprite,
    camera::{CameraSet, TILE_SIZE, tile_to_world},
    placement_ui::PlacementState,
    shop::OpenPanel,
    sim::Sim,
};

/// Durée d'affichage d'un message de l'éditeur, en secondes.
const MESSAGE_SECONDS: f32 = 4.0;
const SELECTION_Z: f32 = 20.0;

/// État de l'édition de masse.
#[derive(Resource, Debug, Default)]
pub struct Editor {
    pub history: History,
    /// Action en cours (glisser de convoyeurs, suppression maintenue), enregistrée
    /// d'un bloc quand les boutons sont relâchés.
    pub pending: Transaction,
    pub selection: Vec<BuildingId>,
    /// Plan en cours de pose.
    pub blueprint: Option<Blueprint>,
    /// Dernier plan copié, pour Ctrl+V.
    last_blueprint: Option<Blueprint>,
    /// Coin de départ du rectangle de sélection.
    select_start: Option<TilePos>,
    pub message: Option<(String, Timer)>,
    /// Un clic a servi à l'édition : le placement l'ignore jusqu'au relâchement.
    pub swallow: bool,
    /// Incrémenté à chaque changement de sélection ou de plan (rendu).
    version: u64,
}

impl Editor {
    /// Une sélection ou un plan est actif : clics et Échap lui reviennent.
    pub fn is_busy(&self) -> bool {
        !self.selection.is_empty() || self.blueprint.is_some() || self.select_start.is_some()
    }

    fn say(&mut self, text: impl Into<String>) {
        self.message = Some((
            text.into(),
            Timer::from_seconds(MESSAGE_SECONDS, TimerMode::Once),
        ));
    }

    fn flush(&mut self) {
        let tx = std::mem::take(&mut self.pending);
        self.history.record(tx);
    }

    fn touch(&mut self) {
        self.version += 1;
    }
}

/// Systèmes d'édition ; le placement de bâtiments passe après.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditingSet;

#[derive(Component, Debug)]
struct SelectionMarker;

#[derive(Component, Debug)]
struct BlueprintGhost;

#[derive(Debug, Default)]
pub struct EditingUiPlugin;

impl Plugin for EditingUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Editor>()
            .add_systems(OnExit(AppState::InGame), |mut editor: ResMut<Editor>| {
                *editor = Editor::default();
            })
            .add_systems(
                Update,
                (
                    undo_keys,
                    mass_select,
                    selection_actions,
                    place_blueprint,
                    flush_pending,
                    tick_message,
                )
                    .chain()
                    .in_set(EditingSet)
                    .after(CameraSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                PostUpdate,
                (draw_selection, draw_blueprint).run_if(in_state(AppState::InGame)),
            );
    }
}

fn ctrl(keys: &ButtonInput<KeyCode>) -> bool {
    // Un raccourci tapé très vite peut relâcher Ctrl dans la même image que la lettre.
    const CTRL: [KeyCode; 4] = [
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ];
    keys.any_pressed(CTRL) || keys.any_just_released(CTRL)
}

fn shift(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
}

/// Touche logique `c` (minuscule ou majuscule) enfoncée à cette image.
pub fn letter(keys: &ButtonInput<Key>, c: char) -> bool {
    let lower = Key::Character(c.to_ascii_lowercase().to_string().into());
    let upper = Key::Character(c.to_ascii_uppercase().to_string().into());
    keys.just_pressed(lower) || keys.just_pressed(upper)
}

fn undo_keys(
    keys: Res<ButtonInput<KeyCode>>,
    letters: Res<ButtonInput<Key>>,
    mut sim: ResMut<Sim>,
    mut editor: ResMut<Editor>,
) {
    if !ctrl(&keys) {
        return;
    }
    let redo = letter(&letters, 'y') || (shift(&keys) && letter(&letters, 'z'));
    let undo = !redo && letter(&letters, 'z');
    if !(undo || redo) {
        return;
    }
    editor.flush();
    editor.selection.clear();
    editor.touch();
    let done = if undo {
        editor.history.undo(&mut sim)
    } else {
        editor.history.redo(&mut sim)
    };
    if !done {
        editor.say(if undo {
            "Rien a annuler"
        } else {
            "Rien a retablir"
        });
    }
}

fn mass_select(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    panel: Res<OpenPanel>,
    placement: Res<PlacementState>,
    sim: Res<Sim>,
    mut editor: ResMut<Editor>,
) {
    let Some(tile) = placement.cursor_tile else {
        return;
    };
    if mouse.just_pressed(MouseButton::Left)
        && ctrl(&keys)
        && editor.blueprint.is_none()
        && !panel.is_open()
    {
        editor.select_start = Some(tile);
        editor.swallow = true;
        editor.touch();
    }
    let Some(start) = editor.select_start else {
        return;
    };
    if mouse.pressed(MouseButton::Left) {
        editor.touch();
        return;
    }
    // Relâché : la sélection remplace l'ancienne, ou s'y ajoute avec Maj.
    editor.select_start = None;
    let found = sim.buildings_in_rect(start, tile);
    if !shift(&keys) {
        editor.selection.clear();
    }
    for id in found {
        if let Some(i) = editor.selection.iter().position(|&s| s == id) {
            // Maj+clic sur un bâtiment déjà choisi le retire.
            if start == tile {
                editor.selection.remove(i);
            }
        } else {
            editor.selection.push(id);
        }
    }
    editor.touch();
}

fn selection_actions(
    keys: Res<ButtonInput<KeyCode>>,
    letters: Res<ButtonInput<Key>>,
    mut sim: ResMut<Sim>,
    mut editor: ResMut<Editor>,
    mut placement: ResMut<PlacementState>,
) {
    // Les bâtiments supprimés entre-temps sortent de la sélection.
    let before = editor.selection.len();
    editor
        .selection
        .retain(|&id| sim.buildings.get(id).is_some());
    if editor.selection.len() != before {
        editor.touch();
    }

    let ctrl = ctrl(&keys);
    if ctrl && letter(&letters, 'v') {
        match editor.last_blueprint.clone() {
            Some(bp) => {
                editor.blueprint = Some(bp);
                editor.selection.clear();
                placement.selected = None;
            }
            None => editor.say("Aucun plan copie"),
        }
        editor.touch();
        return;
    }
    if editor.selection.is_empty() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        editor.selection.clear();
        editor.touch();
        return;
    }
    let delete = !ctrl && keys.any_just_pressed([KeyCode::Delete, KeyCode::KeyX]);
    let copy = ctrl && letter(&letters, 'c');
    let cut = ctrl && letter(&letters, 'x');
    if (copy || cut) && !blueprints_unlocked(&sim) {
        editor.say("Les plans (copier-coller) se debloquent au niveau 12");
        return;
    }
    if copy || cut {
        let bp = Blueprint::from_buildings(&sim, &editor.selection);
        editor.last_blueprint = bp.clone();
        editor.blueprint = bp;
        placement.selected = None;
    }
    if delete || cut {
        editor.flush();
        let selection = std::mem::take(&mut editor.selection);
        let tx = sim.remove_all(&selection);
        editor.history.record(tx);
    }
    if delete || cut || copy {
        editor.selection.clear();
        editor.touch();
    }
}

fn place_blueprint(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    panel: Res<OpenPanel>,
    placement: Res<PlacementState>,
    mut sim: ResMut<Sim>,
    mut editor: ResMut<Editor>,
) {
    if editor.blueprint.is_none() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right) {
        editor.blueprint = None;
        editor.swallow = true;
        editor.touch();
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        let ccw = shift(&keys);
        if let Some(bp) = editor.blueprint.as_mut() {
            if ccw {
                bp.rotate_ccw();
            } else {
                bp.rotate_cw();
            }
        }
        editor.touch();
    }
    let Some(tile) = placement.cursor_tile else {
        return;
    };
    if !mouse.just_pressed(MouseButton::Left) || panel.is_open() {
        return;
    }
    editor.swallow = true;
    let bp = editor.blueprint.clone().expect("plan actif");
    match sim.place_blueprint(&bp, tile) {
        Ok(tx) => {
            editor.flush();
            editor.history.record(tx);
        }
        Err(CommandError::CannotAfford) => editor.say(format!(
            "Pas assez de formes de plan : {} requises ({BLUEPRINT_SHAPE})",
            bp.cost()
        )),
        Err(_) => editor.say("Aucun batiment du plan ne peut etre pose ici"),
    }
}

/// Une action à plusieurs étapes s'enregistre quand plus rien n'est maintenu.
fn flush_pending(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut editor: ResMut<Editor>,
) {
    let held =
        mouse.any_pressed([MouseButton::Left, MouseButton::Right]) || keys.pressed(KeyCode::KeyX);
    if !held {
        editor.swallow = false;
        if !editor.pending.is_empty() {
            editor.flush();
        }
    }
}

fn tick_message(time: Res<Time>, mut editor: ResMut<Editor>) {
    let done = editor
        .message
        .as_mut()
        .is_some_and(|(_, timer)| timer.tick(time.delta()).just_finished());
    if done {
        editor.message = None;
    }
}

/// Rectangle couvrant les tuiles `min`..=`max`.
fn tile_rect(min: TilePos, max: TilePos, color: Color, z: f32) -> (Sprite, Transform) {
    let w = (max.x - min.x + 1) as f32;
    let h = (max.y - min.y + 1) as f32;
    let center = tile_to_world(min.x as f32 + w / 2.0, min.y as f32 + h / 2.0);
    (
        Sprite::from_color(color, Vec2::new(w, h) * TILE_SIZE),
        Transform::from_translation(center.extend(z)),
    )
}

fn draw_selection(
    mut commands: Commands,
    editor: Res<Editor>,
    sim: Res<Sim>,
    placement: Res<PlacementState>,
    markers: Query<Entity, With<SelectionMarker>>,
    mut drawn: Local<Option<(u64, Option<TilePos>)>>,
) {
    let key = (editor.version, placement.cursor_tile);
    if *drawn == Some(key) {
        return;
    }
    *drawn = Some(key);
    for e in &markers {
        commands.entity(e).despawn();
    }
    let selected = Color::srgba(0.3, 0.6, 1.0, 0.35);
    for &id in &editor.selection {
        if let Some(b) = sim.buildings.get(id) {
            let (lo, (w, h)) = b.bounds();
            let max = TilePos::new(lo.x + w - 1, lo.y + h - 1);
            commands.spawn((
                SelectionMarker,
                DespawnOnExit(AppState::InGame),
                tile_rect(lo, max, selected, SELECTION_Z),
            ));
        }
    }
    if let (Some(start), Some(end)) = (editor.select_start, placement.cursor_tile) {
        let min = TilePos::new(start.x.min(end.x), start.y.min(end.y));
        let max = TilePos::new(start.x.max(end.x), start.y.max(end.y));
        commands.spawn((
            SelectionMarker,
            DespawnOnExit(AppState::InGame),
            tile_rect(min, max, Color::srgba(0.3, 0.6, 1.0, 0.18), SELECTION_Z),
        ));
    }
}

fn draw_blueprint(
    mut commands: Commands,
    editor: Res<Editor>,
    sim: Res<Sim>,
    placement: Res<PlacementState>,
    assets: Res<AssetServer>,
    ghosts: Query<Entity, With<BlueprintGhost>>,
    mut drawn: Local<Option<(u64, Option<TilePos>, u64)>>,
) {
    let key = (editor.version, placement.cursor_tile, sim.tick_count() / 30);
    if *drawn == Some(key) {
        return;
    }
    *drawn = Some(key);
    for e in &ghosts {
        commands.entity(e).despawn();
    }
    let (Some(bp), Some(tile)) = (&editor.blueprint, placement.cursor_tile) else {
        return;
    };
    let affordable = bp.can_afford(&sim);
    for p in bp.placements_at(tile) {
        let building = shapez_sim::building::Building {
            kind: p.kind,
            variant: p.variant,
            rotation_variant: p.rotation_variant,
            origin: p.origin,
            rotation: p.rotation,
            original_rotation: p.original_rotation,
        };
        let (mut sprite, transform) = building_sprite(&building, &assets, SELECTION_Z + 1.0);
        sprite.color = if affordable && sim.buildings.can_place(&p).is_ok() {
            Color::WHITE.with_alpha(0.7)
        } else {
            Color::srgba(1.0, 0.3, 0.3, 0.5)
        };
        commands.spawn((
            BlueprintGhost,
            DespawnOnExit(AppState::InGame),
            sprite,
            transform,
        ));
    }
}
