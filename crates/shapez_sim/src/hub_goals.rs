//! Objectifs du hub : formes livrées, niveau courant et récompenses
//! (port partiel de `src/js/game/hub_goals.js` et `modes/levels.js`).
//!
//! Les 26 niveaux de l'histoire sont suivis du mode libre (objectifs de débit tirés
//! de la graine), et les formes stockées paient les paliers d'amélioration.

use std::collections::HashMap;

use shapez_core::{ShapeDef, ShapeId, ShapeRegistry};

use crate::analytics::{Analytics, DataSource, SLICE_SECONDS};
use crate::freeplay::{freeplay_required, freeplay_shape};
use crate::units::Upgrades;
use crate::upgrades::{TIER_COUNT, Tier, UpgradeKind, tiers, total_improvement};

/// Récompense de fin de niveau (`enumHubGoalRewards`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Reward {
    CutterAndTrash,
    Rotater,
    Painter,
    Mixer,
    Stacker,
    Balancer,
    Tunnel,
    RotaterCcw,
    Rotater180,
    MinerChainable,
    UndergroundBeltTier2,
    BeltReader,
    Splitter,
    CutterQuad,
    PainterDouble,
    Storage,
    Merger,
    WiresPainterAndLevers,
    Display,
    ConstantSignal,
    LogicGates,
    VirtualProcessing,
    Filter,
    Blueprints,
    Freeplay,
    None,
    /// Objectif du mode libre (`no_reward_freeplay`).
    NoRewardFreeplay,
}

/// Un niveau de l'histoire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelDef {
    pub shape: &'static str,
    pub required: u64,
    pub reward: Reward,
    /// L'objectif est un débit (formes par seconde), pas un total.
    pub throughput_only: bool,
}

const fn lvl(shape: &'static str, required: u64, reward: Reward) -> LevelDef {
    LevelDef {
        shape,
        required,
        reward,
        throughput_only: false,
    }
}

/// `STANDALONE_LEVELS` (version complète, hors variante chinoise).
pub const LEVELS: [LevelDef; 26] = {
    use Reward::*;
    [
        lvl("CuCuCuCu", 30, CutterAndTrash),
        lvl("----CuCu", 40, None),
        lvl("RuRuRuRu", 70, Balancer),
        lvl("RuRu----", 70, Rotater),
        lvl("Cu----Cu", 170, Tunnel),
        lvl("Cu------", 270, Painter),
        lvl("CrCrCrCr", 300, RotaterCcw),
        lvl("RbRb----", 480, Mixer),
        lvl("CpCpCpCp", 600, Merger),
        lvl("ScScScSc", 800, Stacker),
        lvl("CgScScCg", 1000, MinerChainable),
        lvl("CbCbCbRb:CwCwCwCw", 1000, Blueprints),
        lvl("RpRpRpRp:CwCwCwCw", 3800, UndergroundBeltTier2),
        LevelDef {
            shape: "--Cg----:--Cr----",
            required: 8,
            reward: BeltReader,
            throughput_only: true,
        },
        lvl("SrSrSrSr:CyCyCyCy", 10000, Storage),
        lvl("SrSrSrSr:CyCyCyCy:SwSwSwSw", 6000, CutterQuad),
        lvl("CbRbRbCb:CwCwCwCw:WbWbWbWb", 20000, PainterDouble),
        lvl("Sg----Sg:CgCgCgCg:--CyCy--", 20000, Rotater180),
        lvl("CpRpCp--:SwSwSwSw", 25000, Splitter),
        lvl("RuCw--Cw:----Ru--", 25000, WiresPainterAndLevers),
        lvl("CrCwCrCw:CwCrCwCr:CrCwCrCw:CwCrCwCr", 25000, Filter),
        lvl("Cg----Cr:Cw----Cw:Sy------:Cy----Cy", 25000, ConstantSignal),
        lvl("CcSyCcSy:SyCcSyCc:CcSyCcSy", 25000, Display),
        lvl("CcRcCcRc:RwCwRwCw:Sr--Sw--:CyCyCyCy", 25000, LogicGates),
        lvl("Rg--Rg--:CwRwCwRw:--Rg--Rg", 25000, VirtualProcessing),
        lvl("CbCuCbCu:Sr------:--CrSrCr:CwCwCwCw", 50000, Freeplay),
    ]
};

/// Objectif courant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    pub shape: ShapeId,
    pub required: u64,
    pub reward: Reward,
    pub throughput_only: bool,
}

/// Événement de progression, relayé par [`crate::SimEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelCompleted {
    /// Niveau qui vient d'être terminé (1 pour le premier).
    pub level: u32,
    pub reward: Reward,
}

/// État de progression (`HubGoals`, avec `ProductionAnalytics`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HubGoals {
    /// Niveau courant, à partir de 1.
    pub level: u32,
    /// Graine de la carte, pour les formes du mode libre.
    seed: u32,
    stored: HashMap<ShapeId, u64>,
    gained: HashMap<Reward, u32>,
    goal: Goal,
    upgrade_levels: [usize; 4],
    improvements: Upgrades,
    /// Statistiques de production et de livraison.
    pub analytics: Analytics,
}

impl HubGoals {
    pub fn new(seed: u32, registry: &mut ShapeRegistry) -> Self {
        Self {
            level: 1,
            seed,
            stored: HashMap::new(),
            gained: HashMap::new(),
            goal: Self::goal_of_level(1, seed, registry),
            upgrade_levels: [0; 4],
            improvements: Upgrades::default(),
            analytics: Analytics::default(),
        }
    }

    /// Objectif courant.
    pub fn goal(&self) -> Goal {
        self.goal
    }

    /// `isFreePlay`.
    pub fn is_free_play(&self) -> bool {
        self.level as usize >= LEVELS.len()
    }

    /// `getCurrentGoalDelivered` : stock, ou formes livrées par seconde pour un objectif
    /// de débit (sur la dernière tranche complète de statistiques).
    pub fn goal_delivered(&self) -> u64 {
        if self.goal.throughput_only {
            self.analytics
                .current_count(DataSource::Delivered, self.goal.shape)
                / SLICE_SECONDS
        } else {
            self.stored(self.goal.shape)
        }
    }

    /// Toutes les formes en stock.
    pub fn stored_shapes(&self) -> &HashMap<ShapeId, u64> {
        &self.stored
    }

    /// Fin de tick : `productionAnalytics.update`.
    pub fn update_analytics(&mut self, now: u64) {
        self.analytics.update(now, &self.stored);
    }

    /// Paliers achetés dans une famille.
    pub fn upgrade_level(&self, kind: UpgradeKind) -> usize {
        self.upgrade_levels[kind as usize]
    }

    /// Prochain palier à acheter, `None` au maximum.
    pub fn next_tier(&self, kind: UpgradeKind) -> Option<&'static Tier> {
        tiers(kind).get(self.upgrade_level(kind))
    }

    /// Multiplicateurs de vitesse (`upgradeImprovements`).
    pub fn upgrades(&self) -> Upgrades {
        self.improvements
    }

    /// Fixe les paliers achetés (chargement, bac à sable).
    pub fn set_upgrade_level(&mut self, kind: UpgradeKind, level: usize) {
        self.upgrade_levels[kind as usize] = level.min(TIER_COUNT);
        let total = |kind| total_improvement(kind, self.upgrade_level(kind));
        self.improvements = Upgrades {
            belt: total(UpgradeKind::Belt),
            miner: total(UpgradeKind::Miner),
            processors: total(UpgradeKind::Processors),
            painting: total(UpgradeKind::Painting),
        };
    }

    /// `getShapesStoredByKey` (le code court doit être valide).
    pub fn stored_by_key(&self, key: &str, registry: &ShapeRegistry) -> u64 {
        let def = ShapeDef::from_short_key(key).expect("code court valide");
        registry.find(&def).map_or(0, |id| self.stored(id))
    }

    /// `takeShapeByKey` : retire `amount` formes du stock, si elles y sont.
    pub fn take_by_key(&mut self, key: &str, amount: u64, registry: &ShapeRegistry) -> bool {
        if self.stored_by_key(key, registry) < amount {
            return false;
        }
        if amount > 0 {
            let def = ShapeDef::from_short_key(key).expect("code court valide");
            let id = registry.find(&def).expect("forme en stock");
            *self.stored.get_mut(&id).expect("forme en stock") -= amount;
        }
        true
    }

    /// `canUnlockUpgrade`.
    pub fn can_unlock_upgrade(&self, kind: UpgradeKind, registry: &ShapeRegistry) -> bool {
        self.upgrade_level(kind) < TIER_COUNT
            && self.next_tier(kind).is_some_and(|tier| {
                tier.required
                    .iter()
                    .all(|&(key, amount)| self.stored_by_key(key, registry) >= amount)
            })
    }

    /// `tryUnlockUpgrade` : paie le palier et l'applique.
    pub fn try_unlock_upgrade(&mut self, kind: UpgradeKind, registry: &ShapeRegistry) -> bool {
        if !self.can_unlock_upgrade(kind, registry) {
            return false;
        }
        let tier = self.next_tier(kind).expect("palier vérifié");
        for &(key, amount) in &tier.required {
            let def = ShapeDef::from_short_key(key).expect("forme d'amélioration valide");
            let id = registry.find(&def).expect("forme déjà livrée");
            *self.stored.get_mut(&id).expect("forme en stock") -= amount;
        }
        self.set_upgrade_level(kind, self.upgrade_level(kind) + 1);
        true
    }

    /// Donne directement des formes au hub (bac à sable, tests).
    pub fn add_stored(&mut self, shape: ShapeId, amount: u64) {
        *self.stored.entry(shape).or_insert(0) += amount;
    }

    pub fn stored(&self, shape: ShapeId) -> u64 {
        self.stored.get(&shape).copied().unwrap_or(0)
    }

    pub fn is_reward_unlocked(&self, reward: Reward) -> bool {
        self.gained.contains_key(&reward)
    }

    /// Accorde une récompense sans finir de niveau (bac à sable, chargement).
    pub fn grant_reward(&mut self, reward: Reward) {
        *self.gained.entry(reward).or_insert(0) += 1;
    }

    /// `computeNextGoal`.
    fn compute_next_goal(&mut self, registry: &mut ShapeRegistry) {
        self.goal = Self::goal_of_level(self.level, self.seed, registry);
    }

    fn goal_of_level(level: u32, seed: u32, registry: &mut ShapeRegistry) -> Goal {
        match LEVELS.get(level as usize - 1) {
            Some(def) => Goal {
                shape: registry
                    .intern(ShapeDef::from_short_key(def.shape).expect("forme de niveau valide")),
                required: def.required,
                reward: def.reward,
                throughput_only: def.throughput_only,
            },
            None => Goal {
                shape: registry.intern(freeplay_shape(seed, level)),
                required: freeplay_required(level),
                reward: Reward::NoRewardFreeplay,
                throughput_only: true,
            },
        }
    }

    /// `handleDefinitionDelivered` : renvoie le niveau terminé, le cas échéant.
    pub fn deliver(
        &mut self,
        shape: ShapeId,
        registry: &mut ShapeRegistry,
    ) -> Option<LevelCompleted> {
        *self.stored.entry(shape).or_insert(0) += 1;
        self.analytics.on_delivered(shape);
        let goal = self.goal;
        if self.goal_delivered() < goal.required {
            return None;
        }
        *self.gained.entry(goal.reward).or_insert(0) += 1;
        let done = LevelCompleted {
            level: self.level,
            reward: goal.reward,
        };
        self.level += 1;
        self.compute_next_goal(registry);
        Some(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_level_shapes_parse() {
        for def in LEVELS {
            ShapeDef::from_short_key(def.shape).unwrap();
        }
    }

    #[test]
    fn delivering_the_goal_advances_the_level() {
        let mut reg = ShapeRegistry::new();
        let mut goals = HubGoals::new(0, &mut reg);
        let circle = reg.intern(ShapeDef::from_short_key("CuCuCuCu").unwrap());
        let other = reg.intern(ShapeDef::from_short_key("RuRuRuRu").unwrap());
        assert_eq!(goals.deliver(other, &mut reg), None);
        for _ in 0..29 {
            assert_eq!(goals.deliver(circle, &mut reg), None);
        }
        assert_eq!(
            goals.deliver(circle, &mut reg),
            Some(LevelCompleted {
                level: 1,
                reward: Reward::CutterAndTrash
            })
        );
        assert_eq!(goals.level, 2);
        assert!(goals.is_reward_unlocked(Reward::CutterAndTrash));
        assert_eq!(goals.stored(other), 1);
        assert_eq!(reg.get(goals.goal().shape).short_key(), "----CuCu");
    }

    #[test]
    fn freeplay_follows_the_story() {
        let mut reg = ShapeRegistry::new();
        let mut goals = HubGoals::new(0, &mut reg);
        goals.level = 27;
        goals.compute_next_goal(&mut reg);
        assert!(goals.is_free_play());
        let goal = goals.goal();
        assert_eq!(reg.get(goal.shape).short_key(), "SwRgSwRg:RwWyRwWy");
        assert_eq!(goal.required, 4);
        assert!(goal.throughput_only);
    }

    #[test]
    fn throughput_goals_use_the_last_complete_slice() {
        use crate::analytics::SLICE_DURATION;
        let mut reg = ShapeRegistry::new();
        let mut goals = HubGoals::new(0, &mut reg);
        goals.level = 14;
        goals.compute_next_goal(&mut reg);
        let goal = goals.goal();
        assert!(goal.throughput_only);
        // 8 formes/s : 80 livraisons dans une tranche de 10 s.
        for _ in 0..80 {
            assert_eq!(goals.deliver(goal.shape, &mut reg), None);
        }
        goals.update_analytics(SLICE_DURATION + 1);
        assert_eq!(goals.goal_delivered(), 8);
        assert_eq!(
            goals.deliver(goal.shape, &mut reg),
            Some(LevelCompleted {
                level: 14,
                reward: Reward::BeltReader
            })
        );
    }

    #[test]
    fn buying_an_upgrade_spends_shapes() {
        let mut reg = ShapeRegistry::new();
        let mut goals = HubGoals::new(0, &mut reg);
        assert!(!goals.can_unlock_upgrade(UpgradeKind::Belt, &reg));
        let circle = reg.intern(ShapeDef::from_short_key("CuCuCuCu").unwrap());
        goals.add_stored(circle, 35);
        assert!(goals.try_unlock_upgrade(UpgradeKind::Belt, &reg));
        assert_eq!(goals.stored(circle), 5);
        assert_eq!(goals.upgrade_level(UpgradeKind::Belt), 1);
        assert_eq!(goals.upgrades().belt, 15_000);
        assert!(!goals.try_unlock_upgrade(UpgradeKind::Belt, &reg));
    }
}
