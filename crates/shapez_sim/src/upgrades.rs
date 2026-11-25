//! Paliers d'amélioration du shop (port de `generateUpgrades`, `modes/regular.js`).
//!
//! Chaque famille a 7 paliers fixes puis 992 paliers « infinis ». Un palier demande
//! ses propres formes et, sauf `excludePrevious`, celles des paliers précédents, dont
//! les quantités grossissent de ×2,5 (arrondies par `findNiceIntegerValue`).

use std::sync::OnceLock;

use crate::units::IMPROVEMENT_ONE;

/// Famille d'améliorations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum UpgradeKind {
    Belt,
    Miner,
    Processors,
    Painting,
}

impl UpgradeKind {
    pub const ALL: [Self; 4] = [Self::Belt, Self::Miner, Self::Processors, Self::Painting];

    /// Identifiant du JS (`upgradeLevels` des sauvegardes).
    pub const fn js_id(self) -> &'static str {
        match self {
            Self::Belt => "belt",
            Self::Miner => "miner",
            Self::Processors => "processors",
            Self::Painting => "painting",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// Un palier : gain de vitesse et formes à payer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tier {
    /// Gain additif, en dix-millièmes (+0.5 → 5 000).
    pub improvement: u64,
    /// `(code court, quantité)`.
    pub required: Vec<(&'static str, u64)>,
}

/// Nombre total de paliers par famille (`1000 - 1` dans le JS).
pub const TIER_COUNT: usize = 999;

const PREPAREMENT_SHAPE: &str = "CpRpCp--:SwSwSwSw";
const FINAL_GAME_SHAPE: &str = "RuCw--Cw:----Ru--";
const ROCKET_SHAPE: &str = "CbCuCbCu:Sr------:--CrSrCr:CwCwCwCw";
const TIER_GROWTH: f64 = 2.5;

/// Paliers fixes de chaque famille, sans les exigences héritées.
fn fixed_tiers(kind: UpgradeKind) -> [&'static [(&'static str, u64)]; 7] {
    let common: [&'static [(&'static str, u64)]; 2] = [
        &[(PREPAREMENT_SHAPE, 25000)],
        &[(PREPAREMENT_SHAPE, 25000), (FINAL_GAME_SHAPE, 50000)],
    ];
    let own: [&'static [(&'static str, u64)]; 5] = match kind {
        UpgradeKind::Belt => [
            &[("CuCuCuCu", 30)],
            &[("--CuCu--", 500)],
            &[("CpCpCpCp", 1000)],
            &[("SrSrSrSr:CyCyCyCy", 6000)],
            &[("SrSrSrSr:CyCyCyCy:SwSwSwSw", 25000)],
        ],
        UpgradeKind::Miner => [
            &[("RuRuRuRu", 300)],
            &[("Cu------", 800)],
            &[("ScScScSc", 3500)],
            &[("CwCwCwCw:WbWbWbWb", 23000)],
            &[("CbRbRbCb:CwCwCwCw:WbWbWbWb", 50000)],
        ],
        UpgradeKind::Processors => [
            &[("SuSuSuSu", 500)],
            &[("RuRu----", 600)],
            &[("CgScScCg", 3500)],
            &[("CwCrCwCr:SgSgSgSg", 25000)],
            &[("WrRgWrRg:CwCrCwCr:SgSgSgSg", 50000)],
        ],
        UpgradeKind::Painting => [
            &[("RbRb----", 600)],
            &[("WrWrWrWr", 3800)],
            &[("RpRpRpRp:CwCwCwCw", 6500)],
            &[("WpWpWpWp:CwCwCwCw:WpWpWpWp", 25000)],
            &[("WpWpWpWp:CwCwCwCw:WpWpWpWp:CwCwCwCw", 50000)],
        ],
    };
    [own[0], own[1], own[2], own[3], own[4], common[0], common[1]]
}

/// `fixedImprovements` puis les gains décroissants des paliers infinis.
fn improvement(tier: usize) -> u64 {
    const FIXED: [u64; 7] = [5000, 5000, 10000, 10000, 20000, 10000, 10000];
    match tier {
        0..7 => FIXED[tier],
        _ => match tier - 7 {
            0..20 => 1000,
            20..50 => 500,
            50..100 => 250,
            _ => 125,
        },
    }
}

/// `findNiceValue`, pour les quantités.
fn find_nice_value(num: f64) -> f64 {
    if num > 1e8 {
        return num;
    }
    if num < 0.00001 {
        return 0.0;
    }
    let round = if num > 50000.0 {
        10000.0
    } else if num > 20000.0 {
        5000.0
    } else if num > 5000.0 {
        1000.0
    } else if num > 2000.0 {
        500.0
    } else if num > 1000.0 {
        100.0
    } else if num > 100.0 {
        20.0
    } else if num > 20.0 {
        5.0
    } else {
        1.0
    };
    let nice = (num / round).floor() * round;
    if num >= 10.0 {
        js_round(nice)
    } else if num >= 1.0 {
        js_round(nice * 10.0) / 10.0
    } else {
        js_round(nice * 100.0) / 100.0
    }
}

fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `findNiceIntegerValue`.
fn find_nice_integer_value(num: f64) -> f64 {
    find_nice_value(num).ceil()
}

fn generate(kind: UpgradeKind) -> Vec<Tier> {
    let fixed = fixed_tiers(kind);
    let mut tiers = Vec::with_capacity(TIER_COUNT);
    // Exigences cumulées (`currentTierRequirements`), reprises par chaque palier sauf
    // `excludePrevious`.
    let mut current: Vec<(&'static str, f64)> = Vec::new();
    for i in 0..TIER_COUNT {
        let (own, exclude_previous): (Vec<(&'static str, u64)>, bool) = match fixed.get(i) {
            Some(tier) => (tier.to_vec(), i >= 5),
            None => {
                let k = (i - 7) as u64;
                (
                    vec![
                        (PREPAREMENT_SHAPE, 30000 + k * 10000),
                        (FINAL_GAME_SHAPE, 20000 + k * 5000),
                        (ROCKET_SHAPE, 20000 + k * 5000),
                    ],
                    true,
                )
            }
        };
        let mut required = Vec::new();
        if !exclude_previous {
            required.extend(current.iter().map(|&(s, a)| (s, a as u64)));
        }
        required.extend(own.iter().copied());
        tiers.push(Tier {
            improvement: improvement(i),
            required,
        });
        // Les paliers infinis ne réutilisent jamais ces exigences : inutile de faire
        // grossir une liste de milliers d'entrées.
        if i < 7 {
            current.extend(own.iter().map(|&(s, a)| (s, a as f64)));
            for entry in &mut current {
                entry.1 = find_nice_integer_value(entry.1 * TIER_GROWTH);
            }
        }
    }
    tiers
}

/// Tous les paliers d'une famille.
pub fn tiers(kind: UpgradeKind) -> &'static [Tier] {
    static CACHE: OnceLock<[Vec<Tier>; 4]> = OnceLock::new();
    &CACHE.get_or_init(|| UpgradeKind::ALL.map(generate))[kind.index()]
}

/// Multiplicateur total après `level` paliers achetés (`upgradeImprovements`).
pub fn total_improvement(kind: UpgradeKind, level: usize) -> u64 {
    IMPROVEMENT_ONE
        + tiers(kind)[..level.min(TIER_COUNT)]
            .iter()
            .map(|t| t.improvement)
            .sum::<u64>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_tiers_match_the_game() {
        let belt = tiers(UpgradeKind::Belt);
        assert_eq!(belt.len(), TIER_COUNT);
        assert_eq!(belt[0].required, vec![("CuCuCuCu", 30)]);
        assert_eq!(belt[1].required, vec![("CuCuCuCu", 75), ("--CuCu--", 500)]);
        assert_eq!(total_improvement(UpgradeKind::Belt, 2), 2 * IMPROVEMENT_ONE);
    }
}
