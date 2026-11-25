//! Simulation du jeu, indépendante de Bevy.
//!
//! Le monde avance par ticks fixes ([`TICKS_PER_SECOND`] par seconde) et toute la
//! logique travaille en entiers, pour rester déterministe (plan, §5.1 et §5.2).

pub mod belt_path;
pub mod building;
pub mod factory;
pub mod hub_goals;
pub mod map;
pub mod placement;
pub mod units;

use building::{Building, BuildingKind};
use factory::{Factory, TickContext};
use hub_goals::HubGoals;
use placement::{BuildingId, Buildings, CommandError, Placement, SimCommand, SimEvent};
use shapez_core::{Rotation, ShapeRegistry, TilePos};
use units::{TICK, Upgrades};

/// Fréquence fixe de la simulation.
pub const TICKS_PER_SECOND: u32 = 60;

/// État complet de la simulation.
#[derive(Debug, Clone)]
pub struct SimWorld {
    tick: u64,
    /// Date de jeu (`root.time.now()`), en unités de [`units::SECOND`].
    time: u64,
    /// Carte (ressources générées depuis la graine).
    pub map: map::Map,
    /// Bâtiments posés et grille d'occupation.
    pub buildings: Buildings,
    /// Convoyeurs, éjecteurs, processeurs… et leurs items.
    pub factory: Factory,
    /// Toutes les formes rencontrées, internées.
    pub registry: ShapeRegistry,
    /// Progression (niveau, formes livrées).
    pub hub: HubGoals,
    pub upgrades: Upgrades,
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
        let mut registry = ShapeRegistry::new();
        let hub = HubGoals::new(&mut registry);
        Self {
            tick: 0,
            time: 0,
            map: map::Map::new(seed),
            buildings: Buildings::default(),
            factory: Factory::default(),
            registry,
            hub,
            upgrades: Upgrades::default(),
            events: Vec::new(),
        }
    }

    /// Nouvelle partie : le hub 4×4 est posé en (-2, -2) comme dans `core.js`.
    pub fn new_game(seed: u32) -> Self {
        let mut world = Self::new(seed);
        let hub = Placement::new(BuildingKind::Hub, TilePos::new(-2, -2), Rotation::R0);
        world
            .apply(SimCommand::Place(hub))
            .expect("la carte est vide");
        world
    }

    /// Applique une commande du joueur. Toute mutation du monde passe par ici.
    pub fn apply(&mut self, command: SimCommand) -> Result<Option<BuildingId>, CommandError> {
        let start = self.events.len();
        let result = match command {
            SimCommand::Place(p) => self.buildings.place(p, &mut self.events).map(Some),
            SimCommand::Remove { tile } => self
                .buildings
                .remove_at(tile, &mut self.events)
                .map(|_| None),
        };
        // Signaux `entityAdded` / `entityDestroyed`, dans l'ordre.
        let mut i = start;
        while i < self.events.len() {
            match self.events[i] {
                SimEvent::BuildingPlaced { id, building } => {
                    self.factory.on_added(id, &self.buildings);
                    self.update_surrounding_belts(&building);
                }
                SimEvent::BuildingRemoved { id, building } => {
                    self.factory.on_removed(id, &building, &self.buildings);
                    self.update_surrounding_belts(&building);
                }
                _ => {}
            }
            i += 1;
        }
        result
    }

    /// `BeltSystem.updateSurroundingBeltPlacement` : réoriente les convoyeurs autour
    /// d'un bâtiment posé ou supprimé, puis prévient leurs chemins.
    fn update_surrounding_belts(&mut self, building: &Building) {
        let (min, (w, h)) = building.bounds();
        let mut touched = Vec::new();
        for x in min.x - 1..=min.x + w {
            for y in min.y - 1..=min.y + h {
                if (min.x..min.x + w).contains(&x) && (min.y..min.y + h).contains(&y) {
                    continue;
                }
                let tile = TilePos::new(x, y);
                let Some((id, belt)) = self.buildings.building_at(tile) else {
                    continue;
                };
                if belt.kind != BuildingKind::Belt {
                    continue;
                }
                let belt = *belt;
                let (rotation, variant) = self.buildings.optimal_belt(tile, belt.original_rotation);
                if rotation != belt.rotation || variant != belt.rotation_variant {
                    let original = self.factory.detach_belt(id, &self.buildings);
                    self.buildings
                        .set_orientation(id, rotation, variant, &mut self.events);
                    self.factory.reattach_belt(id, original, &self.buildings);
                }
                touched.push(id);
            }
        }
        self.factory.notify_paths_of(&touched, &self.buildings);
    }

    /// Dépose un item au début du chemin du convoyeur posé sur `tile`.
    pub fn inject_item(&mut self, tile: TilePos, item: shapez_core::Item) -> bool {
        match self.buildings.at(tile) {
            Some(id) => self.factory.inject(id, item, &self.upgrades),
            None => false,
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

    /// Date de jeu en unités de [`units::SECOND`].
    pub fn time(&self) -> u64 {
        self.time
    }

    /// Fait avancer le monde d'un tick.
    pub fn tick(&mut self) {
        let mut ctx = TickContext {
            buildings: &self.buildings,
            map: &mut self.map,
            registry: &mut self.registry,
            hub: &mut self.hub,
            upgrades: self.upgrades,
            now: self.time,
            events: &mut self.events,
        };
        self.factory.tick(&mut ctx);
        self.time += TICK;
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
