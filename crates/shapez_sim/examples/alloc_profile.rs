//! Profil des allocations de la simulation (plan §3.3, jalon M10) : combien
//! d'allocations un tick fait-il, sur les usines de charge des benchmarks ?
//!
//! `cargo run --release -p shapez_sim --example alloc_profile`

use shapez_core::TilePos;
use shapez_sim::{SimWorld, stress};

// dhat compte chaque allocation ; `unsafe_code` reste interdit dans le workspace.
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn counters() -> (u64, u64) {
    let stats = dhat::HeapStats::get();
    (stats.total_blocks, stats.total_bytes)
}

fn measure(name: &str, world: &mut SimWorld) {
    const TICKS: u64 = 600;
    // Une seconde de chauffe : capacités des Vec réutilisées déjà atteintes.
    for _ in 0..60 {
        world.tick();
    }
    let (a0, b0) = counters();
    for _ in 0..TICKS {
        world.tick();
    }
    let (a1, b1) = counters();
    let (a, b) = (a1 - a0, b1 - b0);
    println!(
        "{name:<16} {:>10.1} allocations/tick {:>12.0} octets/tick",
        a as f64 / TICKS as f64,
        b as f64 / TICKS as f64
    );
}

fn main() {
    // Mode test : compte sans écrire de fichier de profil.
    let _profiler = dhat::Profiler::builder().testing().build();
    for tiles in [10_000, 100_000] {
        let mut world = SimWorld::new(1);
        stress::add_rings(&mut world, TilePos::new(0, 0), tiles);
        measure(&format!("rings/{tiles}"), &mut world);

        let mut world = SimWorld::new(1);
        stress::add_blocked_lines(&mut world, TilePos::new(0, 0), tiles);
        measure(&format!("blocked/{tiles}"), &mut world);
    }
    let mut world = SimWorld::new_game(1);
    measure("new_game", &mut world);
}
