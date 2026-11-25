//! Objectifs du hub : formes livrées, niveau courant et récompenses
//! (port partiel de `src/js/game/hub_goals.js` et `modes/levels.js`).
//!
//! Le freeplay, les objectifs de débit (`throughputOnly`) et le shop d'améliorations
//! arrivent en M5 ; ici on joue les niveaux de l'histoire jusqu'au 26.

use std::collections::HashMap;

use shapez_core::{ShapeDef, ShapeId, ShapeRegistry};

/// Récompense de fin de niveau (`enumHubGoalRewards`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// État de progression (`HubGoals`).
#[derive(Debug, Clone)]
pub struct HubGoals {
    /// Niveau courant, à partir de 1.
    pub level: u32,
    stored: HashMap<ShapeId, u64>,
    gained: HashMap<Reward, u32>,
    goal: Option<Goal>,
}

impl HubGoals {
    pub fn new(registry: &mut ShapeRegistry) -> Self {
        let mut goals = Self {
            level: 1,
            stored: HashMap::new(),
            gained: HashMap::new(),
            goal: None,
        };
        goals.compute_next_goal(registry);
        goals
    }

    /// Objectif courant, `None` une fois l'histoire terminée (le freeplay arrive en M5).
    pub fn goal(&self) -> Option<Goal> {
        self.goal
    }

    pub fn stored(&self, shape: ShapeId) -> u64 {
        self.stored.get(&shape).copied().unwrap_or(0)
    }

    pub fn is_reward_unlocked(&self, reward: Reward) -> bool {
        self.gained.contains_key(&reward)
    }

    /// `computeNextGoal`.
    fn compute_next_goal(&mut self, registry: &mut ShapeRegistry) {
        self.goal = LEVELS.get(self.level as usize - 1).map(|def| Goal {
            shape: registry
                .intern(ShapeDef::from_short_key(def.shape).expect("forme de niveau valide")),
            required: def.required,
            reward: def.reward,
            throughput_only: def.throughput_only,
        });
    }

    /// `handleDefinitionDelivered` : renvoie le niveau terminé, le cas échéant.
    pub fn deliver(
        &mut self,
        shape: ShapeId,
        registry: &mut ShapeRegistry,
    ) -> Option<LevelCompleted> {
        *self.stored.entry(shape).or_insert(0) += 1;
        let goal = self.goal?;
        // Les objectifs de débit ont besoin des statistiques (M5).
        if goal.throughput_only || self.stored(goal.shape) < goal.required {
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
        let mut goals = HubGoals::new(&mut reg);
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
        assert_eq!(reg.get(goals.goal().unwrap().shape).short_key(), "----CuCu");
    }
}
