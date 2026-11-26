//! Touches réassignables (port de `KEYMAPPINGS` dans `game/key_action_mapper.js` et des
//! surcharges `keybindingOverrides` des paramètres).
//!
//! Les touches sont physiques (`KeyCode`) : la touche à la place du W sur un clavier
//! QWERTY reste « monter » quel que soit l'agencement. Leur libellé suit l'agencement dès
//! que la touche a été tapée une fois (`KeyboardInput::logical_key`) ; avant, c'est le
//! nom QWERTY.
//!
//! Les raccourcis Ctrl (annuler, copier, couper, coller), Échap, Espace (pause), Tab
//! (vitesse), les flèches et X (supprimer) restent fixes.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{OnceLock, RwLock},
};

use bevy::{
    input::{ButtonState, keyboard::KeyboardInput},
    prelude::*,
};
use shapez_sim::building::BuildingKind;

use crate::{i18n, settings::Settings};

/// Catégories de l'écran des touches (`keybindings.categoryLabels`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Ingame,
    Navigation,
    Buildings,
    Placement,
    MassSelect,
}

impl Category {
    pub const ALL: [Self; 5] = [
        Self::Ingame,
        Self::Navigation,
        Self::Buildings,
        Self::Placement,
        Self::MassSelect,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Ingame => "ingame",
            Self::Navigation => "navigation",
            Self::Buildings => "buildings",
            Self::Placement => "placement",
            Self::MassSelect => "massSelect",
        }
    }
}

/// Une action réassignable ; `id()` est l'identifiant du JS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    MenuOpenShop,
    MenuOpenStats,
    ExportScreenshot,
    SwitchLayers,
    MapMoveUp,
    MapMoveRight,
    MapMoveDown,
    MapMoveLeft,
    CreateMarker,
    Building(BuildingKind),
    Pipette,
    RotateWhilePlacing,
    CycleBuildingVariants,
    MassSelectClear,
    ConfirmMassDelete,
}

/// Bâtiments des barres d'outils, dans l'ordre du JS.
const BUILDINGS: [BuildingKind; 23] = {
    use BuildingKind as K;
    [
        K::Belt,
        K::Balancer,
        K::UndergroundBelt,
        K::Miner,
        K::Cutter,
        K::Rotater,
        K::Stacker,
        K::Mixer,
        K::Painter,
        K::Trash,
        K::Storage,
        K::Reader,
        K::Lever,
        K::Filter,
        K::Display,
        K::Wire,
        K::WireTunnel,
        K::ConstantSignal,
        K::LogicGate,
        K::VirtualProcessor,
        K::Analyzer,
        K::Comparator,
        K::Transistor,
    ]
};

impl Action {
    /// Toutes les actions, dans l'ordre de l'écran des touches.
    pub fn all() -> impl Iterator<Item = Self> {
        use Action as A;
        [
            A::MenuOpenShop,
            A::MenuOpenStats,
            A::ExportScreenshot,
            A::SwitchLayers,
            A::MapMoveUp,
            A::MapMoveRight,
            A::MapMoveDown,
            A::MapMoveLeft,
            A::CreateMarker,
        ]
        .into_iter()
        .chain(BUILDINGS.into_iter().map(A::Building))
        .chain([
            A::Pipette,
            A::RotateWhilePlacing,
            A::CycleBuildingVariants,
            A::MassSelectClear,
            A::ConfirmMassDelete,
        ])
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::MenuOpenShop => "menuOpenShop",
            Self::MenuOpenStats => "menuOpenStats",
            Self::ExportScreenshot => "exportScreenshot",
            Self::SwitchLayers => "switchLayers",
            Self::MapMoveUp => "mapMoveUp",
            Self::MapMoveRight => "mapMoveRight",
            Self::MapMoveDown => "mapMoveDown",
            Self::MapMoveLeft => "mapMoveLeft",
            Self::CreateMarker => "createMarker",
            Self::Building(kind) => kind.js_id(),
            Self::Pipette => "pipette",
            Self::RotateWhilePlacing => "rotateWhilePlacing",
            Self::CycleBuildingVariants => "cycleBuildingVariants",
            Self::MassSelectClear => "massSelectClear",
            Self::ConfirmMassDelete => "confirmMassDelete",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::all().find(|a| a.id() == id)
    }

    pub const fn category(self) -> Category {
        match self {
            Self::MenuOpenShop
            | Self::MenuOpenStats
            | Self::ExportScreenshot
            | Self::SwitchLayers => Category::Ingame,
            Self::MapMoveUp
            | Self::MapMoveRight
            | Self::MapMoveDown
            | Self::MapMoveLeft
            | Self::CreateMarker => Category::Navigation,
            Self::Building(_) => Category::Buildings,
            Self::Pipette | Self::RotateWhilePlacing | Self::CycleBuildingVariants => {
                Category::Placement
            }
            Self::MassSelectClear | Self::ConfirmMassDelete => Category::MassSelect,
        }
    }

    /// Touche par défaut du JS.
    pub const fn default_key(self) -> KeyCode {
        use BuildingKind as K;
        use KeyCode as C;
        match self {
            Self::MenuOpenShop => C::KeyF,
            Self::MenuOpenStats => C::KeyG,
            Self::ExportScreenshot => C::F3,
            Self::SwitchLayers => C::KeyE,
            Self::MapMoveUp => C::KeyW,
            Self::MapMoveRight => C::KeyD,
            Self::MapMoveDown => C::KeyS,
            Self::MapMoveLeft => C::KeyA,
            Self::CreateMarker => C::KeyM,
            Self::Building(kind) => match kind {
                K::Belt | K::Wire => C::Digit1,
                K::Balancer | K::WireTunnel => C::Digit2,
                K::UndergroundBelt | K::ConstantSignal => C::Digit3,
                K::Miner | K::LogicGate => C::Digit4,
                K::Cutter | K::VirtualProcessor => C::Digit5,
                K::Rotater | K::Analyzer => C::Digit6,
                K::Stacker | K::Comparator => C::Digit7,
                K::Mixer | K::Transistor => C::Digit8,
                K::Painter => C::Digit9,
                K::Trash => C::Digit0,
                K::Storage => C::KeyY,
                K::Reader => C::KeyU,
                K::Lever => C::KeyI,
                K::Filter => C::KeyO,
                K::Display => C::KeyP,
                // Le hub n'est sur aucune barre d'outils.
                K::Hub => C::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
            },
            Self::Pipette => C::KeyQ,
            Self::RotateWhilePlacing => C::KeyR,
            Self::CycleBuildingVariants => C::KeyT,
            Self::MassSelectClear => C::KeyB,
            Self::ConfirmMassDelete => C::Delete,
        }
    }

    /// Nom affiché (`keybindings.mappings.<id>`).
    pub fn title(self) -> String {
        i18n::t(&format!("keybindings.mappings.{}", self.id()))
    }
}

/// Touches qu'on peut assigner, avec leur nom QWERTY. Les modificateurs, Échap, Entrée,
/// Espace, Tab et les flèches gardent leur rôle fixe.
const ASSIGNABLE: &[(KeyCode, &str)] = {
    use KeyCode as C;
    &[
        (C::KeyA, "A"),
        (C::KeyB, "B"),
        (C::KeyC, "C"),
        (C::KeyD, "D"),
        (C::KeyE, "E"),
        (C::KeyF, "F"),
        (C::KeyG, "G"),
        (C::KeyH, "H"),
        (C::KeyI, "I"),
        (C::KeyJ, "J"),
        (C::KeyK, "K"),
        (C::KeyL, "L"),
        (C::KeyM, "M"),
        (C::KeyN, "N"),
        (C::KeyO, "O"),
        (C::KeyP, "P"),
        (C::KeyQ, "Q"),
        (C::KeyR, "R"),
        (C::KeyS, "S"),
        (C::KeyT, "T"),
        (C::KeyU, "U"),
        (C::KeyV, "V"),
        (C::KeyW, "W"),
        (C::KeyX, "X"),
        (C::KeyY, "Y"),
        (C::KeyZ, "Z"),
        (C::Digit0, "0"),
        (C::Digit1, "1"),
        (C::Digit2, "2"),
        (C::Digit3, "3"),
        (C::Digit4, "4"),
        (C::Digit5, "5"),
        (C::Digit6, "6"),
        (C::Digit7, "7"),
        (C::Digit8, "8"),
        (C::Digit9, "9"),
        (C::F1, "F1"),
        (C::F2, "F2"),
        (C::F3, "F3"),
        (C::F4, "F4"),
        (C::F5, "F5"),
        (C::F6, "F6"),
        (C::F7, "F7"),
        (C::F8, "F8"),
        (C::F9, "F9"),
        (C::F10, "F10"),
        (C::F11, "F11"),
        (C::F12, "F12"),
        (C::Numpad0, "NUM 0"),
        (C::Numpad1, "NUM 1"),
        (C::Numpad2, "NUM 2"),
        (C::Numpad3, "NUM 3"),
        (C::Numpad4, "NUM 4"),
        (C::Numpad5, "NUM 5"),
        (C::Numpad6, "NUM 6"),
        (C::Numpad7, "NUM 7"),
        (C::Numpad8, "NUM 8"),
        (C::Numpad9, "NUM 9"),
        (C::NumpadAdd, "+"),
        (C::NumpadSubtract, "-"),
        (C::NumpadMultiply, "*"),
        (C::NumpadDivide, "/"),
        (C::Delete, "DEL"),
        (C::Insert, "INS"),
        (C::Home, "HOME"),
        (C::End, "END"),
        (C::PageUp, "PGUP"),
        (C::PageDown, "PGDOWN"),
        (C::Backspace, "⌫"),
        (C::Minus, "-"),
        (C::Equal, "="),
        (C::Comma, ","),
        (C::Period, "."),
        (C::Slash, "/"),
        (C::Semicolon, ";"),
        (C::Quote, "'"),
        (C::BracketLeft, "["),
        (C::BracketRight, "]"),
        (C::Backslash, "\\"),
        (C::Backquote, "`"),
        (C::IntlBackslash, "<"),
    ]
};

/// Vrai si la touche peut être assignée à une action.
pub fn is_assignable(key: KeyCode) -> bool {
    ASSIGNABLE.iter().any(|&(k, _)| k == key)
}

/// Nom enregistré dans les paramètres (`KeyW`, `Digit1`, `F3`…).
pub fn key_name(key: KeyCode) -> String {
    format!("{key:?}")
}

pub fn key_from_name(name: &str) -> Option<KeyCode> {
    ASSIGNABLE
        .iter()
        .map(|&(k, _)| k)
        .find(|&k| key_name(k) == name)
}

/// Touches courantes et libellés appris de l'agencement.
#[derive(Debug, Default)]
struct State {
    overrides: HashMap<Action, KeyCode>,
    /// Lettre réellement imprimée sur la touche, apprise à la première frappe.
    learned: HashMap<KeyCode, String>,
}

fn global() -> &'static RwLock<State> {
    static STATE: OnceLock<RwLock<State>> = OnceLock::new();
    STATE.get_or_init(Default::default)
}

/// Surcharges lues des paramètres (identifiant JS → nom de touche) ; les entrées
/// inconnues sont ignorées.
pub fn parse_overrides(map: &BTreeMap<String, String>) -> HashMap<Action, KeyCode> {
    map.iter()
        .filter_map(|(id, key)| Some((Action::from_id(id)?, key_from_name(key)?)))
        .collect()
}

fn set_overrides(overrides: HashMap<Action, KeyCode>) {
    global()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .overrides = overrides;
}

/// Touche de l'action.
pub fn key(action: Action) -> KeyCode {
    global()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .overrides
        .get(&action)
        .copied()
        .unwrap_or_else(|| action.default_key())
}

pub fn just_pressed(keys: &ButtonInput<KeyCode>, action: Action) -> bool {
    keys.just_pressed(key(action))
}

pub fn pressed(keys: &ButtonInput<KeyCode>, action: Action) -> bool {
    keys.pressed(key(action))
}

/// Libellé d'une touche : la lettre de l'agencement si elle est connue, sinon le nom
/// QWERTY (`getStringForKeyCode`).
pub fn key_label(key: KeyCode) -> String {
    let learned = global()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .learned
        .get(&key)
        .cloned();
    learned.unwrap_or_else(|| {
        ASSIGNABLE
            .iter()
            .find(|&&(k, _)| k == key)
            .map_or_else(|| key_name(key), |&(_, name)| name.to_owned())
    })
}

/// Libellé de la touche d'une action.
pub fn label(action: Action) -> String {
    key_label(key(action))
}

/// Retient le caractère d'une touche ; les chiffres et le pavé numérique gardent leur
/// nom (sur un AZERTY, la touche « 1 » donne « & » sans Maj).
fn learn(key: KeyCode, text: &str) {
    let name = key_name(key);
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return;
    };
    if name.starts_with("Digit")
        || name.starts_with("Numpad")
        || c.is_control()
        || c.is_whitespace()
        || !is_assignable(key)
    {
        return;
    }
    let label = c.to_uppercase().to_string();
    let mut state = global().write().unwrap_or_else(|e| e.into_inner());
    if state.learned.get(&key) != Some(&label) {
        state.learned.insert(key, label);
    }
}

/// `tf` avec un paramètre par action (`<pipette>`, `<menuOpenShop>`…) : les textes
/// d'aide affichent les touches courantes.
pub fn tk(key: &str) -> String {
    tk_with(key, &[])
}

/// [`tk`] avec des paramètres en plus.
pub fn tk_with(key: &str, args: &[(&str, &str)]) -> String {
    let labels: Vec<(&'static str, String)> = Action::all()
        .filter(|a| a.category() != Category::Buildings)
        .map(|a| (a.id(), label(a)))
        .collect();
    let mut all: Vec<(&str, &str)> = args.to_vec();
    all.extend(labels.iter().map(|(id, l)| (*id, l.as_str())));
    i18n::tf(key, &all)
}

#[derive(Debug, Default)]
pub struct KeybindingsPlugin;

impl Plugin for KeybindingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, (apply_overrides, learn_labels));
    }
}

fn apply_overrides(settings: Res<Settings>) {
    if settings.is_changed() {
        set_overrides(parse_overrides(&settings.keybindings));
    }
}

fn learn_labels(mut input: MessageReader<KeyboardInput>) {
    for event in input.read() {
        if event.state == ButtonState::Pressed
            && let bevy::input::keyboard::Key::Character(text) = &event.logical_key
        {
            learn(event.key_code, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_round_trip() {
        let mut seen = std::collections::HashSet::new();
        for action in Action::all() {
            assert!(seen.insert(action.id()), "id en double : {}", action.id());
            assert_eq!(Action::from_id(action.id()), Some(action));
            assert!(is_assignable(action.default_key()), "{}", action.id());
        }
        assert_eq!(seen.len(), 9 + 23 + 5);
    }

    #[test]
    fn key_names_round_trip() {
        for &(key, _) in ASSIGNABLE {
            assert_eq!(key_from_name(&key_name(key)), Some(key));
        }
        assert_eq!(key_name(KeyCode::KeyW), "KeyW");
        assert_eq!(key_from_name("ShiftLeft"), None);
    }

    #[test]
    fn unknown_overrides_are_ignored() {
        let map = BTreeMap::from([
            ("pipette".to_owned(), "KeyH".to_owned()),
            ("nope".to_owned(), "KeyA".to_owned()),
            ("belt".to_owned(), "Escape".to_owned()),
        ]);
        let parsed = parse_overrides(&map);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[&Action::Pipette], KeyCode::KeyH);
    }

    #[test]
    fn defaults_follow_the_js_toolbars() {
        assert_eq!(
            Action::Building(BuildingKind::Belt).default_key(),
            KeyCode::Digit1
        );
        assert_eq!(
            Action::Building(BuildingKind::Wire).default_key(),
            KeyCode::Digit1
        );
        assert_eq!(
            Action::Building(BuildingKind::Display).default_key(),
            KeyCode::KeyP
        );
    }
}
