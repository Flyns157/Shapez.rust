//! Statistiques de production (port de `production_analytics.js`).
//!
//! L'historique garde [`SLICES`] tranches de [`SLICE_DURATION`] : la dernière est en
//! cours de remplissage, l'avant-dernière donne les débits « actuels ».

use std::collections::{HashMap, VecDeque};

use shapez_core::ShapeId;

use crate::units::SECOND;

/// `globalConfig.statisticsGraphSlices`.
pub const SLICES: usize = 100;
/// `globalConfig.analyticsSliceDurationSeconds` (hors mode dev).
pub const SLICE_SECONDS: u64 = 10;
pub const SLICE_DURATION: u64 = SLICE_SECONDS * SECOND;

/// `enumAnalyticsDataSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataSource {
    /// Formes extraites ou fabriquées.
    Produced,
    /// Formes en stock dans le hub (copie à chaque tranche).
    Stored,
    /// Formes livrées au hub.
    Delivered,
}

impl DataSource {
    pub const ALL: [Self; 3] = [Self::Produced, Self::Stored, Self::Delivered];

    const fn index(self) -> usize {
        self as usize
    }
}

type Slice = HashMap<ShapeId, u64>;

#[derive(Debug, Clone)]
pub struct Analytics {
    history: [VecDeque<Slice>; 3],
    last_slice: u64,
}

impl Default for Analytics {
    fn default() -> Self {
        let empty: VecDeque<Slice> = std::iter::repeat_with(Slice::new).take(SLICES).collect();
        Self {
            history: [empty.clone(), empty.clone(), empty],
            last_slice: 0,
        }
    }
}

impl Analytics {
    fn bump(&mut self, source: DataSource, shape: ShapeId) {
        let current = self.history[source.index()]
            .back_mut()
            .expect("historique jamais vide");
        *current.entry(shape).or_insert(0) += 1;
    }

    /// Signal `itemProduced` (formes uniquement).
    pub fn on_produced(&mut self, shape: ShapeId) {
        self.bump(DataSource::Produced, shape);
    }

    /// Signal `shapeDelivered`.
    pub fn on_delivered(&mut self, shape: ShapeId) {
        self.bump(DataSource::Delivered, shape);
    }

    /// `update` : ouvre une tranche toutes les [`SLICE_DURATION`].
    pub fn update(&mut self, now: u64, stored: &HashMap<ShapeId, u64>) {
        if now - self.last_slice > SLICE_DURATION {
            self.last_slice = now;
            for source in DataSource::ALL {
                let slices = &mut self.history[source.index()];
                slices.push_back(match source {
                    DataSource::Stored => stored.clone(),
                    _ => Slice::new(),
                });
                while slices.len() > SLICES {
                    slices.pop_front();
                }
            }
        }
    }

    /// `getPastShapeRate` : nombre de formes dans la tranche complète `offset`
    /// (0 = la plus récente), sur [`SLICE_SECONDS`] secondes.
    pub fn past_count(&self, source: DataSource, shape: ShapeId, offset: usize) -> u64 {
        let slices = &self.history[source.index()];
        assert!(offset < SLICES - 1, "tranche invalide : {offset}");
        slices[slices.len() - 2 - offset]
            .get(&shape)
            .copied()
            .unwrap_or(0)
    }

    /// `getCurrentShapeRateRaw`.
    pub fn current_count(&self, source: DataSource, shape: ShapeId) -> u64 {
        self.past_count(source, shape, 0)
    }

    /// `getCurrentShapeRatesRaw` : comptes de la dernière tranche complète, plus les
    /// formes vues dans les dix précédentes (à 0).
    pub fn current_counts(&self, source: DataSource) -> HashMap<ShapeId, u64> {
        let slices = &self.history[source.index()];
        let mut values = slices[slices.len() - 2].clone();
        for i in 0..10 {
            for &shape in slices[slices.len() - i - 3].keys() {
                values.entry(shape).or_insert(0);
            }
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shapez_core::{ShapeDef, ShapeRegistry};

    #[test]
    fn rates_come_from_the_last_complete_slice() {
        let mut reg = ShapeRegistry::new();
        let circle = reg.intern(ShapeDef::from_short_key("CuCuCuCu").unwrap());
        let mut a = Analytics::default();
        let stored = HashMap::new();
        for _ in 0..7 {
            a.on_delivered(circle);
        }
        assert_eq!(a.current_count(DataSource::Delivered, circle), 0);
        // Pile 10 s : pas encore de nouvelle tranche (comparaison stricte du JS).
        a.update(SLICE_DURATION, &stored);
        assert_eq!(a.current_count(DataSource::Delivered, circle), 0);
        a.update(SLICE_DURATION + 1, &stored);
        assert_eq!(a.current_count(DataSource::Delivered, circle), 7);
        assert_eq!(a.current_counts(DataSource::Produced).len(), 0);
    }
}
