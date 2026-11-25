//! Chemin de convoyeurs (port de `src/js/game/belt_path.js`).
//!
//! Une suite de convoyeurs qui se suivent forme un seul chemin. Les items y sont
//! rangés du début vers la fin ; chacun connaît la distance qui le sépare du suivant
//! (ou de la fin du chemin pour le dernier). Avancer un chemin coûte donc O(items en
//! mouvement), quel que soit le nombre de convoyeurs.
//!
//! Les distances sont en unités fixes ([`crate::units::TILE`] par tuile).

use std::collections::VecDeque;

use shapez_core::Item;

use crate::{placement::BuildingId, units::ITEM_SPACING};

slotmap::new_key_type! {
    /// Identifiant générationnel d'un chemin de convoyeurs.
    pub struct BeltPathId;
}

/// Un convoyeur du chemin et sa longueur effective (`getEffectiveLengthTiles`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeltEntry {
    pub id: BuildingId,
    pub length: u32,
}

/// Où le chemin livre ses items (`computeAcceptingEntityAndSlot`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathTarget {
    /// Rien ne prend les items : ils s'accumulent au bout.
    #[default]
    None,
    /// Le premier convoyeur d'un autre chemin (ou du même, pour une boucle).
    Belt(BuildingId),
    /// Un slot d'entrée de bâtiment.
    Slot {
        building: BuildingId,
        slot: u8,
        filter: crate::building::SlotFilter,
    },
}

/// Accepteur passé à [`BeltPath::update`].
pub enum Acceptor<'a> {
    None,
    /// Le chemin se reboucle sur lui-même.
    SelfLoop,
    Other(&'a mut dyn FnMut(Item) -> bool),
}

impl std::fmt::Debug for Acceptor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::SelfLoop => "SelfLoop",
            Self::Other(_) => "Other(..)",
        })
    }
}

/// Chemin de convoyeurs et ses items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeltPath {
    belts: Vec<BeltEntry>,
    /// `(distance au suivant, item)`, du début vers la fin.
    items: VecDeque<(u32, Item)>,
    /// Distance du début du chemin au premier item (ou longueur totale si vide).
    spacing_to_first: u32,
    total_length: u32,
    /// Cible calculée par la simulation à chaque changement de voisinage.
    pub target: PathTarget,
    num_compressed: usize,
}

impl BeltPath {
    /// Nouveau chemin vide.
    pub fn new(belts: Vec<BeltEntry>) -> Self {
        assert!(!belts.is_empty(), "chemin vide");
        let total_length = belts.iter().map(|b| b.length).sum();
        Self {
            belts,
            items: VecDeque::new(),
            spacing_to_first: total_length,
            total_length,
            target: PathTarget::None,
            num_compressed: 0,
        }
    }

    pub fn belts(&self) -> &[BeltEntry] {
        &self.belts
    }

    pub fn total_length(&self) -> u32 {
        self.total_length
    }

    pub fn spacing_to_first(&self) -> u32 {
        self.spacing_to_first
    }

    pub fn len_items(&self) -> usize {
        self.items.len()
    }

    pub fn is_start(&self, id: BuildingId) -> bool {
        self.belts.first().is_some_and(|b| b.id == id)
    }

    pub fn is_end(&self, id: BuildingId) -> bool {
        self.belts.last().is_some_and(|b| b.id == id)
    }

    /// Distance de chaque item au suivant (ou à la fin pour le dernier).
    pub fn item_distances(&self) -> impl Iterator<Item = u32> + '_ {
        self.items.iter().map(|&(next, _)| next)
    }

    /// Items avec leur distance au début du chemin.
    pub fn items_with_progress(&self) -> impl Iterator<Item = (u32, Item)> + '_ {
        let mut pos = self.spacing_to_first;
        self.items.iter().map(move |&(next, item)| {
            let here = pos;
            pos += next;
            (here, item)
        })
    }

    /// `onPathChanged` : à appeler quand la cible a été recalculée.
    pub fn on_path_changed(&mut self, target: PathTarget) {
        self.target = target;
        self.num_compressed = 0;
    }

    pub fn can_accept_item(&self) -> bool {
        self.spacing_to_first >= ITEM_SPACING
    }

    /// `tryAcceptItem` : l'item entre au début du chemin et avance déjà d'un tick.
    pub fn try_accept_item(&mut self, item: Item, speed: u32) -> bool {
        if self.spacing_to_first < ITEM_SPACING {
            return false;
        }
        let max_progress = self.spacing_to_first - ITEM_SPACING;
        let initial = max_progress.min(speed);
        self.items
            .push_front((self.spacing_to_first - initial, item));
        self.spacing_to_first = initial;
        true
    }

    /// `update` : avance les items de `speed` unités, en poussant le premier bloqué
    /// contre les suivants, et livre l'item de tête à l'accepteur.
    pub fn update(&mut self, speed: u32, mut acceptor: Acceptor<'_>) {
        if self.items.is_empty() {
            return;
        }
        let speed_i = i64::from(speed);
        let spacing = i64::from(ITEM_SPACING);
        let mut first = true;
        let mut remaining = speed_i;
        let mut last = self.items.len() as isize - 1;

        while last >= 0 {
            let idx = last as usize;
            let min_spacing = if idx == self.items.len() - 1 {
                0
            } else {
                spacing
            };
            let mut clamped = i64::from(self.items[idx].0) - min_spacing;
            if remaining < clamped {
                clamped = remaining;
            }
            if clamped < 0 {
                clamped = 0;
            }
            remaining -= clamped;
            self.items[idx].0 -= clamped as u32;
            self.spacing_to_first += clamped as u32;

            if first && self.items[idx].0 == 0 {
                let item = self.items[idx].1;
                if self.deliver(item, speed, &mut acceptor) {
                    self.items.pop_back();
                    if self.num_compressed > 0
                        && last >= 1
                        && let Some(behind) = self.items.get_mut(idx - 1)
                    {
                        let fixup = remaining.min(i64::from(behind.0)).max(0);
                        behind.0 -= fixup as u32;
                        remaining -= fixup;
                        self.spacing_to_first += fixup as u32;
                    }
                    self.num_compressed = self.num_compressed.saturating_sub(1);
                }
            }

            if first {
                last -= self.num_compressed as isize;
            }
            first = false;
            if remaining <= 0 {
                break;
            }
            last -= 1;
        }

        let processed_tail = self.items.len() as isize - 2 - last;
        self.num_compressed = self.num_compressed.max(processed_tail.max(0) as usize);

        if let Some(&(0, item)) = self.items.back()
            && self.deliver(item, speed, &mut acceptor)
        {
            self.items.pop_back();
            self.num_compressed = self.num_compressed.saturating_sub(1);
        }
    }

    fn deliver(&mut self, item: Item, speed: u32, acceptor: &mut Acceptor<'_>) -> bool {
        match acceptor {
            Acceptor::None => false,
            Acceptor::SelfLoop => self.try_accept_item(item, speed),
            Acceptor::Other(f) => f(item),
        }
    }

    /// `extendOnEnd`.
    pub fn extend_on_end(&mut self, belt: BeltEntry) {
        self.belts.push(belt);
        self.total_length += belt.length;
        match self.items.back_mut() {
            None => self.spacing_to_first = self.total_length,
            Some(last) => last.0 += belt.length,
        }
    }

    /// `extendOnBeginning` : le nouveau convoyeur est vide.
    pub fn extend_on_beginning(&mut self, belt: BeltEntry) {
        self.total_length += belt.length;
        self.spacing_to_first += belt.length;
        self.belts.insert(0, belt);
    }

    /// `extendByPath` : ajoute `other` à la suite de ce chemin.
    pub fn extend_by_path(&mut self, other: BeltPath) {
        let old_length = self.total_length;
        for belt in &other.belts {
            self.belts.push(*belt);
            self.total_length += belt.length;
        }
        match self.items.back_mut() {
            Some(last) => last.0 += other.spacing_to_first,
            None => self.spacing_to_first = old_length + other.spacing_to_first,
        }
        self.items.extend(other.items);
    }

    /// `deleteEntityOnEnd` : les items du dernier convoyeur sont perdus.
    pub fn delete_on_end(&mut self) {
        let belt = self.belts.pop().expect("chemin non vide");
        self.total_length -= belt.length;
        if self.items.is_empty() {
            self.spacing_to_first = self.total_length;
            return;
        }
        let mut offset = self.spacing_to_first;
        let mut last_offset = offset;
        let mut kept = 0;
        for &(next, _) in &self.items {
            if offset >= self.total_length {
                break;
            }
            kept += 1;
            last_offset = offset;
            offset += next;
        }
        self.items.truncate(kept);
        match self.items.back_mut() {
            Some(last) => last.0 = self.total_length - last_offset,
            None => self.spacing_to_first = self.total_length,
        }
    }

    /// `deleteEntityOnStart` : les items du premier convoyeur sont perdus.
    pub fn delete_on_start(&mut self) {
        let belt = self.belts.remove(0);
        self.total_length -= belt.length;
        if self.items.is_empty() {
            self.spacing_to_first = self.total_length;
            return;
        }
        if self.spacing_to_first >= belt.length {
            self.spacing_to_first -= belt.length;
            return;
        }
        let mut offset = self.spacing_to_first;
        while let Some(&(next, _)) = self.items.front() {
            if offset > belt.length {
                break;
            }
            self.items.pop_front();
            offset += next;
        }
        self.spacing_to_first = if self.items.is_empty() {
            self.total_length
        } else {
            offset - belt.length
        };
    }

    /// `deleteEntityOnPathSplitIntoTwo` : retire le convoyeur d'indice `index`
    /// (ni le premier ni le dernier) et renvoie la seconde moitié.
    pub fn split_at(&mut self, index: usize) -> BeltPath {
        assert!(
            index > 0 && index + 1 < self.belts.len(),
            "coupure invalide"
        );
        let first_length: u32 = self.belts[..index].iter().map(|b| b.length).sum();
        let removed = self.belts[index].length;
        let second_start = first_length + removed;

        let second_belts = self.belts.split_off(index + 1);
        self.belts.pop();
        let mut second = BeltPath::new(second_belts);

        let mut pos = self.spacing_to_first;
        let mut kept = VecDeque::with_capacity(self.items.len());
        for (next, item) in self.items.drain(..) {
            if pos >= first_length {
                if pos >= second_start {
                    second.items.push_back((next, item));
                    if second.items.len() == 1 {
                        second.spacing_to_first = pos - second_start;
                    }
                }
            } else {
                let clamped = (pos + next).min(first_length) - pos;
                kept.push_back((clamped, item));
            }
            pos += next;
        }
        self.items = kept;
        self.total_length = first_length;
        if self.items.is_empty() {
            self.spacing_to_first = self.total_length;
        }
        second
    }

    /// Vérifie les invariants (tests et assertions de debug).
    pub fn check_integrity(&self) -> Result<(), String> {
        let sum: u32 = self.belts.iter().map(|b| b.length).sum();
        if sum != self.total_length {
            return Err(format!("longueur {} ≠ {}", self.total_length, sum));
        }
        let mut pos = self.spacing_to_first;
        for (i, &(next, _)) in self.items.iter().enumerate() {
            if i + 1 < self.items.len() && next < ITEM_SPACING {
                return Err(format!("items {i} et {} trop proches ({next})", i + 1));
            }
            pos += next;
        }
        if pos != self.total_length && !self.items.is_empty() {
            return Err(format!("fin à {pos} ≠ longueur {}", self.total_length));
        }
        if self.items.is_empty() && self.spacing_to_first != self.total_length {
            return Err("chemin vide mal initialisé".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{CURVED_BELT_LENGTH, TILE};
    use shapez_core::Color;
    use slotmap::SlotMap;

    fn ids(n: usize) -> Vec<BuildingId> {
        let mut sm: SlotMap<BuildingId, ()> = SlotMap::with_key();
        (0..n).map(|_| sm.insert(())).collect()
    }

    fn straight(ids: &[BuildingId]) -> Vec<BeltEntry> {
        ids.iter()
            .map(|&id| BeltEntry { id, length: TILE })
            .collect()
    }

    const RED: Item = Item::Color(Color::Red);
    const SPEED: u32 = 210;

    #[test]
    fn items_flow_and_stop_at_the_end() {
        let mut path = BeltPath::new(straight(&ids(3)));
        assert!(path.try_accept_item(RED, SPEED));
        assert!(!path.try_accept_item(RED, SPEED), "trop proche");
        for _ in 0..200 {
            path.update(SPEED, Acceptor::None);
            path.try_accept_item(RED, SPEED);
            path.check_integrity().unwrap();
        }
        // 3 tuiles / 0,63 → 4 items + le premier collé au bout.
        assert_eq!(path.len_items(), 5);
        assert!(!path.can_accept_item());
    }

    #[test]
    fn throughput_is_two_items_per_second() {
        let mut path = BeltPath::new(straight(&ids(4)));
        let mut delivered = 0;
        for tick in 0..1200 {
            path.try_accept_item(RED, SPEED);
            let mut take = |_| {
                if tick >= 600 {
                    delivered += 1;
                }
                true
            };
            path.update(SPEED, Acceptor::Other(&mut take));
            path.check_integrity().unwrap();
        }
        // Régime établi : 2 items par seconde, soit 20 en 10 s.
        assert_eq!(delivered, 20);
    }

    #[test]
    fn extend_and_split_keep_items() {
        let belts = ids(5);
        let mut path = BeltPath::new(straight(&belts[..2]));
        path.extend_on_end(BeltEntry {
            id: belts[2],
            length: CURVED_BELT_LENGTH,
        });
        path.extend_on_beginning(BeltEntry {
            id: belts[3],
            length: TILE,
        });
        path.check_integrity().unwrap();
        for _ in 0..400 {
            path.try_accept_item(RED, SPEED);
            path.update(SPEED, Acceptor::None);
        }
        let before = path.len_items();
        let second = path.split_at(1);
        path.check_integrity().unwrap();
        second.check_integrity().unwrap();
        assert!(path.len_items() + second.len_items() < before);
        assert!(path.len_items() + second.len_items() >= before - 2);

        let mut merged = path.clone();
        merged.extend_by_path(second.clone());
        merged.check_integrity().unwrap();
        assert_eq!(merged.len_items(), path.len_items() + second.len_items());
    }

    #[test]
    fn deleting_ends_drops_their_items() {
        let mut path = BeltPath::new(straight(&ids(3)));
        for _ in 0..300 {
            path.try_accept_item(RED, SPEED);
            path.update(SPEED, Acceptor::None);
        }
        let full = path.len_items();
        path.delete_on_end();
        path.check_integrity().unwrap();
        assert!(path.len_items() < full);
        path.delete_on_start();
        path.check_integrity().unwrap();
        assert_eq!(path.total_length(), TILE);
        assert!(path.len_items() <= 2);
    }

    #[test]
    fn self_loop_keeps_circulating() {
        let mut path = BeltPath::new(straight(&ids(4)));
        path.try_accept_item(RED, SPEED);
        for _ in 0..1000 {
            path.update(SPEED, Acceptor::SelfLoop);
            path.check_integrity().unwrap();
        }
        assert_eq!(path.len_items(), 1);
    }
}
