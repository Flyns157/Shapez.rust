//! Silhouettes des bâtiments dans la vue d'ensemble (`getSilhouetteColor` et
//! `getSpecialOverlayRenderMatrix` du JS, dessinés par `generateOverlayBuffer`) : chaque
//! tuile vaut 3 × 3 pixels, pleine de la couleur du bâtiment ou dessinée selon un motif.

use shapez_core::Rotation;
use shapez_sim::building::{
    Building, BuildingKind, Variant, belt_variant, tunnel_variant, wire_type,
};

use crate::theme::Theme;

/// `CHUNK_OVERLAY_RES` : pixels par tuile dans l'image de vue d'ensemble.
pub const OVERLAY_RES: usize = 3;

/// Motif 3 × 3, ligne par ligne, avant rotation.
type Matrix = [u8; 9];

/// `rotateFlatMatrix3x3` : un quart de tour dans le sens horaire.
fn rotate(m: Matrix) -> Matrix {
    [m[6], m[3], m[0], m[7], m[4], m[1], m[8], m[5], m[2]]
}

fn rotated(mut m: Matrix, rotation: Rotation) -> Matrix {
    for _ in 0..rotation.quarter_turns() {
        m = rotate(m);
    }
    m
}

fn rgb(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

/// Couleur de silhouette (`getSilhouetteColor`).
pub fn color(building: &Building, theme: &Theme) -> [u8; 3] {
    use BuildingKind as K;
    use Variant as V;
    rgb(match building.kind {
        K::Belt => return theme.overview_belt,
        K::Analyzer => 0x3a52bc,
        K::Balancer => 0x555759,
        K::Comparator => 0x823cab,
        K::ConstantSignal => 0x2b84fd,
        K::Cutter => 0x7dcda2,
        K::Display => 0xaaaaaa,
        K::Filter => 0xc45c2e,
        K::Hub => 0xeb5555,
        K::Lever => 0x1a678b,
        K::LogicGate => match building.variant {
            V::Xor => 0xf4a241,
            V::Or => 0xf4d041,
            V::Not => 0xf44184,
            _ => 0xf48d41,
        },
        K::Miner => 0xb37dcd,
        K::Mixer => 0xcdbb7d,
        K::Painter => 0xcd9b7d,
        K::Reader => 0x25fff2,
        K::Rotater => 0x7dc6cd,
        K::Stacker => 0x9fcd7d,
        K::Storage => 0xbbdf6d,
        K::Transistor => 0xbc3a61,
        K::Trash => 0xed1d5d,
        K::UndergroundBelt => {
            if building.rotation_variant == tunnel_variant::RECEIVER {
                0x71ff9c
            } else {
                0x6d9dff
            }
        }
        // Les couleurs du découpeur, du pivoteur, de l'empileur et du peintre.
        K::VirtualProcessor => match building.variant {
            V::Rotate => 0x7dc6cd,
            V::Stack | V::Unstack => 0x9fcd7d,
            V::Paint => 0xcd9b7d,
            _ => 0x7dcda2,
        },
        K::Wire => 0x61ef6f,
        K::WireTunnel => 0x777a86,
    })
}

/// Motif d'une tuile (`getSpecialOverlayRenderMatrix`), déjà tourné ; `None` : la tuile
/// est pleine.
pub fn matrix(building: &Building) -> Option<Matrix> {
    use BuildingKind as K;
    use Variant as V;
    let m: Matrix = match building.kind {
        K::Belt => match building.rotation_variant {
            belt_variant::LEFT => [0, 0, 0, 1, 1, 0, 0, 1, 0],
            belt_variant::RIGHT => [0, 0, 0, 0, 1, 1, 0, 1, 0],
            _ => [0, 1, 0, 0, 1, 0, 0, 1, 0],
        },
        K::Balancer => match building.variant {
            V::Merger | V::Splitter => [0, 1, 0, 0, 1, 1, 0, 1, 0],
            V::MergerInverse | V::SplitterInverse => [0, 1, 0, 1, 1, 0, 0, 1, 0],
            _ => return None,
        },
        K::UndergroundBelt => {
            if building.rotation_variant == tunnel_variant::RECEIVER {
                [0, 1, 0, 0, 1, 0, 1, 1, 1]
            } else {
                [1, 1, 1, 0, 1, 0, 0, 1, 0]
            }
        }
        K::Transistor => match building.variant {
            V::Mirrored => [0, 1, 0, 0, 1, 1, 0, 1, 0],
            _ => [0, 1, 0, 1, 1, 0, 0, 1, 0],
        },
        K::WireTunnel => [0, 1, 0, 1, 1, 1, 0, 1, 0],
        K::Trash => [1, 1, 0, 1, 1, 1, 0, 1, 1],
        K::Wire => match building.rotation_variant {
            wire_type::SPLIT => [0, 0, 0, 1, 1, 1, 0, 1, 0],
            wire_type::TURN => [0, 0, 0, 0, 1, 1, 0, 1, 0],
            wire_type::CROSS => [0, 1, 0, 1, 1, 1, 0, 1, 0],
            _ => [0, 1, 0, 0, 1, 0, 0, 1, 0],
        },
        K::LogicGate => match building.variant {
            V::Not => [0, 1, 0, 0, 1, 0, 0, 1, 0],
            _ => [0, 1, 0, 1, 1, 1, 0, 1, 1],
        },
        K::Reader => [0, 1, 0, 0, 1, 0, 0, 1, 0],
        K::Rotater => match building.variant {
            V::Ccw => [1, 1, 0, 0, 1, 1, 1, 1, 0],
            V::Rotate180 => [1, 1, 0, 1, 1, 1, 0, 1, 1],
            _ => [0, 1, 1, 1, 1, 0, 0, 1, 1],
        },
        K::Miner => match building.variant {
            V::Chainable => [0, 1, 0, 1, 1, 1, 1, 1, 1],
            _ => [1, 1, 1, 1, 0, 1, 1, 1, 1],
        },
        K::Analyzer => [1, 1, 0, 1, 1, 1, 0, 1, 0],
        K::ConstantSignal => [0, 1, 0, 1, 1, 1, 1, 1, 1],
        _ => return None,
    };
    Some(rotated(m, building.rotation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_matches_generate_matrix_rotations() {
        // Convoyeur droit vers le haut, puis vers la droite : la barre devient horizontale.
        let up = [0, 1, 0, 0, 1, 0, 0, 1, 0];
        assert_eq!(rotate(up), [0, 0, 0, 1, 1, 1, 0, 0, 0]);
        // Virage gauche (entrée en bas, sortie à gauche) tourné de 90° : sortie en haut.
        let left = [0, 0, 0, 1, 1, 0, 0, 1, 0];
        assert_eq!(rotate(left), [0, 1, 0, 1, 1, 0, 0, 0, 0]);
        for m in [up, left] {
            assert_eq!(rotated(m, Rotation::R270), rotate(rotate(rotate(m))));
            assert_eq!(rotate(rotated(m, Rotation::R270)), m);
        }
    }
}
