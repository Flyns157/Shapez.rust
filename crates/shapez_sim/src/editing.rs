//! Édition groupée : transactions annulables, sélection de masse et plans
//! (port de `blueprint.js` et de la logique de `hud/parts/mass_selector.js`).
//!
//! Le jeu d'origine n'a pas d'annulation ; ici chaque action du joueur produit une
//! [`Transaction`] (bâtiments posés et retirés) que l'[`History`] sait inverser.

use shapez_core::{Rotation, TilePos};

use crate::{
    SimWorld,
    building::{Building, Layer},
    hub_goals::Reward,
    placement::{BuildingId, CommandError, Placement, SimCommand, SimEvent},
    upgrades::find_nice_integer_value,
};

/// Forme qui paie les plans (`getBlueprintShapeKey`).
pub const BLUEPRINT_SHAPE: &str = "CbCbCbRb:CwCwCwCw";

/// Effet net d'une action sur les bâtiments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transaction {
    /// Bâtiments posés (état final, après orientation automatique).
    pub placed: Vec<(BuildingId, Building)>,
    /// Bâtiments retirés, dans leur état d'avant.
    pub removed: Vec<(BuildingId, Building)>,
}

impl Transaction {
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty() && self.removed.is_empty()
    }

    /// Ajoute une transaction appliquée juste après (une action en plusieurs étapes,
    /// comme un glisser de convoyeurs, s'annule d'un coup).
    pub fn merge(&mut self, next: Transaction) {
        for (id, building) in next.removed {
            match self.placed.iter().position(|(i, _)| *i == id) {
                Some(i) => {
                    self.placed.remove(i);
                }
                None => self.removed.push((id, building)),
            }
        }
        for (id, building) in next.placed {
            match self.placed.iter_mut().find(|(i, _)| *i == id) {
                Some(entry) => entry.1 = building,
                None => self.placed.push((id, building)),
            }
        }
    }
}

impl SimWorld {
    /// Applique des commandes de pose et de suppression, en ignorant celles qui
    /// échouent, et renvoie leur effet net.
    pub fn apply_all(&mut self, commands: impl IntoIterator<Item = SimCommand>) -> Transaction {
        let start = self.events.len();
        for command in commands {
            let _ = self.apply(command);
        }
        let mut tx = Transaction::default();
        for event in &self.events[start..] {
            match *event {
                SimEvent::BuildingPlaced { id, building } => tx.placed.push((id, building)),
                SimEvent::BuildingChanged { id, building } => {
                    if let Some(entry) = tx.placed.iter_mut().find(|(i, _)| *i == id) {
                        entry.1 = building;
                    }
                }
                SimEvent::BuildingRemoved { id, building } => {
                    // Posé puis retiré dans la même action : il n'a jamais existé.
                    match tx.placed.iter().position(|(i, _)| *i == id) {
                        Some(i) => {
                            tx.placed.remove(i);
                        }
                        None => tx.removed.push((id, building)),
                    }
                }
                _ => {}
            }
        }
        tx
    }

    /// Défait une transaction : retire ce qu'elle a posé (s'il est toujours là),
    /// repose ce qu'elle a retiré (si la place est libre). Renvoie la transaction
    /// inverse.
    pub fn revert(&mut self, tx: &Transaction) -> Transaction {
        let mut commands = Vec::new();
        for (id, building) in &tx.placed {
            let layer = building.kind.layer();
            if self.buildings.at_layer(building.origin, layer) == Some(*id) {
                commands.push(SimCommand::Remove {
                    tile: building.origin,
                    layer,
                });
            }
        }
        for (_, building) in &tx.removed {
            commands.push(SimCommand::Place(Placement::of_building(building)));
        }
        self.apply_all(commands)
    }

    /// Bâtiments supprimables d'une couche touchant le rectangle `a`–`b` (bornes
    /// incluses).
    pub fn buildings_in_rect(&self, a: TilePos, b: TilePos, layer: Layer) -> Vec<BuildingId> {
        let (min, max) = (
            TilePos::new(a.x.min(b.x), a.y.min(b.y)),
            TilePos::new(a.x.max(b.x), a.y.max(b.y)),
        );
        self.buildings
            .iter()
            .filter(|(_, building)| building.kind.is_removable() && building.kind.layer() == layer)
            .filter(|(_, building)| {
                let (lo, (w, h)) = building.bounds();
                lo.x <= max.x && lo.x + w > min.x && lo.y <= max.y && lo.y + h > min.y
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// Supprime une sélection (suppression de masse, ou fin d'un « couper »).
    pub fn remove_all(&mut self, ids: &[BuildingId]) -> Transaction {
        let commands: Vec<SimCommand> = ids
            .iter()
            .filter_map(|&id| self.buildings.get(id))
            .map(|b| SimCommand::Remove {
                tile: b.origin,
                layer: b.kind.layer(),
            })
            .collect();
        self.apply_all(commands)
    }

    /// Pose un plan avec son point d'ancrage en `tile`, en payant son coût en formes
    /// de plan (`tryPlace` puis `takeShapeByKey`). Les bâtiments bloqués sont sautés.
    pub fn place_blueprint(
        &mut self,
        blueprint: &Blueprint,
        tile: TilePos,
    ) -> Result<Transaction, CommandError> {
        if !blueprint.can_afford(self) {
            return Err(CommandError::CannotAfford);
        }
        let placements: Vec<Placement> = blueprint
            .placements_at(tile)
            .filter(|p| self.buildings.can_place(p).is_ok())
            .collect();
        if placements.is_empty() {
            return Err(CommandError::Blocked { tile });
        }
        let tx = self.apply_all(placements.into_iter().map(SimCommand::Place));
        let paid = self
            .hub
            .take_by_key(BLUEPRINT_SHAPE, blueprint.cost(), &self.registry);
        debug_assert!(paid, "coût vérifié avant la pose");
        Ok(tx)
    }
}

/// Copie de bâtiments, relative à un point d'ancrage (`Blueprint`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blueprint {
    entries: Vec<Placement>,
}

impl Blueprint {
    /// `Blueprint.fromUids` : l'ancrage est le centre moyen des bâtiments, arrondi
    /// comme dans le JS (`average - 0.5`, puis `floor`).
    pub fn from_buildings(world: &SimWorld, ids: &[BuildingId]) -> Option<Self> {
        let buildings: Vec<&Building> = ids
            .iter()
            .filter_map(|&id| world.buildings.get(id))
            .collect();
        if buildings.is_empty() {
            return None;
        }
        let n = buildings.len() as i64;
        // Somme des doubles des centres, pour rester en entiers.
        let (mut sx, mut sy) = (0i64, 0i64);
        for b in &buildings {
            let (lo, (w, h)) = b.bounds();
            sx += 2 * i64::from(lo.x) + i64::from(w);
            sy += 2 * i64::from(lo.y) + i64::from(h);
        }
        // floor(s / 2n - 1/2) = floor((s - n) / 2n)
        let anchor = TilePos::new(
            (sx - n).div_euclid(2 * n) as i32,
            (sy - n).div_euclid(2 * n) as i32,
        );
        let entries = buildings
            .into_iter()
            .map(|b| {
                let mut p = Placement::of_building(b);
                p.origin = TilePos::new(b.origin.x - anchor.x, b.origin.y - anchor.y);
                p
            })
            .collect();
        Some(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Bâtiments du plan posé avec son ancrage en `tile`.
    pub fn placements_at(&self, tile: TilePos) -> impl Iterator<Item = Placement> + '_ {
        self.entries.iter().map(move |p| Placement {
            origin: TilePos::new(p.origin.x + tile.x, p.origin.y + tile.y),
            ..*p
        })
    }

    /// `rotateCw` : chaque bâtiment tourne, et sa position tourne autour de l'ancrage.
    pub fn rotate_cw(&mut self) {
        for p in &mut self.entries {
            p.rotation = p.rotation.rotate_cw();
            p.original_rotation = p.original_rotation.rotate_cw();
            p.origin = p.origin.rotated(Rotation::R90);
        }
    }

    pub fn rotate_ccw(&mut self) {
        for _ in 0..3 {
            self.rotate_cw();
        }
    }

    /// Coût en formes de plan.
    pub fn cost(&self) -> u64 {
        blueprint_cost(self.entries.len())
    }

    pub fn can_afford(&self, world: &SimWorld) -> bool {
        world.hub.stored_by_key(BLUEPRINT_SHAPE, &world.registry) >= self.cost()
    }

    /// `canPlace` : au moins un bâtiment trouve sa place.
    pub fn can_place_any(&self, world: &SimWorld, tile: TilePos) -> bool {
        self.placements_at(tile)
            .any(|p| world.buildings.can_place(&p).is_ok())
    }
}

/// `Blueprint.getCost` : `findNiceIntegerValue(4 × n^1.1)` pour `n` bâtiments.
pub fn blueprint_cost(n: usize) -> u64 {
    find_nice_integer_value(4.0 * (n as f64).powf(1.1)) as u64
}

/// Copier-coller réservé aux plans débloqués (niveau 12), comme dans le JS.
pub fn blueprints_unlocked(world: &SimWorld) -> bool {
    world.hub.is_reward_unlocked(Reward::Blueprints)
}

/// Piles d'annulation et de rétablissement.
#[derive(Debug, Clone, Default)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
}

impl History {
    /// Nombre d'actions conservées.
    pub const LIMIT: usize = 200;

    /// Enregistre une nouvelle action ; le rétablissement n'a alors plus de sens.
    pub fn record(&mut self, tx: Transaction) {
        if tx.is_empty() {
            return;
        }
        self.redo.clear();
        self.undo.push(tx);
        if self.undo.len() > Self::LIMIT {
            self.undo.remove(0);
        }
    }

    /// Annule la dernière action ; `false` s'il n'y en a pas.
    pub fn undo(&mut self, world: &mut SimWorld) -> bool {
        let Some(tx) = self.undo.pop() else {
            return false;
        };
        let inverse = world.revert(&tx);
        self.redo.push(inverse);
        true
    }

    /// Rétablit la dernière action annulée.
    pub fn redo(&mut self, world: &mut SimWorld) -> bool {
        let Some(tx) = self.redo.pop() else {
            return false;
        };
        let inverse = world.revert(&tx);
        self.undo.push(inverse);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
