//! Simulation du jeu, indépendante de Bevy.
//!
//! Le monde avance par ticks fixes ([`TICKS_PER_SECOND`] par seconde) et toute la
//! logique travaille en entiers, pour rester déterministe (plan, §5.1 et §5.2).
//! Les bâtiments, convoyeurs et wires arriveront aux jalons M3 et suivants.

pub mod building;
pub mod map;
pub mod placement;

use placement::{BuildingId, Buildings, CommandError, Placement, SimCommand, SimEvent};
use shapez_core::{Rotation, TilePos};

/// Fréquence fixe de la simulation.
pub const TICKS_PER_SECOND: u32 = 60;

/// État complet de la simulation.
#[derive(Debug, Clone)]
pub struct SimWorld {
    tick: u64,
    /// Carte (ressources générées depuis la graine).
    pub map: map::Map,
    /// Bâtiments posés et grille d'occupation.
    pub buildings: Buildings,
    events: Vec<SimEvent>,
}

impl Default for SimWorld {
    fn default() -> Self {
        Self::new(0)
    }
}

impl SimWorld {
    /// Nouveau monde ; la graine est un entier comme `map.seed` dans le JS.
    pub fn new(seed: u32) -> Self {
        Self {
            tick: 0,
            map: map::Map::new(seed),
            buildings: Buildings::default(),
            events: Vec::new(),
        }
    }

    /// Nouvelle partie : le hub 4×4 est posé en (-2, -2) comme dans `core.js`.
    pub fn new_game(seed: u32) -> Self {
        let mut world = Self::new(seed);
        let hub = Placement::new(
            building::BuildingKind::Hub,
            TilePos::new(-2, -2),
            Rotation::R0,
        );
        world
            .buildings
            .place(hub, &mut world.events)
            .expect("la carte est vide");
        world
    }

    /// Applique une commande du joueur. Toute mutation du monde passe par ici.
    pub fn apply(&mut self, command: SimCommand) -> Result<Option<BuildingId>, CommandError> {
        match command {
            SimCommand::Place(p) => self.buildings.place(p, &mut self.events).map(Some),
            SimCommand::Remove { tile } => self
                .buildings
                .remove_at(tile, &mut self.events)
                .map(|_| None),
        }
    }

    /// Événements produits depuis le dernier appel, dans l'ordre.
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, SimEvent> {
        self.events.drain(..)
    }

    /// Numéro du tick courant (0 au démarrage).
    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    /// Fait avancer le monde d'un tick.
    pub fn tick(&mut self) {
        self.tick += 1;
    }

    /// Fait avancer le monde de `n` ticks (vitesse ×N).
    pub fn advance(&mut self, n: u32) {
        for _ in 0..n {
            self.tick();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_game_places_the_hub_and_reports_it() {
        let mut world = SimWorld::new_game(1);
        assert!(world.buildings.at(TilePos::new(1, 1)).is_some());
        assert!(world.buildings.at(TilePos::new(2, 2)).is_none());
        let events: Vec<_> = world.drain_events().collect();
        assert!(matches!(events[..], [SimEvent::BuildingPlaced { .. }]));
        assert_eq!(world.drain_events().count(), 0);
    }

    #[test]
    fn commands_place_and_remove() {
        let mut world = SimWorld::new(0);
        let p = Placement::new(
            building::BuildingKind::Miner,
            TilePos::new(5, 5),
            Rotation::R90,
        );
        let id = world.apply(SimCommand::Place(p)).unwrap().unwrap();
        assert_eq!(world.buildings.at(TilePos::new(5, 5)), Some(id));
        world
            .apply(SimCommand::Remove {
                tile: TilePos::new(5, 5),
            })
            .unwrap();
        assert_eq!(world.buildings.len(), 0);
        assert_eq!(world.drain_events().count(), 2);
    }

    #[test]
    fn advances_by_whole_ticks() {
        let mut world = SimWorld::new(0);
        world.advance(TICKS_PER_SECOND);
        assert_eq!(world.tick_count(), 60);
    }
}
