//! Logistique et traitement des items : chemins de convoyeurs, éjecteurs, extracteurs,
//! tunnels, stockages, processeurs, filtres et hub (port des systèmes `belt`,
//! `itemEjector`, `miner`, `undergroundBelt`, `storage`, `itemProcessor`, `filter` et
//! `beltReader` du JS).
//!
//! Chaque type de composant vit dans sa propre table indexée par [`BuildingId`]
//! (plan, §3.3). Les systèmes tournent dans l'ordre du `GameSystemManager` du JS :
//! convoyeurs → tunnels → extracteurs → stockages → processeurs → filtres → éjecteurs.
//! Les lecteurs de convoyeur publient leurs valeurs après la couche wires
//! ([`Factory::update_readers`]).

use std::collections::VecDeque;

use shapez_core::{Direction, Item, ShapeRegistry, TilePos};
use slotmap::{SecondaryMap, SlotMap};

use crate::{
    belt_path::{Acceptor, BeltEntry, BeltPath, BeltPathId, PathTarget},
    building::{Building, BuildingKind, SlotFilter, Variant, belt_variant, tunnel_variant},
    hub_goals::HubGoals,
    map::{Map, MapResource},
    placement::{BuildingId, Buildings, SimEvent},
    units::{CURVED_BELT_LENGTH, ProcessorKind, SECOND, TICK, TILE, Upgrades},
    wires::{Wiring, is_truthy},
};

/// `MAX_QUEUED_CHARGES`.
const MAX_QUEUED_CHARGES: usize = 2;
/// `chainBufferSize` des extracteurs chaînables.
const CHAIN_BUFFER_SIZE: usize = 6;
/// `StorageComponent.maximumStorage`.
pub const STORAGE_CAPACITY: u32 = 5000;
/// `MAX_ITEMS_IN_QUEUE` du filtre.
const FILTER_QUEUE: usize = 2;
/// `readerAnalyzeIntervalSeconds`.
const READER_WINDOW: u64 = 10 * SECOND;

/// Où un slot d'éjection livre ses items (cache recalculé quand le voisinage change).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum EjectTarget {
    #[default]
    None,
    Belt(BuildingId),
    Slot {
        building: BuildingId,
        slot: u8,
        filter: SlotFilter,
    },
}

/// Slot d'éjection en coordonnées monde (`ItemEjectorSlot`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EjectSlot {
    pub pos: TilePos,
    pub to: Direction,
    pub item: Option<Item>,
    /// Progression de 0 à [`TILE`] (1.0 dans le JS).
    pub progress: u32,
    pub target: EjectTarget,
}

/// `ItemEjectorComponent`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Ejector {
    pub slots: Vec<EjectSlot>,
}

impl Ejector {
    fn can_eject(&self, slot: usize) -> bool {
        self.slots[slot].item.is_none()
    }

    fn first_free_slot(&self) -> Option<usize> {
        (0..self.slots.len()).find(|&i| self.can_eject(i))
    }

    fn try_eject(&mut self, slot: usize, item: Item) -> bool {
        if !self.can_eject(slot) {
            return false;
        }
        self.slots[slot].item = Some(item);
        self.slots[slot].progress = 0;
        true
    }
}

/// Item produit par une charge, avec sa contrainte de sortie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct OutItem {
    item: Item,
    required_slot: Option<usize>,
    preferred_slot: Option<usize>,
}

impl OutItem {
    fn any(item: Item) -> Self {
        Self {
            item,
            required_slot: None,
            preferred_slot: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Charge {
    items: Vec<OutItem>,
    remaining: i64,
}

/// `ItemProcessorComponent`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Processor {
    pub kind: ProcessorKind,
    inputs_per_charge: usize,
    /// Entrées par slot (ou dans l'ordre d'arrivée pour le hub et la poubelle).
    inputs: Vec<Option<Item>>,
    input_count: usize,
    charges: VecDeque<Charge>,
    bonus_time: i64,
    queued: Vec<OutItem>,
    next_output_slot: usize,
}

impl Processor {
    fn new(kind: ProcessorKind, inputs_per_charge: usize, slots: usize) -> Self {
        Self {
            kind,
            inputs_per_charge,
            inputs: vec![None; slots],
            input_count: 0,
            charges: VecDeque::new(),
            bonus_time: 0,
            queued: Vec::new(),
            next_output_slot: 0,
        }
    }

    fn takes_everything(&self) -> bool {
        matches!(self.kind, ProcessorKind::Hub | ProcessorKind::Trash)
    }

    /// `checkRequirements` : le peintre quadruple n'accepte une couleur que si le pin
    /// de ce quart reçoit un signal vrai.
    fn check_requirements(&self, id: BuildingId, slot: usize, wiring: &Wiring) -> bool {
        self.kind != ProcessorKind::PainterQuad
            || slot == 0
            || is_truthy(wiring.input_value(id, slot - 1))
    }

    /// `tryTakeItem`.
    fn try_take(&mut self, item: Item, slot: usize) -> bool {
        if self.takes_everything() {
            if self.input_count < self.inputs.len() {
                self.inputs[self.input_count] = Some(item);
            } else {
                self.inputs.push(Some(item));
            }
            self.input_count += 1;
            return true;
        }
        if self.inputs[slot].is_some() {
            return false;
        }
        self.inputs[slot] = Some(item);
        self.input_count += 1;
        true
    }

    /// `canProcess`.
    fn can_process(&self, id: BuildingId, wiring: &Wiring, registry: &ShapeRegistry) -> bool {
        if self.kind != ProcessorKind::PainterQuad {
            return self.input_count >= self.inputs_per_charge;
        }
        // Peintre quadruple : il faut la forme, au moins un quart activé, et la couleur
        // de chaque quart activé dont la forme a quelque chose.
        let Some(Item::Shape(shape)) = self.inputs[0] else {
            return false;
        };
        let enabled: [bool; 4] = std::array::from_fn(|i| is_truthy(wiring.input_value(id, i)));
        if !enabled.contains(&true) {
            return false;
        }
        let def = registry.get(shape);
        (0..4).all(|i| {
            !enabled[i]
                || self.inputs[1 + i].is_some()
                || def.layers().iter().all(|layer| layer[i].is_none())
        })
    }

    /// Nombre d'items en attente d'entrée (pour l'affichage et les tests).
    pub fn input_count(&self) -> usize {
        self.input_count
    }
}

/// Cache `cachedChainedMiner` : `Unknown` correspond au `null` du JS, `None` à `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum ChainCache {
    Unknown,
    None,
    Some(BuildingId),
}

/// `MinerComponent`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Miner {
    chainable: bool,
    last_mining_time: u64,
    chain_buffer: VecDeque<Item>,
    mined: Option<Item>,
    chain: ChainCache,
}

/// `UndergroundBeltComponent`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tunnel {
    sender: bool,
    tier2: bool,
    /// `(item, date d'arrivée)` côté récepteur, `(item, 0)` côté émetteur.
    pending: VecDeque<(Item, u64)>,
    /// `None` = à recalculer ; `Some(None)` = pas de récepteur.
    link: Option<Option<(BuildingId, u32)>>,
}

impl Tunnel {
    fn try_accept_external(&mut self, item: Item) -> bool {
        if !self.sender || !self.pending.is_empty() {
            return false;
        }
        self.pending.push_back((item, 0));
        true
    }

    fn try_accept_tunneled(&mut self, item: Item, distance: u32, arrival: u64) -> bool {
        if self.sender {
            return false;
        }
        // `pending.length >= (2 + distance) / itemSpacing`
        if self.pending.len() as u64 * 63 >= (2 + u64::from(distance)) * 100 {
            return false;
        }
        self.pending.push_back((item, arrival));
        true
    }
}

/// `StorageComponent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Storage {
    pub item: Option<Item>,
    pub count: u32,
}

impl Storage {
    pub fn is_full(&self) -> bool {
        self.count >= STORAGE_CAPACITY
    }

    /// `canAcceptItem` : un seul type d'item à la fois (les booléens sont refusés une
    /// fois le stockage entamé).
    fn can_accept(&self, item: Item) -> bool {
        if self.is_full() {
            return false;
        }
        match self.item {
            None => true,
            _ if self.count == 0 => true,
            Some(Item::Bool(_)) => false,
            Some(stored) => stored == item,
        }
    }
}

/// Item en transit dans un filtre (`progress` de 0 à [`TILE`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Pending {
    item: Item,
    progress: u32,
}

/// `FilterComponent` : items qui passent (slot 0, en haut) ou sont rejetés (slot 1,
/// à droite).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Filter {
    lists: [Vec<Pending>; 2],
}

/// `BeltReaderComponent`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Reader {
    pub last_item: Option<Item>,
    /// Dates de passage des items des dix dernières secondes.
    times: VecDeque<u64>,
}

impl Reader {
    /// Débit en items par seconde (`lastThroughput`, pour l'affichage).
    pub fn throughput(&self) -> f32 {
        if self.times.len() < 2 {
            return 0.0;
        }
        let span = self.times.back().unwrap() - self.times.front().unwrap();
        let average = span as f32 / (self.times.len() - 1) as f32 / SECOND as f32;
        (1.0 / average).min(2.0 * 23.9)
    }
}

/// Bâtiments qui reçoivent des items depuis un convoyeur ou un éjecteur.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Receivers {
    processors: SecondaryMap<BuildingId, Processor>,
    tunnels: SecondaryMap<BuildingId, Tunnel>,
    #[serde(default)]
    storages: SecondaryMap<BuildingId, Storage>,
    #[serde(default)]
    filters: SecondaryMap<BuildingId, Filter>,
}

impl Receivers {
    fn accepts_items(&self, id: BuildingId) -> bool {
        self.processors.contains_key(id)
            || self.tunnels.contains_key(id)
            || self.storages.contains_key(id)
            || self.filters.contains_key(id)
    }

    /// `tryPassOverItem` / `computePassOverFunctionWithoutBelts`.
    fn try_take(&mut self, id: BuildingId, slot: usize, item: Item, wiring: &Wiring) -> bool {
        if let Some(p) = self.processors.get_mut(id) {
            return p.check_requirements(id, slot, wiring) && p.try_take(item, slot);
        }
        if let Some(t) = self.tunnels.get_mut(id) {
            return t.try_accept_external(item);
        }
        if let Some(s) = self.storages.get_mut(id) {
            if !s.can_accept(item) {
                return false;
            }
            s.item = Some(item);
            s.count += 1;
            return true;
        }
        if let Some(f) = self.filters.get_mut(id) {
            // `FilterSystem.tryAcceptItem` : sans signal, le filtre refuse tout.
            let Some(value) = wiring.input_value(id, 0) else {
                return false;
            };
            let list = if value == Item::Bool(true) || value == item {
                &mut f.lists[0]
            } else {
                &mut f.lists[1]
            };
            if list.len() >= FILTER_QUEUE {
                return false;
            }
            list.push(Pending { item, progress: 0 });
            return true;
        }
        false
    }
}

fn filter_accepts(filter: SlotFilter, item: Item) -> bool {
    match filter {
        SlotFilter::Any => true,
        SlotFilter::Shape => matches!(item, Item::Shape(_)),
        SlotFilter::Color => matches!(item, Item::Color(_)),
    }
}

/// Ressources partagées par les systèmes pendant un tick.
#[derive(Debug)]
pub struct TickContext<'a> {
    pub buildings: &'a Buildings,
    pub map: &'a mut Map,
    pub registry: &'a mut ShapeRegistry,
    pub hub: &'a mut HubGoals,
    /// Couche wires : signaux lus par les filtres et le peintre quadruple, pins écrits
    /// par les stockages.
    pub wiring: &'a mut Wiring,
    pub upgrades: Upgrades,
    /// Date du tick (`root.time.now()`), en unités de [`crate::units::SECOND`].
    pub now: u64,
    pub events: &'a mut Vec<SimEvent>,
}

/// Item affiché à l'écran : centre en coordonnées tuiles (flottantes, rendu seulement).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleItem {
    pub x: f32,
    pub y: f32,
    pub item: Item,
}

/// État logistique de tous les bâtiments.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Factory {
    paths: SlotMap<BeltPathId, BeltPath>,
    /// Ordre de mise à jour des chemins (`BeltSystem.beltPaths`).
    path_order: Vec<BeltPathId>,
    belt_path: SecondaryMap<BuildingId, BeltPathId>,
    ejectors: SecondaryMap<BuildingId, Ejector>,
    receivers: Receivers,
    miners: SecondaryMap<BuildingId, Miner>,
    #[serde(default)]
    readers: SecondaryMap<BuildingId, Reader>,
    ejectors_dirty: bool,
    tunnels_dirty: bool,
    miners_dirty: bool,
}

fn belt_length(b: &Building) -> u32 {
    if b.rotation_variant == belt_variant::STRAIGHT {
        TILE
    } else {
        CURVED_BELT_LENGTH
    }
}

/// Direction de sortie d'un convoyeur, en coordonnées monde.
fn belt_output(b: &Building) -> Direction {
    let local = match b.rotation_variant {
        belt_variant::LEFT => Direction::Left,
        belt_variant::RIGHT => Direction::Right,
        _ => Direction::Top,
    };
    b.local_direction_to_world(local)
}

/// Côté par lequel un convoyeur accepte les items… vu de l'extérieur : un item
/// poussé dans cette direction entre dans le convoyeur.
fn belt_accepting(b: &Building) -> Direction {
    b.local_direction_to_world(Direction::Top)
}

fn processor_for(b: &Building) -> Option<Processor> {
    use ProcessorKind as P;
    let slots = b.world_slots().acceptors.len();
    let (kind, per_charge) = match (b.kind, b.variant) {
        (BuildingKind::Balancer, _) => (P::Balancer, 1),
        (BuildingKind::Cutter, Variant::Quad) => (P::CutterQuad, 1),
        (BuildingKind::Cutter, _) => (P::Cutter, 1),
        (BuildingKind::Rotater, Variant::Ccw) => (P::RotaterCcw, 1),
        (BuildingKind::Rotater, Variant::Rotate180) => (P::Rotater180, 1),
        (BuildingKind::Rotater, _) => (P::Rotater, 1),
        (BuildingKind::Stacker, _) => (P::Stacker, 2),
        (BuildingKind::Mixer, _) => (P::Mixer, 2),
        (BuildingKind::Painter, Variant::Double) => (P::PainterDouble, 3),
        (BuildingKind::Painter, Variant::Quad) => (P::PainterQuad, 5),
        (BuildingKind::Painter, _) => (P::Painter, 2),
        (BuildingKind::Trash, _) => (P::Trash, 1),
        (BuildingKind::Hub, _) => (P::Hub, 1),
        (BuildingKind::Reader, _) => (P::Reader, 1),
        _ => return None,
    };
    Some(Processor::new(kind, per_charge, slots))
}

impl Factory {
    // ------------------------------------------------------------------
    // Cycle de vie des bâtiments
    // ------------------------------------------------------------------

    /// Un bâtiment vient d'être posé (`entityAdded`).
    pub fn on_added(&mut self, id: BuildingId, buildings: &Buildings) {
        let b = buildings.get(id).copied().expect("bâtiment posé");
        if b.kind == BuildingKind::Belt {
            self.add_belt_to_paths(id, buildings);
        } else {
            let slots = b.world_slots();
            if !slots.ejectors.is_empty() {
                let ejector = Ejector {
                    slots: slots
                        .ejectors
                        .iter()
                        .map(|e| EjectSlot {
                            pos: e.pos,
                            to: e.to,
                            item: None,
                            progress: 0,
                            target: EjectTarget::None,
                        })
                        .collect(),
                };
                self.ejectors.insert(id, ejector);
            }
            if let Some(p) = processor_for(&b) {
                self.receivers.processors.insert(id, p);
            }
            match b.kind {
                BuildingKind::Miner => {
                    let chainable = b.variant == Variant::Chainable;
                    self.miners.insert(
                        id,
                        Miner {
                            chainable,
                            last_mining_time: 0,
                            chain_buffer: VecDeque::new(),
                            mined: None,
                            chain: ChainCache::Unknown,
                        },
                    );
                    self.miners_dirty |= chainable;
                }
                BuildingKind::UndergroundBelt => {
                    self.receivers.tunnels.insert(
                        id,
                        Tunnel {
                            sender: b.rotation_variant == tunnel_variant::SENDER,
                            tier2: b.variant == Variant::Tier2,
                            pending: VecDeque::new(),
                            link: None,
                        },
                    );
                    self.tunnels_dirty = true;
                }
                BuildingKind::Storage => {
                    self.receivers.storages.insert(id, Storage::default());
                }
                BuildingKind::Filter => {
                    self.receivers.filters.insert(id, Filter::default());
                }
                BuildingKind::Reader => {
                    self.readers.insert(id, Reader::default());
                }
                _ => {}
            }
        }
        self.ejectors_dirty = true;
    }

    /// Un bâtiment vient d'être retiré de la carte (`entityDestroyed`).
    pub fn on_removed(&mut self, id: BuildingId, building: &Building, buildings: &Buildings) {
        if building.kind == BuildingKind::Belt {
            self.delete_belt_from_path(id, buildings);
        }
        self.ejectors.remove(id);
        self.receivers.processors.remove(id);
        self.receivers.storages.remove(id);
        self.receivers.filters.remove(id);
        self.readers.remove(id);
        if let Some(m) = self.miners.remove(id) {
            self.miners_dirty |= m.chainable;
        }
        if self.receivers.tunnels.remove(id).is_some() {
            self.tunnels_dirty = true;
        }
        self.ejectors_dirty = true;
    }

    /// Le voisinage de ces convoyeurs a changé (`onSurroundingsChanged`).
    pub fn notify_paths_of(&mut self, belts: &[BuildingId], buildings: &Buildings) {
        let mut seen = Vec::new();
        for &belt in belts {
            if let Some(&p) = self.belt_path.get(belt)
                && !seen.contains(&p)
            {
                seen.push(p);
            }
        }
        for p in seen {
            self.path_changed(p, buildings);
        }
    }

    /// Première moitié d'une réorientation de convoyeur : on le retire de son chemin
    /// (avec son ancienne forme) et on renvoie ce chemin.
    pub fn detach_belt(&mut self, id: BuildingId, buildings: &Buildings) -> Option<BeltPathId> {
        let original = self.belt_path.get(id).copied();
        self.delete_belt_from_path(id, buildings);
        original
    }

    /// Seconde moitié : le convoyeur a sa nouvelle forme, on le remet dans les chemins.
    pub fn reattach_belt(
        &mut self,
        id: BuildingId,
        original: Option<BeltPathId>,
        buildings: &Buildings,
    ) {
        if let Some(p) = original
            && self.paths.contains_key(p)
        {
            self.path_changed(p, buildings);
        }
        self.add_belt_to_paths(id, buildings);
        self.ejectors_dirty = true;
    }

    // ------------------------------------------------------------------
    // Chemins de convoyeurs (`BeltSystem`)
    // ------------------------------------------------------------------

    fn path_changed(&mut self, p: BeltPathId, buildings: &Buildings) {
        let target = self.compute_path_target(p, buildings);
        self.paths[p].on_path_changed(target);
    }

    /// `computeAcceptingEntityAndSlot`.
    fn compute_path_target(&self, p: BeltPathId, buildings: &Buildings) -> PathTarget {
        let last = self.paths[p].belts().last().expect("chemin non vide").id;
        let Some(b) = buildings.get(last) else {
            return PathTarget::None;
        };
        let dir = belt_output(b);
        let tile = b.origin.step(dir);
        let Some((tid, target)) = buildings.building_at(tile) else {
            return PathTarget::None;
        };
        if target.kind == BuildingKind::Belt {
            return if belt_accepting(target) == dir {
                PathTarget::Belt(tid)
            } else {
                PathTarget::None
            };
        }
        let Some((slot, acc)) = target
            .world_slots()
            .acceptors
            .into_iter()
            .enumerate()
            .find(|(_, a)| a.pos == tile && a.from == dir.inverted())
        else {
            return PathTarget::None;
        };
        if !self.receivers.accepts_items(tid) {
            return PathTarget::None;
        }
        PathTarget::Slot {
            building: tid,
            slot: slot as u8,
            filter: acc.filter,
        }
    }

    /// `findSupplyingEntity` : convoyeur derrière qui pointe vers celui-ci.
    fn find_supplying(&self, b: &Building, buildings: &Buildings) -> Option<BuildingId> {
        let supply_dir = b.local_direction_to_world(Direction::Bottom);
        let (sid, s) = buildings.building_at(b.origin.step(supply_dir))?;
        (s.kind == BuildingKind::Belt
            && self.belt_path.contains_key(sid)
            && belt_output(s) == supply_dir.inverted())
        .then_some(sid)
    }

    /// `findFollowUpEntity` : convoyeur devant qui accepte depuis celui-ci.
    fn find_follow_up(&self, b: &Building, buildings: &Buildings) -> Option<BuildingId> {
        let dir = belt_output(b);
        let (fid, f) = buildings.building_at(b.origin.step(dir))?;
        (f.kind == BuildingKind::Belt
            && self.belt_path.contains_key(fid)
            && belt_accepting(f) == dir)
            .then_some(fid)
    }

    /// `addEntityToPaths`.
    fn add_belt_to_paths(&mut self, id: BuildingId, buildings: &Buildings) {
        let b = *buildings.get(id).expect("convoyeur posé");
        let entry = BeltEntry {
            id,
            length: belt_length(&b),
        };
        let from = self.find_supplying(&b, buildings);
        let to = self.find_follow_up(&b, buildings);
        match (from, to) {
            (Some(from), to) => {
                let fp = self.belt_path[from];
                self.paths[fp].extend_on_end(entry);
                self.belt_path.insert(id, fp);
                self.path_changed(fp, buildings);
                if let Some(to) = to {
                    let tp = self.belt_path[to];
                    if tp != fp {
                        let other = self.paths.remove(tp).expect("chemin suivant");
                        self.remove_from_order(tp);
                        for belt in other.belts() {
                            self.belt_path.insert(belt.id, fp);
                        }
                        self.paths[fp].extend_by_path(other);
                        self.path_changed(fp, buildings);
                    }
                }
            }
            (None, Some(to)) => {
                let tp = self.belt_path[to];
                self.paths[tp].extend_on_beginning(entry);
                self.belt_path.insert(id, tp);
                self.path_changed(tp, buildings);
            }
            (None, None) => {
                let p = self.paths.insert(BeltPath::new(vec![entry]));
                self.path_order.push(p);
                self.belt_path.insert(id, p);
                self.path_changed(p, buildings);
            }
        }
    }

    /// `fastArrayDeleteValue(beltPaths, path)`.
    fn remove_from_order(&mut self, p: BeltPathId) {
        if let Some(i) = self.path_order.iter().position(|&x| x == p) {
            self.path_order.swap_remove(i);
        }
    }

    /// `deleteEntityFromPath`.
    fn delete_belt_from_path(&mut self, id: BuildingId, buildings: &Buildings) {
        let Some(p) = self.belt_path.remove(id) else {
            return;
        };
        let path = &mut self.paths[p];
        if path.belts().len() == 1 {
            self.paths.remove(p);
            self.remove_from_order(p);
            return;
        }
        if path.is_start(id) {
            path.delete_on_start();
            self.path_changed(p, buildings);
        } else if path.is_end(id) {
            path.delete_on_end();
            self.path_changed(p, buildings);
        } else {
            let index = path
                .belts()
                .iter()
                .position(|b| b.id == id)
                .expect("convoyeur du chemin");
            let second = path.split_at(index);
            let belts: Vec<_> = second.belts().iter().map(|b| b.id).collect();
            let np = self.paths.insert(second);
            self.path_order.push(np);
            for belt in belts {
                self.belt_path.insert(belt, np);
            }
            self.path_changed(p, buildings);
            self.path_changed(np, buildings);
        }
    }

    // ------------------------------------------------------------------
    // Caches de voisinage
    // ------------------------------------------------------------------

    /// `ItemEjectorSystem.recomputeSingleEntityCache` pour tous les éjecteurs.
    fn recompute_ejector_targets(&mut self, buildings: &Buildings) {
        for (_, ejector) in self.ejectors.iter_mut() {
            for slot in &mut ejector.slots {
                slot.target = EjectTarget::None;
                let tile = slot.pos.step(slot.to);
                let Some((tid, target)) = buildings.building_at(tile) else {
                    continue;
                };
                if target.kind == BuildingKind::Belt {
                    if belt_accepting(target) == slot.to {
                        slot.target = EjectTarget::Belt(tid);
                    }
                    continue;
                }
                if let Some((i, acc)) = target
                    .world_slots()
                    .acceptors
                    .into_iter()
                    .enumerate()
                    .find(|(_, a)| a.pos == tile && a.from == slot.to.inverted())
                {
                    slot.target = EjectTarget::Slot {
                        building: tid,
                        slot: i as u8,
                        filter: acc.filter,
                    };
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Tick
    // ------------------------------------------------------------------

    /// Exécute un tick de logistique, dans l'ordre des systèmes du JS.
    pub fn tick(&mut self, ctx: &mut TickContext<'_>) {
        if std::mem::take(&mut self.ejectors_dirty) {
            self.recompute_ejector_targets(ctx.buildings);
        }
        self.update_belts(ctx);
        self.update_tunnels(ctx);
        self.update_miners(ctx);
        self.update_storages(ctx);
        self.update_processors(ctx);
        self.update_filters(ctx);
        self.update_ejectors(ctx);
    }

    /// `StorageSystem.update` : un item par tick vers le premier slot libre, puis les
    /// pins (item stocké, plein).
    fn update_storages(&mut self, ctx: &mut TickContext<'_>) {
        for (id, storage) in self.receivers.storages.iter_mut() {
            if let Some(item) = storage.item
                && storage.count > 0
                && let Some(ejector) = self.ejectors.get_mut(id)
                && let Some(slot) = ejector.first_free_slot()
                && ejector.try_eject(slot, item)
            {
                storage.count -= 1;
                if storage.count == 0 {
                    storage.item = None;
                }
            }
            if let Some(building) = ctx.buildings.get(id) {
                ctx.wiring.set_pin(id, building, 0, storage.item);
                ctx.wiring
                    .set_pin(id, building, 1, Some(Item::Bool(storage.is_full())));
            }
        }
    }

    /// `FilterSystem.update`, y compris son parcours qui retire toujours le premier
    /// item de la liste et saute le suivant après une éjection.
    fn update_filters(&mut self, ctx: &mut TickContext<'_>) {
        let step = ctx.upgrades.belt_speed_per_tick();
        let required = TILE - step;
        for (id, filter) in self.receivers.filters.iter_mut() {
            let Some(ejector) = self.ejectors.get_mut(id) else {
                continue;
            };
            for (slot, list) in filter.lists.iter_mut().enumerate() {
                let mut j = 0;
                while j < list.len() {
                    let pending = &mut list[j];
                    pending.progress = required.min(pending.progress + step);
                    if pending.progress >= required && ejector.try_eject(slot, pending.item) {
                        list.remove(0);
                    }
                    j += 1;
                }
            }
        }
    }

    /// `BeltReaderSystem.update`, après les portes logiques : pin 0 vrai si un item est
    /// passé dans la dernière seconde, pin 1 = dernier item.
    pub fn update_readers(&mut self, buildings: &Buildings, wiring: &mut Wiring, now: u64) {
        for (id, reader) in self.readers.iter_mut() {
            while reader
                .times
                .front()
                .is_some_and(|&t| t + READER_WINDOW < now)
            {
                reader.times.pop_front();
            }
            let Some(building) = buildings.get(id) else {
                continue;
            };
            let last = reader.times.back().copied().unwrap_or(0);
            wiring.set_pin(id, building, 0, Some(Item::Bool(last + SECOND > now)));
            wiring.set_pin(id, building, 1, reader.last_item);
        }
    }

    fn update_belts(&mut self, ctx: &mut TickContext<'_>) {
        let speed = ctx.upgrades.belt_speed_per_tick();
        for i in 0..self.path_order.len() {
            let p = self.path_order[i];
            match self.paths[p].target {
                PathTarget::None => self.paths[p].update(speed, Acceptor::None),
                PathTarget::Belt(next) => match self.belt_path.get(next).copied() {
                    Some(np) if np == p => self.paths[p].update(speed, Acceptor::SelfLoop),
                    Some(np) => {
                        let [path, next_path] = self
                            .paths
                            .get_disjoint_mut([p, np])
                            .expect("chemins distincts");
                        let mut accept = |item| next_path.try_accept_item(item, speed);
                        path.update(speed, Acceptor::Other(&mut accept));
                    }
                    None => self.paths[p].update(speed, Acceptor::None),
                },
                PathTarget::Slot {
                    building,
                    slot,
                    filter,
                } => {
                    let receivers = &mut self.receivers;
                    let wiring = &*ctx.wiring;
                    let mut accept = |item| {
                        filter_accepts(filter, item)
                            && receivers.try_take(building, usize::from(slot), item, wiring)
                    };
                    self.paths[p].update(speed, Acceptor::Other(&mut accept));
                }
            }
        }
    }

    fn update_tunnels(&mut self, ctx: &mut TickContext<'_>) {
        let reset = std::mem::take(&mut self.tunnels_dirty);
        let ids: Vec<BuildingId> = self.receivers.tunnels.keys().collect();
        if reset {
            for &id in &ids {
                self.receivers.tunnels[id].link = None;
            }
        }
        for id in ids {
            if self.receivers.tunnels[id].sender {
                let link = match self.receivers.tunnels[id].link {
                    Some(link) => link,
                    None => {
                        let link = self.find_receiver(id, ctx.buildings);
                        self.receivers.tunnels[id].link = Some(link);
                        link
                    }
                };
                let Some((receiver, distance)) = link else {
                    continue;
                };
                let Some(&(item, _)) = self.receivers.tunnels[id].pending.front() else {
                    continue;
                };
                let arrival = ctx.now + ctx.upgrades.tunnel_travel_duration(distance);
                if let Some(r) = self.receivers.tunnels.get_mut(receiver)
                    && r.try_accept_tunneled(item, distance, arrival)
                {
                    self.receivers.tunnels[id].pending.pop_front();
                }
            } else {
                let Some(&(item, arrival)) = self.receivers.tunnels[id].pending.front() else {
                    continue;
                };
                if ctx.now <= arrival {
                    continue;
                }
                if let Some(ejector) = self.ejectors.get_mut(id)
                    && let Some(slot) = ejector.first_free_slot()
                    && ejector.try_eject(slot, item)
                {
                    self.receivers.tunnels[id].pending.pop_front();
                }
            }
        }
    }

    /// `findRecieverForSender`.
    fn find_receiver(&self, id: BuildingId, buildings: &Buildings) -> Option<(BuildingId, u32)> {
        let b = buildings.get(id)?;
        let tier2 = self.receivers.tunnels[id].tier2;
        let dir = b.local_direction_to_world(Direction::Top);
        let mut tile = b.origin;
        for offset in 0..crate::building::tunnel_range(b.variant) {
            tile = tile.step(dir);
            let Some((oid, other)) = buildings.building_at(tile) else {
                continue;
            };
            let Some(t) = self.receivers.tunnels.get(oid) else {
                continue;
            };
            if t.tier2 != tier2 || other.rotation != b.rotation {
                continue;
            }
            if t.sender {
                break;
            }
            return Some((oid, offset as u32));
        }
        None
    }

    fn update_miners(&mut self, ctx: &mut TickContext<'_>) {
        let reset = std::mem::take(&mut self.miners_dirty);
        let duration = ctx.upgrades.miner_duration();
        let ids: Vec<BuildingId> = self.miners.keys().collect();
        for id in ids {
            if reset {
                self.miners[id].chain = ChainCache::Unknown;
            }
            if self.miners[id].mined.is_none() {
                let Some(origin) = ctx.buildings.get(id).map(|b| b.origin) else {
                    continue;
                };
                let Some(resource) = ctx.map.resource_at(origin) else {
                    continue;
                };
                self.miners[id].mined = Some(match resource {
                    MapResource::Shape(def) => Item::Shape(ctx.registry.intern(def)),
                    MapResource::Color(c) => Item::Color(c),
                });
            }
            if let Some(&item) = self.miners[id].chain_buffer.front()
                && self.try_miner_eject(id, item, ctx)
            {
                self.miners[id].chain_buffer.pop_front();
                continue;
            }
            let since = ctx.now - self.miners[id].last_mining_time;
            if since > duration {
                let buffer = (since - duration).min(TICK);
                let item = self.miners[id].mined.expect("ressource connue");
                if self.try_miner_eject(id, item, ctx) {
                    // Signal `itemProduced`.
                    if let Item::Shape(shape) = item {
                        ctx.hub.analytics.on_produced(shape);
                    }
                    self.miners[id].last_mining_time = ctx.now - buffer;
                }
            }
        }
    }

    /// `tryPerformMinerEject`, y compris sa lecture du cache *avant* recalcul.
    fn try_miner_eject(&mut self, id: BuildingId, item: Item, ctx: &mut TickContext<'_>) -> bool {
        if self.miners[id].chainable {
            let target = self.miners[id].chain;
            if target == ChainCache::Unknown {
                self.miners[id].chain = self.find_chained_miner(id, ctx);
            }
            if let ChainCache::Some(t) = target {
                return match self.miners.get_mut(t) {
                    Some(m) if m.chain_buffer.len() <= CHAIN_BUFFER_SIZE => {
                        m.chain_buffer.push_back(item);
                        true
                    }
                    _ => false,
                };
            }
        }
        self.ejectors
            .get_mut(id)
            .is_some_and(|e| e.try_eject(0, item))
    }

    /// `findChainedMiner`.
    fn find_chained_miner(&self, id: BuildingId, ctx: &mut TickContext<'_>) -> ChainCache {
        let Some(b) = ctx.buildings.get(id) else {
            return ChainCache::None;
        };
        if ctx.map.resource_at(b.origin).is_none() {
            return ChainCache::None;
        }
        let Some(slot) = self.ejectors.get(id).and_then(|e| e.slots.first()) else {
            return ChainCache::None;
        };
        let tile = slot.pos.step(slot.to);
        if let Some((tid, _)) = ctx.buildings.building_at(tile)
            && self.miners.get(tid).is_some_and(|m| m.chainable)
            && ctx.map.resource_at(tile).is_some()
        {
            return ChainCache::Some(tid);
        }
        ChainCache::None
    }

    fn update_processors(&mut self, ctx: &mut TickContext<'_>) {
        let ids: Vec<BuildingId> = self.receivers.processors.keys().collect();
        for id in ids {
            let p = &mut self.receivers.processors[id];
            if let Some(charge) = p.charges.front_mut() {
                if charge.remaining > 0 {
                    charge.remaining -= TICK as i64;
                    if charge.remaining < 0 {
                        p.bonus_time += -charge.remaining;
                    }
                }
                if charge.remaining <= 0 && p.queued.is_empty() {
                    let charge = p.charges.pop_front().expect("charge en cours");
                    p.queued.extend(charge.items);
                }
            }
            if p.charges.len() < MAX_QUEUED_CHARGES && p.can_process(id, ctx.wiring, ctx.registry) {
                // `process_READER` : l'item passe et le lecteur note son passage.
                if p.kind == ProcessorKind::Reader
                    && let Some(reader) = self.readers.get_mut(id)
                {
                    reader.times.push_back(ctx.now);
                    reader.last_item = p.inputs[0];
                }
                let slots = self.ejectors.get(id).map_or(0, |e| e.slots.len());
                start_new_charge(p, slots, ctx);
            }
            let Some(ejector) = self.ejectors.get_mut(id) else {
                p.queued.clear();
                continue;
            };
            p.queued.retain(|out| {
                let slot = if let Some(required) = out.required_slot {
                    ejector.can_eject(required).then_some(required)
                } else if let Some(preferred) = out.preferred_slot
                    && ejector.can_eject(preferred)
                {
                    Some(preferred)
                } else {
                    ejector.first_free_slot()
                };
                match slot {
                    Some(slot) => !ejector.try_eject(slot, out.item),
                    None => true,
                }
            });
        }
    }

    fn update_ejectors(&mut self, ctx: &mut TickContext<'_>) {
        let growth = ctx.upgrades.ejector_progress_per_tick();
        let speed = ctx.upgrades.belt_speed_per_tick();
        for (_, ejector) in self.ejectors.iter_mut() {
            for slot in &mut ejector.slots {
                let Some(item) = slot.item else {
                    continue;
                };
                slot.progress = (slot.progress + growth).min(TILE);
                if slot.progress < TILE {
                    continue;
                }
                match slot.target {
                    EjectTarget::None => {}
                    EjectTarget::Belt(belt) => {
                        if let Some(&p) = self.belt_path.get(belt)
                            && self.paths[p].try_accept_item(item, speed)
                        {
                            slot.item = None;
                        }
                    }
                    EjectTarget::Slot {
                        building,
                        slot: index,
                        filter,
                    } => {
                        if filter_accepts(filter, item)
                            && self.receivers.try_take(
                                building,
                                usize::from(index),
                                item,
                                ctx.wiring,
                            )
                        {
                            slot.item = None;
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Lecture (rendu, tests)
    // ------------------------------------------------------------------

    /// Dépose un item au début du chemin du convoyeur (outil de test et de bac à sable).
    pub fn inject(&mut self, belt: BuildingId, item: Item, upgrades: &Upgrades) -> bool {
        let Some(&p) = self.belt_path.get(belt) else {
            return false;
        };
        self.paths[p].try_accept_item(item, upgrades.belt_speed_per_tick())
    }

    pub fn paths(&self) -> impl Iterator<Item = (BeltPathId, &BeltPath)> {
        self.path_order.iter().map(|&p| (p, &self.paths[p]))
    }

    pub fn path_of(&self, belt: BuildingId) -> Option<&BeltPath> {
        self.belt_path.get(belt).map(|&p| &self.paths[p])
    }

    pub fn ejector(&self, id: BuildingId) -> Option<&Ejector> {
        self.ejectors.get(id)
    }

    pub fn processor(&self, id: BuildingId) -> Option<&Processor> {
        self.receivers.processors.get(id)
    }

    pub fn storage(&self, id: BuildingId) -> Option<&Storage> {
        self.receivers.storages.get(id)
    }

    pub fn reader(&self, id: BuildingId) -> Option<&Reader> {
        self.readers.get(id)
    }

    /// Nombre total d'items présents dans l'usine (convoyeurs, slots, tampons).
    pub fn item_count(&self) -> usize {
        let belts: usize = self.paths.values().map(BeltPath::len_items).sum();
        let ejectors: usize = self
            .ejectors
            .values()
            .flat_map(|e| &e.slots)
            .filter(|s| s.item.is_some())
            .count();
        let processors: usize = self
            .receivers
            .processors
            .values()
            .map(|p| {
                p.input_count
                    + p.queued.len()
                    + p.charges.iter().map(|c| c.items.len()).sum::<usize>()
            })
            .sum();
        let tunnels: usize = self
            .receivers
            .tunnels
            .values()
            .map(|t| t.pending.len())
            .sum();
        let miners: usize = self.miners.values().map(|m| m.chain_buffer.len()).sum();
        let storages: usize = self
            .receivers
            .storages
            .values()
            .map(|s| s.count as usize)
            .sum();
        let filters: usize = self
            .receivers
            .filters
            .values()
            .map(|f| f.lists[0].len() + f.lists[1].len())
            .sum();
        belts + ejectors + processors + tunnels + miners + storages + filters
    }

    /// Items visibles sur les convoyeurs et en sortie de bâtiment
    /// (`BeltPath.draw` et `ItemEjectorSystem.drawChunk`).
    pub fn visible_items(&self, buildings: &Buildings) -> Vec<VisibleItem> {
        let mut out = Vec::new();
        for path in self.paths.values() {
            for (progress, item) in path.items_with_progress() {
                if let Some((x, y)) = position_on_path(path, progress, buildings) {
                    out.push(VisibleItem { x, y, item });
                }
            }
        }
        for ejector in self.ejectors.values() {
            for slot in &ejector.slots {
                let Some(item) = slot.item else {
                    continue;
                };
                let mut progress = slot.progress as f32 / TILE as f32;
                if let EjectTarget::Belt(belt) = slot.target
                    && let Some(path) = self.path_of(belt)
                {
                    let spacing = path.spacing_to_first() as f32 / TILE as f32;
                    let max =
                        (0.5 + spacing - crate::units::ITEM_SPACING as f32 / TILE as f32) * 2.0;
                    progress = progress.min(max);
                }
                if progress < 0.05 {
                    continue;
                }
                let (dx, dy) = slot.to.offset();
                out.push(VisibleItem {
                    x: slot.pos.x as f32 + 0.5 + dx as f32 * 0.5 * progress,
                    y: slot.pos.y as f32 + 0.5 + dy as f32 * 0.5 * progress,
                    item,
                });
            }
        }
        out
    }
}

/// `computePositionFromProgress` + `transformBeltToLocalSpace`.
fn position_on_path(path: &BeltPath, progress: u32, buildings: &Buildings) -> Option<(f32, f32)> {
    let belts = path.belts();
    let mut current = 0;
    for (i, entry) in belts.iter().enumerate() {
        if current + entry.length >= progress || i + 1 == belts.len() {
            let b = buildings.get(entry.id)?;
            let local = ((progress - current.min(progress)) as f32 / TILE as f32).min(1.0);
            let curve = CURVED_BELT_LENGTH as f32 / TILE as f32;
            let (lx, ly) = match b.rotation_variant {
                belt_variant::RIGHT => {
                    let a = local / curve * std::f32::consts::FRAC_PI_2;
                    (0.5 - 0.5 * a.cos(), 0.5 - 0.5 * a.sin())
                }
                belt_variant::LEFT => {
                    let a = local / curve * std::f32::consts::FRAC_PI_2;
                    (-0.5 + 0.5 * a.cos(), 0.5 - 0.5 * a.sin())
                }
                _ => (0.0, 0.5 - local),
            };
            let (rx, ry) = rotate(lx, ly, b.rotation);
            return Some((b.origin.x as f32 + 0.5 + rx, b.origin.y as f32 + 0.5 + ry));
        }
        current += entry.length;
    }
    None
}

fn rotate(x: f32, y: f32, r: shapez_core::Rotation) -> (f32, f32) {
    match r {
        shapez_core::Rotation::R0 => (x, y),
        shapez_core::Rotation::R90 => (-y, x),
        shapez_core::Rotation::R180 => (-x, -y),
        shapez_core::Rotation::R270 => (y, -x),
    }
}

/// `startNewCharge` et les `process_*` du JS.
fn start_new_charge(p: &mut Processor, ejector_slots: usize, ctx: &mut TickContext<'_>) {
    use ProcessorKind as P;
    let shape = |i: usize, p: &Processor| match p.inputs.get(i).copied().flatten() {
        Some(Item::Shape(id)) => Some(id),
        _ => None,
    };
    let color = |i: usize, p: &Processor| match p.inputs.get(i).copied().flatten() {
        Some(Item::Color(c)) => Some(c),
        _ => None,
    };
    let mut out = Vec::new();
    match p.kind {
        P::Balancer => {
            for i in 0..2 {
                if let Some(item) = p.inputs.get(i).copied().flatten() {
                    let preferred = p.next_output_slot % ejector_slots.max(1);
                    p.next_output_slot += 1;
                    out.push(OutItem {
                        item,
                        required_slot: None,
                        preferred_slot: Some(preferred),
                    });
                }
            }
        }
        P::Cutter | P::CutterQuad => {
            if let Some(id) = shape(0, p) {
                let def = *ctx.registry.get(id);
                let parts: Vec<_> = if p.kind == P::Cutter {
                    def.cut_half().to_vec()
                } else {
                    def.cut_quad().to_vec()
                };
                for (i, part) in parts.into_iter().enumerate() {
                    if part.is_empty() {
                        continue;
                    }
                    out.push(OutItem {
                        item: Item::Shape(ctx.registry.intern(part)),
                        required_slot: Some(i),
                        preferred_slot: None,
                    });
                }
            }
        }
        P::Rotater | P::RotaterCcw | P::Rotater180 => {
            if let Some(id) = shape(0, p) {
                let def = ctx.registry.get(id);
                let rotated = match p.kind {
                    P::Rotater => def.rotate_cw(),
                    P::RotaterCcw => def.rotate_ccw(),
                    _ => def.rotate_180(),
                };
                out.push(OutItem::any(Item::Shape(ctx.registry.intern(rotated))));
            }
        }
        P::Stacker => {
            if let (Some(lower), Some(upper)) = (shape(0, p), shape(1, p)) {
                let stacked = ctx.registry.get(lower).stack(ctx.registry.get(upper));
                out.push(OutItem::any(Item::Shape(ctx.registry.intern(stacked))));
            }
        }
        P::Mixer => {
            if let (Some(a), Some(b)) = (color(0, p), color(1, p)) {
                out.push(OutItem::any(Item::Color(a.mix(b))));
            }
        }
        P::Painter => {
            if let (Some(s), Some(c)) = (shape(0, p), color(1, p)) {
                let painted = ctx.registry.get(s).paint(c);
                out.push(OutItem::any(Item::Shape(ctx.registry.intern(painted))));
            }
        }
        P::PainterDouble => {
            if let (Some(s1), Some(s2), Some(c)) = (shape(0, p), shape(1, p), color(2, p)) {
                for s in [s1, s2] {
                    let painted = ctx.registry.get(s).paint(c);
                    out.push(OutItem::any(Item::Shape(ctx.registry.intern(painted))));
                }
            }
        }
        P::PainterQuad => {
            if let Some(s) = shape(0, p) {
                let colors = [1, 2, 3, 4].map(|i| color(i, p));
                let painted = ctx.registry.get(s).paint_quadrants(colors);
                out.push(OutItem::any(Item::Shape(ctx.registry.intern(painted))));
            }
        }
        P::Reader => {
            if let Some(item) = p.inputs[0] {
                out.push(OutItem::any(item));
            }
        }
        P::Trash => {}
        P::Hub => {
            for i in 0..p.input_count {
                if let Some(id) = shape(i, p)
                    && let Some(done) = ctx.hub.deliver(id, ctx.registry)
                {
                    ctx.events.push(SimEvent::LevelCompleted(done));
                }
            }
        }
    }

    // Signal `itemProduced` ; l'équilibreur et le lecteur passent `doNotTrack`.
    if !matches!(p.kind, P::Balancer | P::Reader) {
        for o in &out {
            if let Item::Shape(shape) = o.item {
                ctx.hub.analytics.on_produced(shape);
            }
        }
    }

    let original = ctx.upgrades.processor_duration(p.kind) as i64;
    let bonus = original.min(p.bonus_time);
    p.bonus_time -= bonus;
    p.charges.push_back(Charge {
        items: out,
        remaining: original - bonus,
    });
    p.inputs.iter_mut().for_each(|i| *i = None);
    p.input_count = 0;
}
