//! Compare `BeltPath` au code JS d'origine (`tools/js-oracle/belts.mjs`).
//!
//! Le générateur fait tourner le JS dans nos unités entières (1 tuile = 10 000) : les
//! deux implémentations doivent donner exactement le même état à chaque relevé.

use serde::Deserialize;
use shapez_core::{Color, Item};
use shapez_sim::{
    belt_path::{Acceptor, BeltEntry, BeltPath},
    placement::BuildingId,
    units::{CURVED_BELT_LENGTH, TILE},
};
use slotmap::SlotMap;

#[derive(Deserialize)]
struct Golden {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    belts: Vec<bool>,
    steps: String,
    ops: Vec<Op>,
    snapshots: Vec<Snapshot>,
}

#[derive(Deserialize)]
struct Op {
    t: usize,
    op: String,
    index: Option<usize>,
    second: Option<Snapshot>,
}

#[derive(Deserialize, Debug)]
struct Snapshot {
    tick: usize,
    total: u32,
    spacing: u32,
    items: Vec<u32>,
}

const SPEED: u32 = 210;

fn check(path: &BeltPath, expected: &Snapshot, case: usize) {
    let items: Vec<u32> = path.item_distances().collect();
    let ctx = || {
        format!(
            "cas {case}, tick {} : attendu {expected:?}, obtenu spacing {} items {items:?}",
            expected.tick,
            path.spacing_to_first()
        )
    };
    assert_eq!(path.total_length(), expected.total, "{}", ctx());
    assert_eq!(path.spacing_to_first(), expected.spacing, "{}", ctx());
    assert_eq!(items, expected.items, "{}", ctx());
}

#[test]
fn belt_paths_match_the_js_oracle() {
    // `SHAPEZ_GOLDEN_BELTS` permet de vérifier un fichier plus gros, généré à la main.
    let json = match std::env::var("SHAPEZ_GOLDEN_BELTS") {
        Ok(path) => std::fs::read_to_string(path).expect("fichier golden lisible"),
        Err(_) => include_str!("golden/belts.json").to_string(),
    };
    let golden: Golden = serde_json::from_str(&json).expect("golden valide");
    let mut ids: SlotMap<BuildingId, ()> = SlotMap::with_key();
    let mut entry = |curved: bool| BeltEntry {
        id: ids.insert(()),
        length: if curved { CURVED_BELT_LENGTH } else { TILE },
    };
    for (n, case) in golden.cases.iter().enumerate() {
        let mut path = BeltPath::new(case.belts.iter().map(|&c| entry(c)).collect());
        let mut ops = case.ops.iter().peekable();
        let mut snapshots = case.snapshots.iter().peekable();
        for (t, step) in case.steps.bytes().enumerate() {
            let bits = step - b'0';
            if let Some(op) = ops.next_if(|o| o.t == t) {
                match op.op.as_str() {
                    "end" => path.extend_on_end(entry(false)),
                    "end_curved" => path.extend_on_end(entry(true)),
                    "begin" => path.extend_on_beginning(entry(false)),
                    "delete_start" => path.delete_on_start(),
                    "delete_end" => path.delete_on_end(),
                    "split" => {
                        let second = path.split_at(op.index.unwrap());
                        check(&second, op.second.as_ref().unwrap(), n);
                    }
                    other => panic!("opération inconnue {other}"),
                }
                path.on_path_changed(path.target);
            }
            if bits & 1 != 0 {
                path.try_accept_item(Item::Color(Color::Red), SPEED);
            }
            let open = bits & 2 != 0;
            let mut accept = |_| open;
            path.update(SPEED, Acceptor::Other(&mut accept));
            if let Some(s) = snapshots.next_if(|s| s.tick == t) {
                check(&path, s, n);
            }
        }
        assert!(snapshots.next().is_none());
    }
}
