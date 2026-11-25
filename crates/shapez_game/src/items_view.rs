//! Rendu des items sur les convoyeurs et en sortie de bâtiment.
//!
//! Un réservoir de sprites est réutilisé d'une image à l'autre : on n'en crée que
//! lorsque le nombre d'items visibles augmente.

use bevy::prelude::*;
use shapez_core::Item;

use crate::{
    AppState,
    buildings_view::ITEM_Z,
    camera::{TILE_SIZE, tile_to_world},
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

fn draw_items(
    mut commands: Commands,
    sim: Res<Sim>,
    mut pool: ResMut<ItemPool>,
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<(&mut Sprite, &mut Transform, &mut Visibility), With<ItemSprite>>,
) {
    let items = sim.factory.visible_items(&sim.buildings);
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

fn clear_pool(mut commands: Commands, mut pool: ResMut<ItemPool>) {
    for entity in pool.0.drain(..) {
        commands.entity(entity).despawn();
    }
}
