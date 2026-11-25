//! Bâtiments posés sur la carte : arène, grilles d'occupation (une par couche),
//! commandes de placement et de suppression, orientation automatique des convoyeurs,
//! tunnels et câbles.
//!
//! Toute mutation passe par une [`SimCommand`] (plan, §3.4) ; les bâtiments vivent
//! dans une arène à index générationnels (plan, §3.3).

use std::collections::HashMap;

use shapez_core::{Direction, Item, Rotation, TilePos};
use slotmap::{SlotMap, new_key_type};

use crate::{
    building::{
        AcceptorSlot, Building, BuildingKind, EjectorSlot, Layer, Variant, belt_variant,
        tunnel_range, tunnel_variant, wire_type,
    },
    map::{CHUNK_SIZE, ChunkPos},
};

new_key_type! {
    /// Identifiant générationnel d'un bâtiment : un index périmé ne pointe jamais
    /// vers un autre bâtiment.
    pub struct BuildingId;
}

type ChunkGrid = [[Option<BuildingId>; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];

/// Grille d'occupation d'une couche, par chunk.
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
    /// Réglage du bâtiment (voir [`Building::config`]).
    pub config: Option<Item>,
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
            config: None,
        }
    }

    pub fn with_variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn with_config(mut self, config: Option<Item>) -> Self {
        self.config = config;
        self
    }

    /// Placement qui reproduit exactement un bâtiment existant.
    pub fn of_building(b: &Building) -> Self {
        Self {
            kind: b.kind,
            variant: b.variant,
            origin: b.origin,
            rotation: b.rotation,
            rotation_variant: b.rotation_variant,
            original_rotation: b.original_rotation,
            config: b.config,
        }
    }

    /// Bâtiment tel qu'il serait posé (aperçu).
    pub fn building(&self) -> Building {
        Building {
            kind: self.kind,
            variant: self.variant,
            rotation_variant: self.rotation_variant,
            origin: self.origin,
            rotation: self.rotation,
            original_rotation: self.original_rotation,
            config: self.config,
        }
    }
}

/// Mutation du monde demandée par le joueur (ou par un test, un replay…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimCommand {
    Place(Placement),
    /// Supprime le bâtiment qui occupe la tuile sur cette couche.
    Remove {
        tile: TilePos,
        layer: Layer,
    },
    /// Change le réglage d'un émetteur constant ou d'un levier.
    Configure {
        tile: TilePos,
        layer: Layer,
        config: Option<Item>,
    },
    /// Achète le prochain palier d'une famille d'améliorations.
    UnlockUpgrade(crate::upgrades::UpgradeKind),
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
    /// Convoyeur ou câble réorienté automatiquement après la pose ou la suppression
    /// d'un voisin, ou réglage modifié.
    BuildingChanged {
        id: BuildingId,
        building: Building,
    },
    /// Objectif du hub atteint.
    LevelCompleted(crate::hub_goals::LevelCompleted),
    /// Palier d'amélioration acheté ; `level` est le nombre de paliers possédés.
    UpgradePurchased {
        kind: crate::upgrades::UpgradeKind,
        level: usize,
    },
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
    /// Pas assez de formes en stock pour ce palier (ou palier maximal atteint).
    CannotAfford,
    /// Ce bâtiment n'a pas de réglage.
    NotConfigurable,
}

/// Bâtiments du monde et leur occupation des grilles.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(from = "BuildingsData")]
pub struct Buildings {
    arena: SlotMap<BuildingId, Building>,
    /// Couche normale ; déduite de l'arène, reconstruite au chargement.
    #[serde(skip)]
    grid: OccupancyGrid,
    /// Couche wires.
    #[serde(skip)]
    wires_grid: OccupancyGrid,
}

/// Forme sérialisée de [`Buildings`] : l'arène seule.
#[derive(serde::Deserialize)]
struct BuildingsData {
    arena: SlotMap<BuildingId, Building>,
}

impl From<BuildingsData> for Buildings {
    fn from(data: BuildingsData) -> Self {
        let mut buildings = Self {
            arena: data.arena,
            ..Self::default()
        };
        let all: Vec<_> = buildings.arena.iter().map(|(id, b)| (id, *b)).collect();
        for (id, building) in all {
            for tile in building.tiles() {
                buildings
                    .grid_mut(building.kind.layer())
                    .set(tile, Some(id));
            }
        }
        buildings
    }
}

impl Buildings {
    pub fn get(&self, id: BuildingId) -> Option<&Building> {
        self.arena.get(id)
    }

    fn grid(&self, layer: Layer) -> &OccupancyGrid {
        match layer {
            Layer::Regular => &self.grid,
            Layer::Wires => &self.wires_grid,
        }
    }

    fn grid_mut(&mut self, layer: Layer) -> &mut OccupancyGrid {
        match layer {
            Layer::Regular => &mut self.grid,
            Layer::Wires => &mut self.wires_grid,
        }
    }

    /// Bâtiment de la couche normale sur cette tuile.
    pub fn at(&self, tile: TilePos) -> Option<BuildingId> {
        self.grid.get(tile)
    }

    pub fn building_at(&self, tile: TilePos) -> Option<(BuildingId, &Building)> {
        self.building_at_layer(tile, Layer::Regular)
    }

    pub fn at_layer(&self, tile: TilePos, layer: Layer) -> Option<BuildingId> {
        self.grid(layer).get(tile)
    }

    pub fn building_at_layer(
        &self,
        tile: TilePos,
        layer: Layer,
    ) -> Option<(BuildingId, &Building)> {
        let id = self.grid(layer).get(tile)?;
        Some((id, &self.arena[id]))
    }

    /// Bâtiments des deux couches sur cette tuile, couche normale d'abord
    /// (`getLayersContentsMultipleXY`).
    pub fn contents_at(&self, tile: TilePos) -> impl Iterator<Item = BuildingId> + '_ {
        [self.grid.get(tile), self.wires_grid.get(tile)]
            .into_iter()
            .flatten()
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
        let layer = placement.kind.layer();
        for tile in placement.building().tiles() {
            if let Some((_, other)) = self.building_at_layer(tile, layer)
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
        let layer = building.kind.layer();
        for tile in building.tiles() {
            if let Some(old) = self.grid(layer).get(tile) {
                self.remove_id(old, events);
            }
        }
        let id = self.arena.insert(building);
        for tile in building.tiles() {
            self.grid_mut(layer).set(tile, Some(id));
        }
        events.push(SimEvent::BuildingPlaced { id, building });
        Ok(id)
    }

    /// `tryDeleteBuilding`.
    pub fn remove_at(
        &mut self,
        tile: TilePos,
        layer: Layer,
        events: &mut Vec<SimEvent>,
    ) -> Result<Building, CommandError> {
        let (id, building) = self
            .building_at_layer(tile, layer)
            .ok_or(CommandError::NothingThere)?;
        if !building.kind.is_removable() {
            return Err(CommandError::NotRemovable);
        }
        Ok(self.remove_id(id, events))
    }

    fn remove_id(&mut self, id: BuildingId, events: &mut Vec<SimEvent>) -> Building {
        let building = self.arena.remove(id).expect("identifiant valide");
        let grid = self.grid_mut(building.kind.layer());
        for tile in building.tiles() {
            if grid.get(tile) == Some(id) {
                grid.set(tile, None);
            }
        }
        events.push(SimEvent::BuildingRemoved { id, building });
        building
    }

    /// Change l'orientation d'un convoyeur ou d'un câble déjà posé (même tuile, même
    /// taille).
    pub fn set_orientation(
        &mut self,
        id: BuildingId,
        rotation: Rotation,
        rotation_variant: u8,
        events: &mut Vec<SimEvent>,
    ) {
        let building = &mut self.arena[id];
        debug_assert!(matches!(
            building.kind,
            BuildingKind::Belt | BuildingKind::Wire
        ));
        building.rotation = rotation;
        building.rotation_variant = rotation_variant;
        let building = *building;
        events.push(SimEvent::BuildingChanged { id, building });
    }

    /// Change le réglage d'un émetteur constant ou d'un levier.
    pub fn set_config(
        &mut self,
        tile: TilePos,
        layer: Layer,
        config: Option<Item>,
        events: &mut Vec<SimEvent>,
    ) -> Result<(), CommandError> {
        let id = self
            .at_layer(tile, layer)
            .ok_or(CommandError::NothingThere)?;
        let building = &mut self.arena[id];
        if !building.kind.is_configurable() {
            return Err(CommandError::NotConfigurable);
        }
        building.config = config;
        let building = *building;
        events.push(SimEvent::BuildingChanged { id, building });
        Ok(())
    }

    /// `logic.computeWireEdgeStatus` : un câble de cette couleur posé en `tile`
    /// se relierait-il à la tuile voisine du côté `edge` ?
    fn wire_edge(&self, tile: TilePos, variant: Variant, edge: Direction) -> bool {
        let target = tile.step(edge);
        for id in self.contents_at(target) {
            let other = &self.arena[id];
            if other
                .world_pins()
                .iter()
                .any(|p| p.pos == target && p.direction == edge.inverted())
            {
                return true;
            }
        }
        match self.building_at_layer(target, Layer::Wires) {
            Some((_, other)) => match other.kind {
                BuildingKind::WireTunnel => true,
                BuildingKind::Wire => other.variant == variant,
                _ => false,
            },
            None => false,
        }
    }

    /// Orientation automatique d'un câble : forme et rotation selon ses voisins
    /// (`MetaWireBuilding.computeOptimalDirectionAndRotationVariantAtTile`).
    pub fn optimal_wire(&self, tile: TilePos, variant: Variant) -> (Rotation, u8) {
        use Direction::{Bottom, Left, Right, Top};
        let edge = |d| self.wire_edge(tile, variant, d);
        let (top, right, bottom, left) = (edge(Top), edge(Right), edge(Bottom), edge(Left));
        use Rotation::{R0, R90, R180, R270};
        use wire_type::{CROSS, FORWARD, SPLIT, TURN};
        match (top, right, bottom, left) {
            (false, false, false, true) => (R90, FORWARD),
            (false, false, true, true) => (R90, TURN),
            (false, true, false, _) => (R90, FORWARD),
            (false, true, true, false) => (R0, TURN),
            (false, true, true, true) => (R0, SPLIT),
            (true, false, false, true) => (R180, TURN),
            (true, false, true, true) => (R90, SPLIT),
            (true, true, false, false) => (R270, TURN),
            (true, true, false, true) => (R180, SPLIT),
            (true, true, true, false) => (R270, SPLIT),
            (true, true, true, true) => (R0, CROSS),
            _ => (R0, FORWARD),
        }
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
        b.remove_at(tiles[3], Layer::Regular, &mut Vec::new())
            .unwrap();
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
            b.remove_at(TilePos::new(0, 0), Layer::Regular, &mut Vec::new()),
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
            config: None,
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
