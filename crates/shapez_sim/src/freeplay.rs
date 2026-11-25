//! Formes du mode libre (`HubGoals.computeFreeplayShape`).

use shapez_core::{Alea, Color, Layer, Quadrant, ShapeDef, SubShape};

/// `generateRandomColorSet`.
fn random_color_set(rng: &mut Alea, allow_uncolored: bool) -> Vec<Color> {
    use Color::*;
    const WHEEL: [Color; 8] = [Red, Yellow, Green, Cyan, Blue, Purple, Red, Yellow];
    let universal: &[Color] = if allow_uncolored {
        &[White, Uncolored]
    } else {
        &[White]
    };
    let index = rng.next_int_range(0.0, (WHEEL.len() - 2) as f64) as usize;
    let mut picked = WHEEL[index..index + 3].to_vec();
    picked.push(*rng.choice(universal));
    picked
}

/// Forme demandée au niveau `level` (≥ 27) pour la graine de carte `seed`.
pub fn freeplay_shape(seed: u32, level: u32) -> ShapeDef {
    // `clamp(level / 25, 2, 4)` n'est pas entier : la boucle `i < layerCount` arrondit
    // au-dessus.
    let layer_count = (f64::from(level) / 25.0).clamp(2.0, 4.0).ceil() as usize;
    let mut rng = Alea::new(&format!("{seed}/{level}"));
    let colors = random_color_set(&mut rng, level > 35);

    let mut shapes = vec![SubShape::Rect, SubShape::Circle, SubShape::Star];
    const RADIAL: &[&[usize]] = &[&[0, 2], &[1, 3]];
    const OTHERS: [&[&[usize]]; 4] = [
        &[&[0, 3], &[1, 2]],
        &[&[0, 1], &[2, 3]],
        &[&[0, 2], &[1], &[3]],
        &[&[1, 3], &[0], &[2]],
    ];
    let symmetry: &[&[usize]] = if rng.next_f64() < 0.5 {
        shapes.push(SubShape::Windmill);
        RADIAL
    } else {
        rng.choice(&OTHERS)
    };

    let mut any_missing_two = false;
    let mut layers: Vec<Layer> = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let mut layer: Layer = [None; 4];
        for group in symmetry {
            let sub = *rng.choice(&shapes);
            let color = *rng.choice(&colors);
            for &quad in *group {
                layer[quad] = Some(Quadrant::new(sub, color));
            }
        }
        if level > 75 && rng.next_f64() > 0.95 && !any_missing_two {
            layer[rng.next_int_range(0.0, 4.0) as usize] = None;
            any_missing_two = true;
        }
        layers.push(layer);
    }
    ShapeDef::from_layers(&layers)
}

/// Quantité demandée (formes par seconde) : `min(200, floor(4 + (level - 27) / 4))`.
pub fn freeplay_required(level: u32) -> u64 {
    u64::from(4 + level.saturating_sub(27) / 4).min(200)
}
