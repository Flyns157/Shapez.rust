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
use shapez_core::{Color, Item, Rotation, TilePos};
use shapez_sim::{
    SimWorld,
    building::{BuildingKind, Variant},
    map::{ChunkPos, generate_chunk},
    placement::{Placement, SimCommand},
};

/// Côté d'un anneau : 4 × 24 = 96 tuiles.
const RING_SIDE: i32 = 25;
/// Longueur d'une ligne bloquée.
const LINE_LENGTH: i32 = 50;
/// Items visés par tuile d'anneau : 300 000 items pour 500 000 tuiles (§5.4).
const ITEMS_PER_TILE: f32 = 0.6;

fn belt(world: &mut SimWorld, x: i32, y: i32, rot: Rotation) {
    let tile = TilePos::new(x, y);
    let mut p = Placement::new(BuildingKind::Belt, tile, rot).with_variant(Variant::Default);
    (p.rotation, p.rotation_variant) = world.buildings.optimal_belt(tile, rot);
    world
        .apply(SimCommand::Place(p))
        .unwrap_or_else(|e| panic!("convoyeur en {tile:?} : {e:?}"));
}

/// Anneau dans le sens horaire dont le coin haut-gauche est (x0, y0).
fn ring(world: &mut SimWorld, x0: i32, y0: i32) {
    let n = RING_SIDE - 1;
    for i in 0..n {
        belt(world, x0 + i, y0, Rotation::R90);
        belt(world, x0 + n, y0 + i, Rotation::R180);
        belt(world, x0 + n - i, y0 + n, Rotation::R270);
        belt(world, x0, y0 + n - i, Rotation::R0);
    }
}

/// Dépose des items aux `starts` jusqu'à `per_start` chacun, ou jusqu'à ce que plus rien
/// ne rentre pendant une seconde de jeu.
fn fill(world: &mut SimWorld, starts: &[TilePos], per_start: usize) {
    let mut counts = vec![0; starts.len()];
    let mut idle = 0;
    while idle < 60 && counts.iter().any(|&c| c < per_start) {
        let mut any = false;
        for (start, count) in starts.iter().zip(&mut counts) {
            if *count < per_start && world.inject_item(*start, Item::Color(Color::Red)) {
                *count += 1;
                any = true;
            }
        }
        idle = if any { 0 } else { idle + 1 };
        world.tick();
    }
}

/// Grille d'anneaux couvrant `tiles` tuiles, remplie à [`ITEMS_PER_TILE`].
fn rings_world(tiles: usize) -> SimWorld {
    let mut world = SimWorld::new(1);
    let per_ring = (4 * (RING_SIDE - 1)) as usize;
    let count = tiles / per_ring;
    let columns = (count as f32).sqrt().ceil() as i32;
    let step = RING_SIDE + 1;
    let mut starts = Vec::with_capacity(count);
    for i in 0..count as i32 {
        let (x0, y0) = ((i % columns) * step, (i / columns) * step);
        ring(&mut world, x0, y0);
        starts.push(TilePos::new(x0 + 1, y0));
    }
    fill(
        &mut world,
        &starts,
        (per_ring as f32 * ITEMS_PER_TILE) as usize,
    );
    world
}

/// Lignes parallèles qui débouchent sur le vide, remplies jusqu'à saturation.
fn blocked_world(tiles: usize) -> SimWorld {
    let mut world = SimWorld::new(1);
    let count = tiles / LINE_LENGTH as usize;
    let rows_per_block = 1000;
    let mut starts = Vec::with_capacity(count);
    for i in 0..count as i32 {
        let (x0, y0) = ((i / rows_per_block) * (LINE_LENGTH + 1), i % rows_per_block);
        for x in 0..LINE_LENGTH {
            belt(&mut world, x0 + x, y0, Rotation::R90);
        }
        starts.push(TilePos::new(x0, y0));
    }
    fill(&mut world, &starts, usize::MAX);
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
