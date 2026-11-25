//! Bâtiments et variantes déverrouillés par la progression
//! (`MetaBuilding.getIsUnlocked` et `getAvailableVariants` des bâtiments du JS).
//!
//! La simulation accepte tout bâtiment (bac à sable, tests) : c'est l'interface qui
//! filtre la barre d'outils avec ces fonctions.

use crate::building::{BuildingKind, Variant};
use crate::hub_goals::{HubGoals, Reward};

/// Le bâtiment apparaît-il dans la barre d'outils ?
pub fn is_unlocked(kind: BuildingKind, hub: &HubGoals) -> bool {
    use BuildingKind::*;
    let reward = match kind {
        Belt | Miner => return true,
        Hub => return false,
        Cutter | Trash => Reward::CutterAndTrash,
        Balancer => Reward::Balancer,
        Rotater => Reward::Rotater,
        UndergroundBelt => Reward::Tunnel,
        Painter => Reward::Painter,
        Mixer => Reward::Mixer,
        Stacker => Reward::Stacker,
        Storage => Reward::Storage,
    };
    hub.is_reward_unlocked(reward)
}

/// Variantes proposées, dans l'ordre de la touche T.
pub fn available_variants(kind: BuildingKind, hub: &HubGoals) -> Vec<Variant> {
    use Variant::*;
    let has = |r| hub.is_reward_unlocked(r);
    let mut out = Vec::new();
    match kind {
        BuildingKind::Cutter => {
            out.push(Default);
            if has(Reward::CutterQuad) {
                out.push(Quad);
            }
        }
        BuildingKind::Balancer => {
            out.push(Default);
            if has(Reward::Merger) {
                out.extend([Merger, MergerInverse]);
            }
            if has(Reward::Splitter) {
                out.extend([Splitter, SplitterInverse]);
            }
        }
        BuildingKind::Rotater => {
            out.push(Default);
            if has(Reward::RotaterCcw) {
                out.push(Ccw);
            }
            if has(Reward::Rotater180) {
                out.push(Rotate180);
            }
        }
        BuildingKind::UndergroundBelt => {
            out.push(Default);
            if has(Reward::UndergroundBeltTier2) {
                out.push(Tier2);
            }
        }
        BuildingKind::Painter => {
            out.extend([Default, Mirrored]);
            if has(Reward::PainterDouble) {
                out.push(Double);
            }
            // Le peintre quadruple a besoin des fils (M8).
            if has(Reward::WiresPainterAndLevers) {
                out.push(Quad);
            }
        }
        // L'extracteur chaînable remplace le normal (`getAvailableVariants`).
        BuildingKind::Miner => out.push(if has(Reward::MinerChainable) {
            Chainable
        } else {
            Default
        }),
        _ => out.push(Default),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shapez_core::ShapeRegistry;

    #[test]
    fn a_new_game_only_has_belts_and_miners() {
        let mut reg = ShapeRegistry::new();
        let hub = HubGoals::new(0, &mut reg);
        let unlocked: Vec<_> = BuildingKind::ALL
            .into_iter()
            .filter(|&k| is_unlocked(k, &hub))
            .collect();
        assert_eq!(unlocked, [BuildingKind::Belt, BuildingKind::Miner]);
        assert_eq!(
            available_variants(BuildingKind::Painter, &hub),
            [Variant::Default, Variant::Mirrored]
        );
    }

    #[test]
    fn rewards_unlock_buildings_and_variants() {
        let mut reg = ShapeRegistry::new();
        let mut hub = HubGoals::new(0, &mut reg);
        hub.grant_reward(Reward::CutterAndTrash);
        hub.grant_reward(Reward::MinerChainable);
        hub.grant_reward(Reward::Splitter);
        assert!(is_unlocked(BuildingKind::Trash, &hub));
        assert!(!is_unlocked(BuildingKind::Balancer, &hub));
        assert_eq!(
            available_variants(BuildingKind::Miner, &hub),
            [Variant::Chainable]
        );
        assert_eq!(
            available_variants(BuildingKind::Balancer, &hub),
            [
                Variant::Default,
                Variant::Splitter,
                Variant::SplitterInverse
            ]
        );
    }
}
