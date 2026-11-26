//! Rendu des items sur les convoyeurs et en sortie de bâtiment.
//!
//! Seuls les items à l'écran sont demandés à la simulation, et aucun en vue d'ensemble
//! (le JS ne les dessine pas non plus à ce zoom) : le coût suit l'écran, pas l'usine.
//! Pendant l'export de la base (F3), c'est toute la zone exportée.
//!
//! Un réservoir de sprites est réutilisé d'une image à l'autre : on n'en crée que
//! lorsque le nombre d'items visibles augmente.

use bevy::prelude::*;
use shapez_core::{Item, TilePos};
use shapez_sim::map::CHUNK_SIZE;

use crate::{
    AppState,
    buildings_view::ITEM_Z,
    camera::{MainCamera, TILE_SIZE, Zoom, tile_to_world, world_to_tile},
    screenshot::Exporting,
    shape_textures::ShapeTextures,
    sim::Sim,
};

/// `globalConfig.defaultItemDiameter` (20 px pour une tuile de 32).
const ITEM_DIAMETER: f32 = TILE_SIZE * 20.0 / 32.0;

#[derive(Component, Debug)]
struct ItemSprite;

#[derive(Resource, Debug, Default)]
struct ItemPool(Vec<Entity>);

#[derive(Debug, Default)]
pub struct ItemsViewPlugin;

impl Plugin for ItemsViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemPool>()
            .add_systems(OnExit(AppState::InGame), clear_pool)
            .add_systems(Update, draw_items.run_if(in_state(AppState::InGame)));
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_items(
    mut commands: Commands,
    sim: Res<Sim>,
    mut pool: ResMut<ItemPool>,
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<(&mut Sprite, &mut Transform, &mut Visibility), With<ItemSprite>>,
    zoom: Res<Zoom>,
    window: Single<&Window>,
    camera: Single<&Transform, (With<MainCamera>, Without<ItemSprite>)>,
    exporting: Option<Res<Exporting>>,
) {
    let items = if let Some(export) = exporting {
        let area = &export.area;
        let min = TilePos::new(area.min.x * CHUNK_SIZE, area.min.y * CHUNK_SIZE);
        let max = TilePos::new(area.max.x * CHUNK_SIZE - 1, area.max.y * CHUNK_SIZE - 1);
        sim.factory.visible_items_in(&sim.buildings, min, max)
    } else if zoom.is_overview() {
        Vec::new()
    } else {
        let (min, max) = screen_tiles(camera.translation.truncate(), &window, zoom.0);
        sim.factory.visible_items_in(&sim.buildings, min, max)
    };
    for (i, visible) in items.iter().enumerate() {
        let image = match visible.item {
            Item::Shape(id) => textures.get_or_render(*sim.registry.get(id), &mut images),
            Item::Color(c) => textures.get_or_render_color(c, &mut images),
            Item::Bool(_) => continue,
        };
        let translation = tile_to_world(visible.x, visible.y).extend(ITEM_Z);
        if let Some(&entity) = pool.0.get(i)
            && let Ok((mut sprite, mut transform, mut visibility)) = sprites.get_mut(entity)
        {
            if sprite.image != image {
                sprite.image = image;
            }
            transform.translation = translation;
            *visibility = Visibility::Inherited;
        } else {
            let entity = commands
                .spawn((
                    ItemSprite,
                    Sprite {
                        image,
                        custom_size: Some(Vec2::splat(ITEM_DIAMETER)),
                        ..default()
                    },
                    Transform::from_translation(translation),
                    Visibility::Inherited,
                ))
                .id();
            if i < pool.0.len() {
                pool.0[i] = entity;
            } else {
                pool.0.push(entity);
            }
        }
    }
    for &entity in pool.0.iter().skip(items.len()) {
        if let Ok((_, _, mut visibility)) = sprites.get_mut(entity) {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Tuiles à l'écran, plus une de marge : un item dépasse un peu de son convoyeur.
fn screen_tiles(center: Vec2, window: &Window, zoom: f32) -> (TilePos, TilePos) {
    let half = Vec2::new(window.width(), window.height()) / (2.0 * zoom);
    let a = world_to_tile(center - half);
    let b = world_to_tile(center + half);
    let (lo, hi) = (a.min(b), a.max(b));
    (
        TilePos::new(lo.x.floor() as i32 - 1, lo.y.floor() as i32 - 1),
        TilePos::new(hi.x.ceil() as i32 + 1, hi.y.ceil() as i32 + 1),
    )
}

fn clear_pool(mut commands: Commands, mut pool: ResMut<ItemPool>) {
    for entity in pool.0.drain(..) {
        commands.entity(entity).despawn();
    }
}
