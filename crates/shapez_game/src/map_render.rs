//! Rendu de la carte par chunk : fond, ressources et vue d'ensemble au dézoom
//! (port de `systems/map_resources.js` et `map_chunk_view.js`).

use std::collections::{HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use shapez_core::TilePos;
use shapez_sim::{
    building::Layer,
    map::{CHUNK_SIZE, ChunkPos, ChunkResources, MapResource},
    placement::{Buildings, SimEvent},
};

use crate::{
    AppState,
    camera::{MainCamera, TILE_SIZE, Zoom, world_to_tile},
    screenshot::Exporting,
    settings::Settings,
    shape_textures::ShapeTextures,
    silhouettes::{self, OVERLAY_RES},
    sim::{Sim, SimEventMsg, SimEventsSet},
    theme::{Theme, active},
};

const CHUNK_WORLD_SIZE: f32 = CHUNK_SIZE as f32 * TILE_SIZE;
/// Diamètre d'un item posé sur la carte (`defaultItemDiameter`).
const ITEM_DIAMETER: f32 = 20.0;
/// Chunks gardés en plus autour de l'écran, pour éviter de les recréer sans cesse.
const MARGIN_CHUNKS: i32 = 1;

#[derive(Resource, Debug, Default)]
struct VisibleChunks(HashMap<ChunkPos, Entity>);

/// Entité d'un chunk ; porte les deux images (normale et vue d'ensemble).
#[derive(Component, Debug)]
struct ChunkView {
    pos: ChunkPos,
    background: Handle<Image>,
    overview: Handle<Image>,
}

/// Icône d'une ressource sur une tuile (vue normale).
#[derive(Component, Debug)]
struct ResourceIcon;

/// Icône d'un gisement de formes (vue d'ensemble).
#[derive(Component, Debug)]
struct PatchIcon;

#[derive(Debug, Default)]
pub struct MapRenderPlugin;

impl Plugin for MapRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisibleChunks>()
            .add_systems(OnExit(AppState::InGame), despawn_all_chunks)
            .add_systems(
                Update,
                (
                    sync_visible_chunks,
                    refresh_overviews,
                    apply_overview,
                    draw_grid,
                )
                    .chain()
                    .after(crate::camera::CameraSet)
                    .after(SimEventsSet)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Image 16 × 16 (un pixel par tuile) : ressources à 50 % d'opacité.
fn chunk_image(chunk: &ChunkResources, theme: &Theme) -> Image {
    let n = CHUNK_SIZE as usize;
    let mut data = Vec::with_capacity(n * n * 4);
    for y in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let px = match chunk.get(x, y) {
                Some(r) => {
                    let [r, g, b] = theme.resource_color(r);
                    [r, g, b, 0x80]
                }
                None => [0, 0, 0, 0],
            };
            data.extend_from_slice(&px);
        }
    }
    rgba_image(n, data)
}

/// Vue d'ensemble d'un chunk (`generateOverlayBuffer`), 3 × 3 pixels par tuile : fond
/// `chunkOverview.empty`, ou `filled` s'il contient un bâtiment, ressources opaques,
/// puis silhouettes des bâtiments de la couche normale.
fn overview_image(
    chunk: &ChunkResources,
    pos: ChunkPos,
    buildings: &Buildings,
    theme: &Theme,
) -> Image {
    let n = CHUNK_SIZE as usize * OVERLAY_RES;
    let origin = pos.origin();
    let tile = |x: i32, y: i32| TilePos::new(origin.x + x, origin.y + y);
    let tiles = || (0..CHUNK_SIZE).flat_map(|y| (0..CHUNK_SIZE).map(move |x| (x, y)));
    let filled = tiles().any(|(x, y)| buildings.contents_at(tile(x, y)).next().is_some());
    let [r, g, b] = if filled {
        theme.overview_filled
    } else {
        theme.overview_empty
    };
    let mut data: Vec<u8> = [r, g, b, 0xff].repeat(n * n);
    let mut paint = |px: usize, py: usize, [r, g, b]: [u8; 3]| {
        let i = (py * n + px) * 4;
        data[i..i + 4].copy_from_slice(&[r, g, b, 0xff]);
    };
    for (x, y) in tiles() {
        let (px, py) = (x as usize * OVERLAY_RES, y as usize * OVERLAY_RES);
        let fill = |color: [u8; 3], paint: &mut dyn FnMut(usize, usize, [u8; 3])| {
            for dy in 0..OVERLAY_RES {
                for dx in 0..OVERLAY_RES {
                    paint(px + dx, py + dy, color);
                }
            }
        };
        let resource = chunk.get(x, y).map(|r| theme.resource_color(r));
        match buildings.building_at_layer(tile(x, y), Layer::Regular) {
            Some((_, building)) => {
                let color = silhouettes::color(building, theme);
                match silhouettes::matrix(building) {
                    Some(matrix) => {
                        // La ressource « transparaît » sous le motif.
                        if let Some(resource) = resource {
                            fill(resource, &mut paint);
                        }
                        for (k, &on) in matrix.iter().enumerate() {
                            if on != 0 {
                                paint(px + k % OVERLAY_RES, py + k / OVERLAY_RES, color);
                            }
                        }
                    }
                    None => fill(color, &mut paint),
                }
            }
            None => {
                if let Some(resource) = resource {
                    fill(resource, &mut paint);
                }
            }
        }
    }
    rgba_image(n, data)
}

fn rgba_image(n: usize, data: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: n as u32,
            height: n as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

/// Chunks qui recouvrent l'écran (plus une marge).
pub(crate) fn visible_range(camera_pos: Vec2, half_extent: Vec2) -> (ChunkPos, ChunkPos) {
    let min = world_to_tile(camera_pos + Vec2::new(-half_extent.x, half_extent.y));
    let max = world_to_tile(camera_pos + Vec2::new(half_extent.x, -half_extent.y));
    let to_chunk = |t: f32| (t / CHUNK_SIZE as f32).floor() as i32;
    (
        ChunkPos::new(
            to_chunk(min.x) - MARGIN_CHUNKS,
            to_chunk(min.y) - MARGIN_CHUNKS,
        ),
        ChunkPos::new(
            to_chunk(max.x) + MARGIN_CHUNKS,
            to_chunk(max.y) + MARGIN_CHUNKS,
        ),
    )
}

#[allow(clippy::too_many_arguments)]
fn sync_visible_chunks(
    mut commands: Commands,
    mut visible: ResMut<VisibleChunks>,
    mut sim: ResMut<Sim>,
    mut images: ResMut<Assets<Image>>,
    mut textures: ResMut<ShapeTextures>,
    zoom: Res<Zoom>,
    window: Single<&Window>,
    camera: Single<&Transform, With<MainCamera>>,
    settings: Res<Settings>,
    exporting: Option<Res<Exporting>>,
) {
    let half = Vec2::new(window.width(), window.height()) / (2.0 * zoom.0);
    let (min, max) = visible_range(camera.translation.truncate(), half);
    let theme = active(&settings);
    // Un changement de thème redessine tous les chunks.
    let retheme = settings.is_changed();
    // Pendant un export, toute la base reste chargée.
    let export = exporting.map(|e| e.area);
    let wanted = |pos: &ChunkPos| {
        ((min.x..=max.x).contains(&pos.x) && (min.y..=max.y).contains(&pos.y))
            || export.is_some_and(|area| area.contains(*pos))
    };

    visible.0.retain(|pos, entity| {
        let keep = !retheme && wanted(pos);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });

    let overview = zoom.is_overview() && export.is_none();
    let screen =
        (min.x..=max.x).flat_map(|cx| (min.y..=max.y).map(move |cy| ChunkPos::new(cx, cy)));
    let exported = export.into_iter().flat_map(|area| {
        (area.min.x..area.max.x)
            .flat_map(move |cx| (area.min.y..area.max.y).map(move |cy| ChunkPos::new(cx, cy)))
    });
    for pos in screen.chain(exported) {
        if visible.0.contains_key(&pos) {
            continue;
        }
        let chunk = sim.map.chunk(pos).clone();
        let background = images.add(chunk_image(&chunk, theme));
        let overview_image = images.add(overview_image(&chunk, pos, &sim.buildings, theme));
        let origin = pos.origin();
        let center = Vec2::new(
            (origin.x as f32 + CHUNK_SIZE as f32 / 2.0) * TILE_SIZE,
            -(origin.y as f32 + CHUNK_SIZE as f32 / 2.0) * TILE_SIZE,
        );

        let mut entity = commands.spawn((
            ChunkView {
                pos,
                background: background.clone(),
                overview: overview_image.clone(),
            },
            Sprite {
                image: if overview { overview_image } else { background },
                custom_size: Some(Vec2::splat(CHUNK_WORLD_SIZE)),
                ..default()
            },
            Transform::from_translation(center.extend(0.0)),
        ));
        if !chunk.is_empty() {
            entity.with_children(|parent| {
                spawn_resource_icons(parent, &chunk, &mut textures, &mut images, overview);
            });
        }
        visible.0.insert(pos, entity.id());
    }
}

fn spawn_resource_icons(
    parent: &mut ChildSpawnerCommands,
    chunk: &ChunkResources,
    textures: &mut ShapeTextures,
    images: &mut Assets<Image>,
    overview: bool,
) {
    let half = CHUNK_SIZE as f32 / 2.0;
    // Position locale au chunk (centre du chunk = origine).
    let local = |x: f32, y: f32| Vec3::new((x - half) * TILE_SIZE, -(y - half) * TILE_SIZE, 1.0);
    let normal = if overview {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    let over = if overview {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            let Some(resource) = chunk.get(x, y) else {
                continue;
            };
            parent.spawn((
                ResourceIcon,
                Sprite {
                    image: textures.resource(resource, images),
                    custom_size: Some(Vec2::splat(ITEM_DIAMETER)),
                    color: Color::WHITE.with_alpha(0.5),
                    ..default()
                },
                Transform::from_translation(local(x as f32 + 0.5, y as f32 + 0.5)),
                normal,
            ));
        }
    }
    for patch in chunk.patches() {
        if let MapResource::Shape(_) = patch.resource {
            parent.spawn((
                PatchIcon,
                Sprite {
                    image: textures.resource(patch.resource, images),
                    custom_size: Some(Vec2::splat(ITEM_DIAMETER * 4.0)),
                    ..default()
                },
                Transform::from_translation(local(patch.center.0 as f32, patch.center.1 as f32)),
                over,
            ));
        }
    }
}

/// Redessine la vue d'ensemble des chunks chargés dont un bâtiment a changé.
fn refresh_overviews(
    mut events: MessageReader<SimEventMsg>,
    mut sim: ResMut<Sim>,
    settings: Res<Settings>,
    chunks: Query<&ChunkView>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut dirty = HashSet::new();
    for SimEventMsg(event) in events.read() {
        if let SimEvent::BuildingPlaced { building, .. }
        | SimEvent::BuildingChanged { building, .. }
        | SimEvent::BuildingRemoved { building, .. } = event
        {
            let (min, (w, h)) = building.bounds();
            let chunk = |t: i32| t.div_euclid(CHUNK_SIZE);
            for cx in chunk(min.x)..=chunk(min.x + w - 1) {
                for cy in chunk(min.y)..=chunk(min.y + h - 1) {
                    dirty.insert(ChunkPos::new(cx, cy));
                }
            }
        }
    }
    if dirty.is_empty() {
        return;
    }
    let theme = active(&settings);
    for view in chunks.iter().filter(|v| dirty.contains(&v.pos)) {
        let chunk = sim.map.chunk(view.pos).clone();
        let image = overview_image(&chunk, view.pos, &sim.buildings, theme);
        let _ = images.insert(&view.overview, image);
    }
}

/// Bascule entre vue normale et vue d'ensemble quand le zoom franchit le seuil.
fn apply_overview(
    zoom: Res<Zoom>,
    exporting: Option<Res<Exporting>>,
    mut chunks: Query<(&ChunkView, &mut Sprite)>,
    mut icons: Query<&mut Visibility, (With<ResourceIcon>, Without<PatchIcon>)>,
    mut patches: Query<&mut Visibility, (With<PatchIcon>, Without<ResourceIcon>)>,
    mut was_overview: Local<Option<bool>>,
) {
    let overview = zoom.is_overview() && exporting.is_none();
    if *was_overview == Some(overview) {
        return;
    }
    *was_overview = Some(overview);
    for (view, mut sprite) in &mut chunks {
        sprite.image = if overview {
            view.overview.clone()
        } else {
            view.background.clone()
        };
    }
    let (normal, over) = if overview {
        (Visibility::Hidden, Visibility::Inherited)
    } else {
        (Visibility::Inherited, Visibility::Hidden)
    };
    icons.iter_mut().for_each(|mut v| *v = normal);
    patches.iter_mut().for_each(|mut v| *v = over);
}

/// Grille des tuiles (`THEME.map.gridRegular`), seulement en vue normale.
fn draw_grid(
    mut gizmos: Gizmos,
    zoom: Res<Zoom>,
    window: Single<&Window>,
    camera: Single<&Transform, With<MainCamera>>,
    settings: Res<Settings>,
    exporting: Option<Res<Exporting>>,
) {
    // L'image exportée n'a pas de grille, comme dans le JS.
    if zoom.is_overview() || exporting.is_some() {
        return;
    }
    let half = Vec2::new(window.width(), window.height()) / (2.0 * zoom.0);
    let center = camera.translation.truncate();
    let cells = (half * 2.0 / TILE_SIZE).ceil() + Vec2::splat(2.0);
    // Aligne la grille sur les tuiles.
    let snapped = (center / TILE_SIZE).round() * TILE_SIZE;
    gizmos.grid_2d(
        Isometry2d::from_translation(snapped),
        UVec2::new(cells.x as u32, cells.y as u32),
        Vec2::splat(TILE_SIZE),
        active(&settings).grid,
    );
}

fn despawn_all_chunks(mut commands: Commands, mut visible: ResMut<VisibleChunks>) {
    for (_, entity) in visible.0.drain() {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_range_covers_screen() {
        // Écran de 1280 × 720 centré sur l'origine, zoom 1 : x de -20 à 20 tuiles.
        let (min, max) = visible_range(Vec2::ZERO, Vec2::new(640.0, 360.0));
        assert_eq!((min.x, max.x), (-2 - MARGIN_CHUNKS, 1 + MARGIN_CHUNKS));
        assert_eq!((min.y, max.y), (-1 - MARGIN_CHUNKS, MARGIN_CHUNKS));
    }
}
