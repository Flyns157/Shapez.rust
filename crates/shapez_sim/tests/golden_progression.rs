//! Paliers d'amélioration et objectifs du freeplay comparés au JS
//! (`tools/js-oracle/progression.mjs`).

use serde_json::Value;
use shapez_sim::upgrades::{UpgradeKind, tiers};

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/progression.json")).unwrap()
}

#[test]
fn upgrade_tiers_match_js() {
    let g = golden();
    for kind in UpgradeKind::ALL {
        let expected = g["upgrades"][kind.js_id()].as_array().unwrap();
        let ours = tiers(kind);
        for (i, tier) in expected.iter().enumerate() {
            let improvement = (tier["improvement"].as_f64().unwrap() * 10_000.0).round() as u64;
            assert_eq!(ours[i].improvement, improvement, "{kind:?} palier {i}");
            let required: Vec<(String, u64)> = tier["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| (r[0].as_str().unwrap().to_owned(), r[1].as_u64().unwrap()))
                .collect();
            let got: Vec<(String, u64)> = ours[i]
                .required
                .iter()
                .map(|&(s, a)| (s.to_owned(), a))
                .collect();
            assert_eq!(got, required, "{kind:?} palier {i}");
        }
    }
}

#[test]
fn freeplay_shapes_match_js() {
    let g = golden();
    for case in g["freeplay"].as_array().unwrap() {
        let seed = case["seed"].as_u64().unwrap() as u32;
        let level = case["level"].as_u64().unwrap() as u32;
        assert_eq!(
            shapez_sim::freeplay::freeplay_shape(seed, level).short_key(),
            case["shape"].as_str().unwrap(),
            "graine {seed}, niveau {level}"
        );
    }
}
