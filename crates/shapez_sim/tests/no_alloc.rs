//! Un tick de simulation n'alloue rien une fois l'usine en régime établi (profilage du
//! jalon M10, plan §3.3) : c'est ce qui rend une arène `bumpalo` inutile. Ce test
//! garde cette propriété.

use shapez_core::TilePos;
use shapez_sim::{SimWorld, analytics::SLICE_DURATION, analytics::SLICES, stress};

// Allocateur qui compte (dhat) : `unsafe_code` reste interdit dans le workspace.
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn steady_state_tick_does_not_allocate() {
    let _profiler = dhat::Profiler::builder().testing().build();

    // Hub (un processeur), anneaux chargés qui tournent, lignes saturées et dormantes.
    let mut world = SimWorld::new_game(1);
    stress::add_rings(&mut world, TilePos::new(4, -12), 2_000);
    stress::add_blocked_lines(&mut world, TilePos::new(-60, 10), 1_000);
    // Historique des statistiques plein : ensuite, une tranche en remplace une autre.
    let warmup = (SLICES as u64 + 1) * SLICE_DURATION / shapez_sim::units::TICK;
    for _ in 0..warmup {
        world.tick();
    }

    let before = dhat::HeapStats::get().total_blocks;
    for _ in 0..600 {
        world.tick();
    }
    let allocations = dhat::HeapStats::get().total_blocks - before;
    assert_eq!(allocations, 0, "{allocations} allocations en 600 ticks");
}
