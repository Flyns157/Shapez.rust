//! Couche wires : réseaux de câbles, émetteurs constants, leviers et portes logiques
//! (port de `systems/wire.js`, `systems/logic_gate.js`, `systems/lever.js` et
//! `systems/constant_signal.js`).
//!
//! Chaque tick, dans l'ordre du JS : les émetteurs constants et les leviers écrivent
//! leur pin, chaque réseau agrège les valeurs de ses émetteurs, puis chaque porte lit
//! ses réseaux d'entrée et écrit sa sortie, que son réseau verra au tick suivant.

use shapez_core::{Direction, Item, TilePos};
use slotmap::SecondaryMap;

use crate::{
    building::{Building, BuildingKind, PinKind, Variant},
    placement::{BuildingId, Buildings},
};

/// Indice d'un réseau dans [`Wiring::networks`], valable jusqu'au prochain recalcul.
pub type NetworkId = u32;

/// Un réseau : câbles reliés entre eux et pins qui y sont branchés (`WireNetwork`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Network {
    /// Pins émetteurs `(bâtiment, indice du pin)`.
    pub providers: Vec<(BuildingId, u8)>,
    /// Pins récepteurs.
    pub receivers: Vec<(BuildingId, u8)>,
    pub wires: Vec<BuildingId>,
    pub tunnels: Vec<BuildingId>,
    /// Valeur courante (rien en cas de conflit).
    pub value: Option<Item>,
    /// Plusieurs émetteurs envoient des valeurs différentes.
    pub conflict: bool,
}

impl Network {
    /// `hasValue`.
    pub fn has_value(&self) -> bool {
        self.value.is_some() && !self.conflict
    }
}

/// État d'un câble, pour le rendu (`getSpriteSetAndOpacityForWire`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireState {
    /// Relié à aucun émetteur.
    Unconnected,
    /// Valeur du réseau (`None` : aucun émetteur n'envoie rien).
    Value(Option<Item>),
    Conflict,
}

/// Un câble est « allumé » s'il porte une valeur vraie (`isTruthyItem`).
pub fn is_truthy(item: Option<Item>) -> bool {
    match item {
        None => false,
        Some(Item::Bool(b)) => b,
        Some(_) => true,
    }
}

/// Valeurs des pins et réseaux de la couche wires.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Wiring {
    /// Valeur de chaque pin (`WiredPinsComponent.slots[].value`), sauvegardée comme
    /// dans le JS : la sortie d'une porte compte au tick suivant.
    pins: SecondaryMap<BuildingId, Vec<Option<Item>>>,
    /// Réseaux, recalculés après chaque changement de la couche.
    #[serde(skip)]
    networks: Vec<Network>,
    #[serde(skip)]
    wire_network: SecondaryMap<BuildingId, NetworkId>,
    #[serde(skip)]
    pin_networks: SecondaryMap<BuildingId, Vec<Option<NetworkId>>>,
    #[serde(skip)]
    tunnel_networks: SecondaryMap<BuildingId, Vec<NetworkId>>,
    #[serde(skip, default = "yes")]
    dirty: bool,
}

fn yes() -> bool {
    true
}

impl Default for Wiring {
    fn default() -> Self {
        Self {
            pins: SecondaryMap::new(),
            networks: Vec::new(),
            wire_network: SecondaryMap::new(),
            pin_networks: SecondaryMap::new(),
            tunnel_networks: SecondaryMap::new(),
            dirty: true,
        }
    }
}

/// `isEntityRelevantForWires`.
fn is_relevant(b: &Building) -> bool {
    matches!(b.kind, BuildingKind::Wire | BuildingKind::WireTunnel) || !b.world_pins().is_empty()
}

/// Élément à visiter pendant la recherche d'un réseau.
#[derive(Debug, Clone, Copy)]
enum Visit {
    Wire(BuildingId),
    Pin(BuildingId, u8),
}

impl Wiring {
    /// `entityAdded` : réserve les pins et invalide les réseaux.
    pub fn on_added(&mut self, id: BuildingId, building: &Building) {
        if !is_relevant(building) {
            return;
        }
        let n = building.world_pins().len();
        if n > 0 {
            self.pins.insert(id, vec![None; n]);
        }
        self.dirty = true;
    }

    /// `entityDestroyed`.
    pub fn on_removed(&mut self, id: BuildingId, building: &Building) {
        if !is_relevant(building) {
            return;
        }
        self.pins.remove(id);
        self.dirty = true;
    }

    /// Réseaux actifs (ceux qui ont au moins un émetteur).
    pub fn networks(&self) -> &[Network] {
        &self.networks
    }

    /// Valeur d'un pin.
    pub fn pin_value(&self, id: BuildingId, pin: usize) -> Option<Item> {
        self.pins
            .get(id)
            .and_then(|p| p.get(pin).copied().flatten())
    }

    /// Réseau relié à un pin.
    pub fn pin_network(&self, id: BuildingId, pin: usize) -> Option<&Network> {
        let n = self.pin_networks.get(id)?.get(pin).copied().flatten()?;
        self.networks.get(n as usize)
    }

    /// État d'un câble.
    pub fn wire_state(&self, id: BuildingId) -> WireState {
        match self.wire_network.get(id) {
            None => WireState::Unconnected,
            Some(&n) => {
                let network = &self.networks[n as usize];
                if network.conflict {
                    WireState::Conflict
                } else {
                    WireState::Value(network.value)
                }
            }
        }
    }

    /// Réseaux traversant un tunnel de câbles.
    pub fn tunnel_networks(&self, id: BuildingId) -> impl Iterator<Item = &Network> {
        self.tunnel_networks
            .get(id)
            .into_iter()
            .flatten()
            .map(|&n| &self.networks[n as usize])
    }

    /// Un tick de la couche wires.
    pub fn tick(&mut self, buildings: &Buildings) {
        if self.dirty {
            self.recompute(buildings);
        }

        // `ConstantSignalSystem` puis `LeverSystem`.
        for kind in [BuildingKind::ConstantSignal, BuildingKind::Lever] {
            for (id, building) in buildings.iter().filter(|(_, b)| b.kind == kind) {
                let value = match kind {
                    BuildingKind::Lever => {
                        Some(Item::Bool(building.config == Some(Item::Bool(true))))
                    }
                    _ => building.config,
                };
                self.pins_mut(id, building)[0] = value;
            }
        }

        // `WireSystem.update` : agrégation des émetteurs.
        for network in &mut self.networks {
            network.conflict = false;
            let mut value = None;
            for &(id, pin) in &network.providers {
                let slot = self.pins.get(id).and_then(|p| p[pin as usize]);
                if value.is_none() {
                    value = slot;
                    continue;
                }
                match slot {
                    None => continue,
                    Some(v) if Some(v) == value => continue,
                    Some(_) => {
                        network.conflict = true;
                        break;
                    }
                }
            }
            network.value = if network.conflict { None } else { value };
        }

        // `LogicGateSystem.update`.
        for (id, building) in buildings.iter() {
            if !matches!(
                building.kind,
                BuildingKind::LogicGate | BuildingKind::Transistor
            ) {
                continue;
            }
            let pins = building.world_pins();
            let mut inputs = Vec::with_capacity(pins.len());
            let mut conflict = false;
            for (k, pin) in pins.iter().enumerate() {
                if pin.kind != PinKind::Acceptor {
                    continue;
                }
                match self.pin_networks.get(id).and_then(|n| n[k]) {
                    Some(n) => {
                        let network = &self.networks[n as usize];
                        if network.conflict {
                            conflict = true;
                            break;
                        }
                        inputs.push(network.value);
                    }
                    None => inputs.push(None),
                }
            }
            let out = self.pins_mut(id, building);
            if conflict {
                for (slot, pin) in out.iter_mut().zip(&pins) {
                    if pin.kind == PinKind::Ejector {
                        *slot = None;
                    }
                }
                continue;
            }
            out[0] = compute_gate(building.kind, building.variant, &inputs);
        }
    }

    /// Pins d'un bâtiment, créés au besoin (monde chargé d'une sauvegarde sans eux).
    fn pins_mut(&mut self, id: BuildingId, building: &Building) -> &mut Vec<Option<Item>> {
        if !self.pins.contains_key(id) {
            self.pins
                .insert(id, vec![None; building.world_pins().len()]);
        }
        &mut self.pins[id]
    }

    /// `recomputeWiresNetwork`.
    fn recompute(&mut self, buildings: &Buildings) {
        self.dirty = false;
        self.networks.clear();
        self.wire_network.clear();
        self.tunnel_networks.clear();
        self.pin_networks.clear();
        for (id, building) in buildings.iter() {
            let n = building.world_pins().len();
            if n > 0 {
                self.pin_networks.insert(id, vec![None; n]);
            }
        }
        for (id, building) in buildings.iter() {
            for (k, pin) in building.world_pins().iter().enumerate() {
                if pin.kind == PinKind::Ejector && self.pin_networks[id][k].is_none() {
                    self.find_network_for_ejector(buildings, id, k as u8);
                }
            }
        }
    }

    /// `findNetworkForEjector` : parcours en profondeur depuis un pin émetteur.
    fn find_network_for_ejector(&mut self, buildings: &Buildings, id: BuildingId, pin: u8) {
        let index = self.networks.len() as NetworkId;
        let mut network = Network::default();
        let mut stack = vec![Visit::Pin(id, pin)];
        // Couleur des câbles du réseau, fixée par le premier câble visité.
        let mut mask: Option<Variant> = None;

        while let Some(visit) = stack.pop() {
            let mut search: Option<(TilePos, Vec<Direction>)> = None;
            match visit {
                Visit::Wire(wire) => {
                    let building = buildings.get(wire).expect("câble existant");
                    if !self.wire_network.contains_key(wire)
                        && mask.is_none_or(|m| m == building.variant)
                    {
                        self.wire_network.insert(wire, index);
                        network.wires.push(wire);
                        search = Some((building.origin, Direction::ALL.to_vec()));
                        mask = Some(building.variant);
                    }
                }
                Visit::Pin(owner, k) => {
                    let building = buildings.get(owner).expect("bâtiment existant");
                    if self.pin_networks[owner][k as usize].is_none() {
                        let slot = building.world_pins()[k as usize];
                        match slot.kind {
                            PinKind::Ejector => network.providers.push((owner, k)),
                            PinKind::Acceptor => network.receivers.push((owner, k)),
                        }
                        self.pin_networks[owner][k as usize] = Some(index);
                        search = Some((slot.pos, vec![slot.direction]));
                    }
                }
            }
            if let Some((tile, directions)) = search {
                let targets =
                    self.find_targets(buildings, tile, &directions, index, &mut network, mask);
                stack.extend(targets);
            }
        }

        let useful = !network.providers.is_empty()
            && (!network.wires.is_empty()
                || !network.receivers.is_empty()
                || !network.tunnels.is_empty());
        if useful {
            self.networks.push(network);
        } else {
            for wire in &network.wires {
                self.wire_network.remove(*wire);
            }
            for tunnel in &network.tunnels {
                if let Some(list) = self.tunnel_networks.get_mut(*tunnel) {
                    list.retain(|&n| n != index);
                }
            }
            for &(owner, k) in network.providers.iter().chain(&network.receivers) {
                self.pin_networks[owner][k as usize] = None;
            }
        }
    }

    /// `findSurroundingWireTargets` : voisins pas encore reliés, à travers les tunnels.
    fn find_targets(
        &mut self,
        buildings: &Buildings,
        tile: TilePos,
        directions: &[Direction],
        index: NetworkId,
        network: &mut Network,
        mask: Option<Variant>,
    ) -> Vec<Visit> {
        let mut result = Vec::new();
        for &direction in directions {
            let start = tile.step(direction);
            let mut visited_tunnels: Vec<BuildingId> = Vec::new();
            let mut contents: Vec<(BuildingId, TilePos)> =
                buildings.contents_at(start).map(|id| (id, start)).collect();
            let mut k = 0;
            while k < contents.len() {
                let (id, at) = contents[k];
                k += 1;
                let building = buildings.get(id).expect("bâtiment existant");

                if building.kind == BuildingKind::Wire
                    && !self.wire_network.contains_key(id)
                    && mask.is_none_or(|m| m == building.variant)
                {
                    result.push(Visit::Wire(id));
                }

                let pins = building.world_pins();
                if !pins.is_empty() {
                    for (j, pin) in pins.iter().enumerate() {
                        if pin.pos == at
                            && pin.direction == direction.inverted()
                            && self.pin_networks[id][j].is_none()
                        {
                            result.push(Visit::Pin(id, j as u8));
                        }
                    }
                    continue;
                }

                if building.kind == BuildingKind::WireTunnel {
                    if visited_tunnels.contains(&id) {
                        continue;
                    }
                    let forwarded = building.origin.step(direction);
                    contents.extend(buildings.contents_at(forwarded).map(|c| (c, forwarded)));
                    if !self.tunnel_networks.contains_key(id) {
                        self.tunnel_networks.insert(id, Vec::new());
                    }
                    let list = &mut self.tunnel_networks[id];
                    if !list.contains(&index) {
                        list.push(index);
                    }
                    if !network.tunnels.contains(&id) {
                        network.tunnels.push(id);
                    }
                    visited_tunnels.push(id);
                }
            }
        }
        result
    }
}

/// Calcul d'une porte (`compute_AND`, `compute_IF`…), entrées dans l'ordre des pins.
fn compute_gate(kind: BuildingKind, variant: Variant, inputs: &[Option<Item>]) -> Option<Item> {
    let t = |i: usize| is_truthy(inputs.get(i).copied().flatten());
    let bool = |b: bool| Some(Item::Bool(b));
    match (kind, variant) {
        (BuildingKind::Transistor, _) => {
            if t(0) {
                inputs.get(1).copied().flatten()
            } else {
                None
            }
        }
        (_, Variant::Not) => bool(!t(0)),
        (_, Variant::Or) => bool(t(0) || t(1)),
        (_, Variant::Xor) => bool(t(0) != t(1)),
        _ => bool(t(0) && t(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_follow_truth_tables() {
        let b = |v| Some(Item::Bool(v));
        let and = |x, y| compute_gate(BuildingKind::LogicGate, Variant::Default, &[b(x), b(y)]);
        assert_eq!(and(true, true), b(true));
        assert_eq!(and(true, false), b(false));
        assert_eq!(
            compute_gate(BuildingKind::LogicGate, Variant::Not, &[None]),
            b(true)
        );
        let red = Some(Item::Color(shapez_core::Color::Red));
        assert_eq!(
            compute_gate(BuildingKind::LogicGate, Variant::Or, &[red, b(false)]),
            b(true)
        );
        assert_eq!(
            compute_gate(BuildingKind::Transistor, Variant::Default, &[b(true), red]),
            red
        );
        assert_eq!(
            compute_gate(BuildingKind::Transistor, Variant::Default, &[b(false), red]),
            None
        );
    }
}
