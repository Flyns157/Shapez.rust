//! Bâtiments posés sur la carte : arène, grille d'occupation, commandes de placement
//! et de suppression, orientation automatique des convoyeurs et tunnels.
//!
//! Toute mutation passe par une [`SimCommand`] (plan, §3.4) ; les bâtiments vivent
//! dans une arène à index générationnels (plan, §3.3).

use std::collections::HashMap;

use shapez_core::{Direction, Rotation, TilePos};
use slotmap::{SlotMap, new_key_type};

use crate::{
    building::{
        AcceptorSlot, Building, BuildingKind, EjectorSlot, Variant, belt_variant, tunnel_range,
        tunnel_variant,
    },
    map::{CHUNK_SIZE, ChunkPos},
};

new_key_type! {
    /// Identifiant générationnel d'un bâtiment : un index périmé ne pointe jamais
    /// vers un autre bâtiment.
    pub struct BuildingId;
}

type ChunkGrid = [[Option<BuildingId>; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];

/// Grille d'occupation de la couche normale, par chunk (les wires viendront en M8).
#[derive(Debug, Clone, Default)]
pub struct OccupancyGrid {
    chunks: HashMap<ChunkPos, Box<ChunkGrid>>,
}

impl OccupancyGrid {
    fn split(tile: TilePos) -> (ChunkPos, usize, usize) {
        let chunk = ChunkPos::of_tile(tile);
        let local = tile - chunk.origin();
        (chunk, local.x as usize, local.y as usize)
    }

    pub fn get(&self, tile: TilePos) -> Option<BuildingId> {
        let (chunk, x, y) = Self::split(tile);
        self.chunks.get(&chunk).and_then(|g| g[x][y])
    }

    fn set(&mut self, tile: TilePos, value: Option<BuildingId>) {
        let (chunk, x, y) = Self::split(tile);
        if value.is_none() && !self.chunks.contains_key(&chunk) {
            return;
        }
        self.chunks
            .entry(chunk)
            .or_insert_with(|| Box::new([[None; CHUNK_SIZE as usize]; CHUNK_SIZE as usize]))[x]
            [y] = value;
    }
}

/// Demande de placement d'un bâtiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub kind: BuildingKind,
    pub variant: Variant,
    pub origin: TilePos,
    pub rotation: Rotation,
    pub rotation_variant: u8,
    /// Rotation demandée par le joueur avant orientation automatique.
    pub original_rotation: Rotation,
}

impl Placement {
    pub fn new(kind: BuildingKind, origin: TilePos, rotation: Rotation) -> Self {
        Self {
            kind,
            variant: Variant::Default,
            origin,
            rotation,
            rotation_variant: 0,
            original_rotation: rotation,
        }
    }

    pub fn with_variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    fn building(&self) -> Building {
        Building {
            kind: self.kind,
            variant: self.variant,
            rotation_variant: self.rotation_variant,
            origin: self.origin,
            rotation: self.rotation,
            original_rotation: self.original_rotation,
        }
    }
}

/// Mutation du monde demandée par le joueur (ou par un test, un replay…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimCommand {
    Place(Placement),
    /// Supprime le bâtiment qui occupe la tuile.
    Remove {
        tile: TilePos,
    },
}

/// Événement produit par la simulation, consommé par le rendu, l'audio…
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimEvent {
    BuildingPlaced {
        id: BuildingId,
        building: Building,
    },
    BuildingRemoved {
        id: BuildingId,
        building: Building,
    },
    /// Convoyeur réorienté automatiquement après la pose ou la suppression d'un voisin.
    BuildingChanged {
        id: BuildingId,
        building: Building,
    },
    /// Objectif du hub atteint.
    LevelCompleted(crate::hub_goals::LevelCompleted),
}

/// Raison d'un refus de commande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    /// Une tuile est occupée par un bâtiment non remplaçable.
    Blocked { tile: TilePos },
    /// Variante inconnue pour ce type de bâtiment.
    InvalidVariant,
    /// Rien à supprimer sur cette tuile.
    NothingThere,
    /// Bâtiment non supprimable (hub).
    NotRemovable,
}

/// Bâtiments du monde et leur occupation de la grille.
#[derive(Debug, Clone, Default)]
pub struct Buildings {
    arena: SlotMap<BuildingId, Building>,
    grid: OccupancyGrid,
}

impl Buildings {
    pub fn get(&self, id: BuildingId) -> Option<&Building> {
        self.arena.get(id)
    }

    pub fn at(&self, tile: TilePos) -> Option<BuildingId> {
        self.grid.get(tile)
    }

    pub fn building_at(&self, tile: TilePos) -> Option<(BuildingId, &Building)> {
        let id = self.grid.get(tile)?;
        Some((id, &self.arena[id]))
    }

    /// Bâtiments dans l'ordre des slots de l'arène (déterministe).
    pub fn iter(&self) -> impl Iterator<Item = (BuildingId, &Building)> {
        self.arena.iter()
    }

    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// `checkCanPlaceEntity`.
    pub fn can_place(&self, placement: &Placement) -> Result<(), CommandError> {
        if !placement.kind.has_variant(placement.variant) {
            return Err(CommandError::InvalidVariant);
        }
        for tile in placement.building().tiles() {
            if let Some((_, other)) = self.building_at(tile)
                && !other.kind.is_replaceable()
            {
                return Err(CommandError::Blocked { tile });
            }
        }
        Ok(())
    }

    /// `tryPlaceBuilding` : libère les convoyeurs recouverts, puis pose le bâtiment.
    pub fn place(
        &mut self,
        placement: Placement,
        events: &mut Vec<SimEvent>,
    ) -> Result<BuildingId, CommandError> {
        self.can_place(&placement)?;
        let building = placement.building();
        for tile in building.tiles() {
            if let Some(old) = self.grid.get(tile) {
                self.remove_id(old, events);
            }
        }
        let id = self.arena.insert(building);
        for tile in building.tiles() {
            self.grid.set(tile, Some(id));
        }
        events.push(SimEvent::BuildingPlaced { id, building });
        Ok(id)
    }

    /// `tryDeleteBuilding`.
    pub fn remove_at(
        &mut self,
        tile: TilePos,
        events: &mut Vec<SimEvent>,
    ) -> Result<Building, CommandError> {
        let (id, building) = self.building_at(tile).ok_or(CommandError::NothingThere)?;
        if !building.kind.is_removable() {
            return Err(CommandError::NotRemovable);
        }
        Ok(self.remove_id(id, events))
    }

    fn remove_id(&mut self, id: BuildingId, events: &mut Vec<SimEvent>) -> Building {
        let building = self.arena.remove(id).expect("identifiant valide");
        for tile in building.tiles() {
            if self.grid.get(tile) == Some(id) {
                self.grid.set(tile, None);
            }
        }
        events.push(SimEvent::BuildingRemoved { id, building });
        building
    }

    /// Change l'orientation d'un convoyeur déjà posé (même tuile, même taille).
    pub fn set_orientation(
        &mut self,
        id: BuildingId,
        rotation: Rotation,
        rotation_variant: u8,
        events: &mut Vec<SimEvent>,
    ) {
        let building = &mut self.arena[id];
        debug_assert_eq!(building.kind, BuildingKind::Belt);
        building.rotation = rotation;
        building.rotation_variant = rotation_variant;
        let building = *building;
        events.push(SimEvent::BuildingChanged { id, building });
    }

    /// `getEjectorsAndAcceptorsAtTile` : slots des voisins qui visent cette tuile.
    pub fn slots_touching(&self, tile: TilePos) -> (Vec<EjectorSlot>, Vec<AcceptorSlot>) {
        let mut ejectors = Vec::new();
        let mut acceptors = Vec::new();
        // Même ordre de parcours que le JS (dx puis dy).
        for (dx, dy) in [(-1, 0), (0, -1), (0, 1), (1, 0)] {
            let Some((_, other)) = self.building_at(TilePos::new(tile.x + dx, tile.y + dy)) else {
                continue;
            };
            let slots = other.world_slots();
            ejectors.extend(
                slots
                    .ejectors
                    .into_iter()
                    .filter(|e| e.pos.step(e.to) == tile),
            );
            acceptors.extend(
                slots
                    .acceptors
                    .into_iter()
                    .filter(|a| a.pos.step(a.from) == tile),
            );
        }
        (ejectors, acceptors)
    }

    /// Orientation automatique d'un convoyeur
    /// (`MetaBeltBuilding.computeOptimalDirectionAndRotationVariantAtTile`).
    pub fn optimal_belt(&self, tile: TilePos, rotation: Rotation) -> (Rotation, u8) {
        let dir = |r: Rotation| Direction::Top.rotated(rotation.then(r));
        let (top, right, bottom, left) = (
            dir(Rotation::R0),
            dir(Rotation::R90),
            dir(Rotation::R180),
            dir(Rotation::R270),
        );
        let (ejectors, acceptors) = self.slots_touching(tile);

        let (mut from_bottom, mut from_right, mut from_left) = (false, false, false);
        for e in &ejectors {
            if e.to == top {
                from_bottom = true;
            } else if e.to == left {
                from_right = true;
            } else if e.to == right {
                from_left = true;
            }
        }
        let (mut to_top, mut to_left, mut to_right) = (false, false, false);
        for a in &acceptors {
            if a.from == bottom {
                to_top = true;
            } else if a.from == right {
                to_left = true;
            } else if a.from == left {
                to_right = true;
            }
        }

        if !from_bottom {
            if from_right && !from_left {
                return (rotation.then(Rotation::R270), belt_variant::RIGHT);
            }
            if from_left && !from_right {
                return (rotation.then(Rotation::R90), belt_variant::LEFT);
            }
        }
        if !to_top {
            if to_right && !to_left {
                return (rotation, belt_variant::RIGHT);
            }
            if to_left && !to_right {
                return (rotation, belt_variant::LEFT);
            }
        }
        (rotation, belt_variant::STRAIGHT)
    }

    /// Orientation automatique d'un tunnel : devient récepteur s'il trouve un émetteur
    /// du même niveau qui pointe vers lui
    /// (`MetaUndergroundBeltBuilding.computeOptimalDirectionAndRotationVariantAtTile`).
    pub fn optimal_tunnel(
        &self,
        tile: TilePos,
        rotation: Rotation,
        variant: Variant,
    ) -> (Rotation, u8) {
        let search = Direction::Top.rotated(rotation);
        let target_rotation = rotation.then(Rotation::R180);
        let tier2 = variant == Variant::Tier2;
        let mut pos = tile;
        for _ in 0..tunnel_range(variant) {
            pos = pos.step(search);
            let Some((_, other)) = self.building_at(pos) else {
                continue;
            };
            if other.kind != BuildingKind::UndergroundBelt
                || (other.variant == Variant::Tier2) != tier2
            {
                continue;
            }
            if other.rotation == target_rotation {
                if other.rotation_variant != tunnel_variant::SENDER {
                    break;
                }
                return (target_rotation, tunnel_variant::RECEIVER);
            } else if other.rotation == rotation {
                if other.rotation_variant == tunnel_variant::RECEIVER {
                    return (rotation, tunnel_variant::SENDER);
                }
                break;
            }
        }
        (rotation, tunnel_variant::SENDER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(b: &mut Buildings, p: Placement) -> Result<BuildingId, CommandError> {
        b.place(p, &mut Vec::new())
    }

    #[test]
    fn buildings_block_but_belts_are_replaced() {
        let mut b = Buildings::default();
        let belt = place(
            &mut b,
            Placement::new(BuildingKind::Belt, TilePos::new(1, 0), Rotation::R0),
        )
        .unwrap();
        let mut events = Vec::new();
        let cutter = b
            .place(
                Placement::new(BuildingKind::Cutter, TilePos::new(0, 0), Rotation::R0),
                &mut events,
            )
            .unwrap();
        assert!(b.get(belt).is_none(), "le convoyeur a été remplacé");
        assert!(matches!(events[0], SimEvent::BuildingRemoved { id, .. } if id == belt));
        assert_eq!(b.at(TilePos::new(1, 0)), Some(cutter));
        assert_eq!(
            place(
                &mut b,
                Placement::new(BuildingKind::Trash, TilePos::new(1, 0), Rotation::R0)
            ),
            Err(CommandError::Blocked {
                tile: TilePos::new(1, 0)
            })
        );
    }

    #[test]
    fn removal_frees_all_tiles_and_stale_ids_stay_invalid() {
        let mut b = Buildings::default();
        let p = Placement::new(BuildingKind::Storage, TilePos::new(-3, -3), Rotation::R90);
        let id = place(&mut b, p).unwrap();
        let tiles: Vec<_> = b.get(id).unwrap().tiles().collect();
        b.remove_at(tiles[3], &mut Vec::new()).unwrap();
        assert!(tiles.iter().all(|t| b.at(*t).is_none()));
        let new = place(&mut b, p).unwrap();
        assert_ne!(new, id);
        assert!(b.get(id).is_none());
    }

    #[test]
    fn hub_cannot_be_removed() {
        let mut b = Buildings::default();
        place(
            &mut b,
            Placement::new(BuildingKind::Hub, TilePos::new(-2, -2), Rotation::R0),
        )
        .unwrap();
        assert_eq!(
            b.remove_at(TilePos::new(0, 0), &mut Vec::new()),
            Err(CommandError::NotRemovable)
        );
    }

    #[test]
    fn belt_curves_towards_side_ejector() {
        let mut b = Buildings::default();
        // Un extracteur à gauche de (0, 0) qui éjecte vers la droite.
        place(
            &mut b,
            Placement::new(BuildingKind::Miner, TilePos::new(-1, 0), Rotation::R90),
        )
        .unwrap();
        // Un convoyeur posé vers le haut en (0, 0) doit tourner : entrée par la gauche.
        let (rot, variant) = b.optimal_belt(TilePos::new(0, 0), Rotation::R0);
        let belt = Building {
            kind: BuildingKind::Belt,
            variant: Variant::Default,
            rotation_variant: variant,
            origin: TilePos::new(0, 0),
            rotation: rot,
            original_rotation: Rotation::R0,
        };
        assert_eq!(belt.world_slots().acceptors[0].from, Direction::Left);
        assert_eq!(belt.world_slots().ejectors[0].to, Direction::Top);
    }

    #[test]
    fn belt_turns_into_side_acceptor() {
        let mut b = Buildings::default();
        // Un découpeur à droite de (0, 0) ; rien au-dessus.
        place(
            &mut b,
            Placement::new(BuildingKind::Cutter, TilePos::new(1, 0), Rotation::R90),
        )
        .unwrap();
        // Le découpeur tourné de 90° accepte depuis la gauche, donc depuis (0, 0).
        assert_eq!(
            b.optimal_belt(TilePos::new(0, 0), Rotation::R0),
            (Rotation::R0, belt_variant::RIGHT)
        );
    }

    #[test]
    fn tunnel_pairs_with_sender_in_range() {
        let mut b = Buildings::default();
        place(
            &mut b,
            Placement::new(
                BuildingKind::UndergroundBelt,
                TilePos::new(0, 0),
                Rotation::R90,
            ),
        )
        .unwrap();
        // Récepteur à 5 tuiles à droite, posé « vers la gauche » : il trouve l'émetteur.
        let (rot, mode) = b.optimal_tunnel(TilePos::new(5, 0), Rotation::R270, Variant::Default);
        assert_eq!((rot, mode), (Rotation::R90, tunnel_variant::RECEIVER));
        // Hors de portée pour le niveau 1, mais pas pour le niveau 2 (qui ne voit pas le niveau 1).
        let far = b.optimal_tunnel(TilePos::new(6, 0), Rotation::R270, Variant::Default);
        assert_eq!(far.1, tunnel_variant::SENDER);
    }
}
