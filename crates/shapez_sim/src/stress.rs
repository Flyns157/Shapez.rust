//! Usines de charge pour les benchmarks et le profilage du jeu (`SHAPEZ_STRESS`) :
//! des anneaux de convoyeurs chargés d'items et des lignes saturées, construits
//! uniquement par des [`SimCommand`], comme le ferait le joueur.

use shapez_core::{Color, Item, Rotation, TilePos};

use crate::{
    SimWorld,
    building::{BuildingKind, Variant},
    placement::{Placement, SimCommand},
};

/// Côté d'un anneau : 4 × 24 = 96 tuiles.
pub const RING_SIDE: i32 = 25;
/// Longueur d'une ligne bloquée.
pub const LINE_LENGTH: i32 = 50;
/// Items visés par tuile d'anneau : 300 000 items pour 500 000 tuiles (plan §5.4).
pub const ITEMS_PER_TILE: f32 = 0.6;

fn belt(world: &mut SimWorld, x: i32, y: i32, rot: Rotation) {
    let tile = TilePos::new(x, y);
    let mut p = Placement::new(BuildingKind::Belt, tile, rot).with_variant(Variant::Default);
    (p.rotation, p.rotation_variant) = world.buildings.optimal_belt(tile, rot);
    // Une tuile déjà prise (gisement couvert par le hub…) est simplement sautée.
    let _ = world.apply(SimCommand::Place(p));
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
/// ne rentre pendant une seconde de jeu. Le monde avance pendant ce temps.
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

/// Grille d'anneaux d'environ `tiles` tuiles à partir de `origin`, remplie à
/// [`ITEMS_PER_TILE`].
pub fn add_rings(world: &mut SimWorld, origin: TilePos, tiles: usize) {
    let per_ring = (4 * (RING_SIDE - 1)) as usize;
    let count = tiles / per_ring;
    let columns = (count as f32).sqrt().ceil() as i32;
    let step = RING_SIDE + 1;
    let mut starts = Vec::with_capacity(count);
    for i in 0..count as i32 {
        let (x0, y0) = (
            origin.x + (i % columns) * step,
            origin.y + (i / columns) * step,
        );
        ring(world, x0, y0);
        starts.push(TilePos::new(x0 + 1, y0));
    }
    fill(world, &starts, (per_ring as f32 * ITEMS_PER_TILE) as usize);
}

/// Lignes parallèles d'environ `tiles` tuiles qui débouchent sur le vide, remplies
/// jusqu'à saturation.
pub fn add_blocked_lines(world: &mut SimWorld, origin: TilePos, tiles: usize) {
    let count = tiles / LINE_LENGTH as usize;
    let rows_per_block = 1000;
    let mut starts = Vec::with_capacity(count);
    for i in 0..count as i32 {
        let x0 = origin.x + (i / rows_per_block) * (LINE_LENGTH + 1);
        let y0 = origin.y + i % rows_per_block;
        for x in 0..LINE_LENGTH {
            belt(world, x0 + x, y0, Rotation::R90);
        }
        starts.push(TilePos::new(x0, y0));
    }
    fill(world, &starts, usize::MAX);
}
