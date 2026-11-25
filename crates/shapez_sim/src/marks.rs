//! Repères de la carte et formes épinglées : de l'état du joueur, sauvegardé avec la
//! partie (`HUDWaypoints` et `HUDPinnedShapes` du jeu d'origine).

use shapez_core::ShapeId;

/// Repère nommé (`Waypoint` : libellé, centre et zoom).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Waypoint {
    pub label: String,
    /// Centre, en tuiles.
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Marks {
    pub waypoints: Vec<Waypoint>,
    /// Formes épinglées, dans l'ordre d'ajout (l'objectif courant s'affiche à part).
    pub pinned: Vec<ShapeId>,
}

impl Marks {
    /// Épingle ou désépingle une forme ; renvoie `true` si elle est maintenant épinglée.
    pub fn toggle_pin(&mut self, shape: ShapeId) -> bool {
        match self.pinned.iter().position(|&s| s == shape) {
            Some(i) => {
                self.pinned.remove(i);
                false
            }
            None => {
                self.pinned.push(shape);
                true
            }
        }
    }

    pub fn is_pinned(&self, shape: ShapeId) -> bool {
        self.pinned.contains(&shape)
    }

    /// Premier libellé libre « Repere N ».
    pub fn fresh_label(&self) -> String {
        (1..)
            .map(|n| format!("Repere {n}"))
            .find(|l| self.waypoints.iter().all(|w| &w.label != l))
            .expect("un libellé libre finit par exister")
    }
}
