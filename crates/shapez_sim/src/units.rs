//! Unités en virgule fixe de la simulation et vitesses dérivées des améliorations.
//!
//! Le JS calcule en flottants (`globalConfig`, `HubGoals.get*BaseSpeed`). On garde les
//! mêmes formules, mais sur des entiers :
//!
//! - une tuile de convoyeur vaut [`TILE`] unités de longueur (espacement des items
//!   `0.63` → [`ITEM_SPACING`], virage `0.78` → [`CURVED_BELT_LENGTH`]) ;
//! - une seconde de jeu vaut [`SECOND`] unités de temps, un tick en vaut [`TICK`] ;
//! - les multiplicateurs d'amélioration sont en dix-millièmes ([`IMPROVEMENT_ONE`] = ×1 ;
//!   le plus petit palier du JS vaut +0.0125).

use crate::TICKS_PER_SECOND;

/// Longueur d'une tuile de convoyeur droit.
pub const TILE: u32 = 10_000;
/// `globalConfig.itemSpacingOnBelts` (0.63 tuile).
pub const ITEM_SPACING: u32 = 6_300;
/// `curvedBeltLength` (0.78 tuile).
pub const CURVED_BELT_LENGTH: u32 = 7_800;

/// Unités de temps par seconde de jeu.
pub const SECOND: u64 = 60_000;
/// Durée d'un tick.
pub const TICK: u64 = SECOND / TICKS_PER_SECOND as u64;

/// Multiplicateur ×1, en dix-millièmes.
pub const IMPROVEMENT_ONE: u64 = 10_000;

/// `globalConfig.beltSpeedItemsPerSecond`.
const BELT_ITEMS_PER_SECOND: u64 = 2;

/// Type de traitement (`enumItemProcessorTypes`), pour les durées.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessorKind {
    Balancer,
    Cutter,
    CutterQuad,
    Rotater,
    RotaterCcw,
    Rotater180,
    Stacker,
    Trash,
    Mixer,
    Painter,
    PainterDouble,
    PainterQuad,
    Hub,
}

/// Multiplicateurs des quatre familles d'améliorations (`upgradeImprovements`),
/// en dix-millièmes (voir [`crate::upgrades`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upgrades {
    pub belt: u64,
    pub miner: u64,
    pub processors: u64,
    pub painting: u64,
}

impl Default for Upgrades {
    fn default() -> Self {
        Self {
            belt: IMPROVEMENT_ONE,
            miner: IMPROVEMENT_ONE,
            processors: IMPROVEMENT_ONE,
            painting: IMPROVEMENT_ONE,
        }
    }
}

impl Upgrades {
    /// Avance d'un convoyeur par tick : `beltSpeed × deltaSeconds × itemSpacing`
    /// (210 unités à ×1).
    pub fn belt_speed_per_tick(&self) -> u32 {
        (BELT_ITEMS_PER_SECOND * u64::from(ITEM_SPACING) * self.belt
            / (TICKS_PER_SECOND as u64 * IMPROVEMENT_ONE)) as u32
    }

    /// Progression d'un slot d'éjection par tick, où [`TILE`] = 1.0
    /// (`2 × deltaSeconds × beltSpeed × itemSpacing`).
    pub fn ejector_progress_per_tick(&self) -> u32 {
        2 * self.belt_speed_per_tick()
    }

    /// Durée d'extraction : `1 / (beltSpeed / 5 × amélioration)`.
    pub fn miner_duration(&self) -> u64 {
        SECOND * IMPROVEMENT_ONE * 5 / (BELT_ITEMS_PER_SECOND * self.miner)
    }

    /// Durée d'une charge (`1 / getProcessorBaseSpeed`).
    pub fn processor_duration(&self, kind: ProcessorKind) -> u64 {
        use ProcessorKind::*;
        // Vitesse = beltSpeed × amélioration / diviseur.
        let (improvement, divisor) = match kind {
            Trash | Hub => return 0,
            Balancer => (self.belt * 2, 1),
            Cutter | CutterQuad => (self.processors, 4),
            Rotater | RotaterCcw | Rotater180 => (self.processors, 1),
            Stacker => (self.processors, 8),
            Mixer => (self.painting, 5),
            Painter => (self.painting, 6),
            PainterDouble => (self.painting, 8),
            PainterQuad => (self.painting, 2),
        };
        SECOND * IMPROVEMENT_ONE * divisor / (BELT_ITEMS_PER_SECOND * improvement)
    }

    /// Temps de traversée d'un tunnel (`UndergroundBeltComponent.tryAcceptTunneledItem`) :
    /// `(distance + 1.5) / beltSpeed / itemSpacing`.
    pub fn tunnel_travel_duration(&self, distance: u32) -> u64 {
        // (2d + 3) / 2 / (2 × amélioration) / 0.63
        SECOND * (2 * u64::from(distance) + 3) * IMPROVEMENT_ONE * 100
            / (2 * BELT_ITEMS_PER_SECOND * self.belt * 63)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_speeds_match_the_js_constants() {
        let u = Upgrades::default();
        // 2 × 0.63 / 60 tuile par tick.
        assert_eq!(u.belt_speed_per_tick(), 210);
        // 0.4 item/s → 2,5 s.
        assert_eq!(u.miner_duration(), 150 * TICK);
        // Découpeur : 2 × 1/4 = 0,5 item/s → 2 s ; rotateur 0,5 s ; équilibreur 0,25 s.
        assert_eq!(u.processor_duration(ProcessorKind::Cutter), 2 * SECOND);
        assert_eq!(u.processor_duration(ProcessorKind::Rotater), SECOND / 2);
        assert_eq!(u.processor_duration(ProcessorKind::Balancer), SECOND / 4);
        assert_eq!(u.processor_duration(ProcessorKind::Hub), 0);
        // (0 + 1,5) / 2 / 0,63 = 1,19 s.
        assert_eq!(u.tunnel_travel_duration(0), 71_428);
    }

    #[test]
    fn upgrades_scale_speeds() {
        let u = Upgrades {
            belt: 2 * IMPROVEMENT_ONE,
            ..Upgrades::default()
        };
        assert_eq!(u.belt_speed_per_tick(), 420);
        assert_eq!(u.processor_duration(ProcessorKind::Balancer), SECOND / 8);
    }
}
