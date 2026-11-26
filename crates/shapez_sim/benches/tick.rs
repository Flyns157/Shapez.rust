//! Benchmarks de la simulation (plan §9.1 et objectifs du §5.4) : un tick complet sur des
//! usines de 10 000, 100 000 et 500 000 tuiles de convoyeur, et la génération d'un chunk.
//!
//! - `rings` : des anneaux fermés de convoyeurs, chargés d'items qui tournent sans fin.
//!   Tous les items bougent à chaque tick : c'est le pire cas, que les chemins dormants
//!   n'aident pas.
//! - `blocked` : des lignes saturées qui débouchent sur le vide. Plus rien ne bouge :
//!   c'est le cas que les chemins dormants rendent presque gratuit ; `blocked_awake`
//!   mesure la même usine sans eux.
//!
//! `cargo bench -p shapez_sim` ; `SHAPEZ_BENCH_FULL=1` ajoute les cartes de 500 000
//! tuiles, longues à préparer.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use shapez_core::TilePos;
use shapez_sim::{
    SimWorld,
    map::{ChunkPos, generate_chunk},
    stress,
};

fn rings_world(tiles: usize) -> SimWorld {
    let mut world = SimWorld::new(1);
    stress::add_rings(&mut world, TilePos::new(0, 0), tiles);
    world
}

fn blocked_world(tiles: usize) -> SimWorld {
    let mut world = SimWorld::new(1);
    stress::add_blocked_lines(&mut world, TilePos::new(0, 0), tiles);
    world
}

fn sizes() -> Vec<usize> {
    let mut sizes = vec![10_000, 100_000];
    if std::env::var_os("SHAPEZ_BENCH_FULL").is_some() {
        sizes.push(500_000);
    }
    sizes
}

fn tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick");
    group.sample_size(20);
    for tiles in sizes() {
        let mut world = rings_world(tiles);
        eprintln!(
            "rings/{tiles} : {} chemins, {} items",
            world.factory.paths().count(),
            world.factory.item_count()
        );
        group.bench_with_input(BenchmarkId::new("rings", tiles), &tiles, |b, _| {
            b.iter(|| world.tick());
        });

        let mut world = blocked_world(tiles);
        eprintln!(
            "blocked/{tiles} : {} chemins, {} items",
            world.factory.paths().count(),
            world.factory.item_count()
        );
        group.bench_with_input(BenchmarkId::new("blocked", tiles), &tiles, |b, _| {
            b.iter(|| world.tick());
        });

        // Même usine sans chemins dormants, pour mesurer leur gain.
        world.factory.disable_dormancy();
        group.bench_with_input(BenchmarkId::new("blocked_awake", tiles), &tiles, |b, _| {
            b.iter(|| world.tick());
        });
    }
    group.finish();
}

fn map(c: &mut Criterion) {
    c.bench_function("generate_chunk", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            black_box(generate_chunk(1234, ChunkPos::new(i % 50, i / 50 % 50)))
        });
    });
}

criterion_group!(benches, tick, map);
criterion_main!(benches);
