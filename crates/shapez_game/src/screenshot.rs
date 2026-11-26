//! Export de toute la base en image (port de `hud/parts/screenshot_exporter.js`, touche
//! F3) : une seconde caméra rend la zone des bâtiments dans une image, enregistrée en PNG
//! dans le dossier des captures.
//!
//! Le JS prévient d'abord que l'export peut être long ; ici, il ne bloque pas le jeu :
//! une notification annonce le début, une autre le fichier écrit.

use std::path::PathBuf;

use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
};
use shapez_core::TilePos;
use shapez_sim::map::{CHUNK_SIZE, ChunkPos};

use crate::{
    AppState,
    camera::TILE_SIZE,
    editing_ui::Editor,
    i18n::{t, tf},
    settings::Settings,
    sim::Sim,
    theme::active,
};

/// Taille d'un chunk dans l'image tant que la base est petite (`chunkSizePixels`).
const CHUNK_PIXELS: u32 = 128;
/// Au-delà de 128 chunks de côté, le JS réduit les chunks pour garder l'image à
/// 128 × 128 chunks de 128 pixels.
const FULL_SIZE_CHUNKS: u32 = 128;
/// Côté maximal d'une texture garanti par wgpu (`max_texture_dimension_2d` par défaut) ;
/// le canvas du JS n'avait pas cette limite.
const MAX_TEXTURE_SIDE: u32 = 8192;
/// Images rendues avant la capture, le temps que les chunks de la zone apparaissent.
const WARMUP_FRAMES: u8 = 3;

/// Zone exportée, en chunks : `min` inclus, `max` exclu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportArea {
    pub min: ChunkPos,
    pub max: ChunkPos,
    /// Côté d'un chunk dans l'image, en pixels.
    pub chunk_pixels: u32,
}

impl ExportArea {
    /// `doExport` : la boîte des bâtiments, toujours avec l'origine, arrondie aux chunks.
    pub fn around(bounds: impl IntoIterator<Item = (TilePos, (i32, i32))>) -> Self {
        let (mut min, mut max) = ((0, 0), (0, 0));
        for (origin, (w, h)) in bounds {
            min = (min.0.min(origin.x), min.1.min(origin.y));
            max = (max.0.max(origin.x + w), max.1.max(origin.y + h));
        }
        let min = ChunkPos::new(min.0.div_euclid(CHUNK_SIZE), min.1.div_euclid(CHUNK_SIZE));
        let ceil = |v: i32| -(-v).div_euclid(CHUNK_SIZE);
        // Au moins un chunk, pour ne jamais demander une image vide.
        let max = ChunkPos::new(ceil(max.0).max(min.x + 1), ceil(max.1).max(min.y + 1));
        let side = (max.x - min.x).max(max.y - min.y) as u32;
        let mut chunk_pixels = CHUNK_PIXELS;
        if side > FULL_SIZE_CHUNKS {
            chunk_pixels = (CHUNK_PIXELS * FULL_SIZE_CHUNKS / side).max(1);
        }
        chunk_pixels = chunk_pixels.min(MAX_TEXTURE_SIDE / side).max(1);
        Self {
            min,
            max,
            chunk_pixels,
        }
    }

    /// Taille de l'image en pixels.
    pub fn size(&self) -> UVec2 {
        UVec2::new(
            (self.max.x - self.min.x) as u32,
            (self.max.y - self.min.y) as u32,
        ) * self.chunk_pixels
    }

    /// Vrai si le chunk est dans la zone.
    pub fn contains(&self, chunk: ChunkPos) -> bool {
        (self.min.x..self.max.x).contains(&chunk.x) && (self.min.y..self.max.y).contains(&chunk.y)
    }

    /// Centre de la zone en coordonnées monde Bevy (Y vers le haut).
    fn world_center(&self) -> Vec2 {
        let chunk = CHUNK_SIZE as f32 * TILE_SIZE;
        Vec2::new(
            (self.min.x + self.max.x) as f32 / 2.0 * chunk,
            -(self.min.y + self.max.y) as f32 / 2.0 * chunk,
        )
    }

    /// Unités monde par pixel de l'image.
    fn world_per_pixel(&self) -> f32 {
        CHUNK_SIZE as f32 * TILE_SIZE / self.chunk_pixels as f32
    }
}

/// Export en cours : la carte garde les chunks de la zone et reste en vue normale.
#[derive(Resource, Debug)]
pub struct Exporting {
    pub area: ExportArea,
    camera: Entity,
    image: Handle<Image>,
    path: PathBuf,
    frames_left: u8,
    captured: bool,
}

/// Dossier des captures : `SHAPEZ_SCREENSHOT_DIR`, sinon `screenshots` dans le dossier
/// de données de la plateforme (à côté des sauvegardes).
pub fn screenshot_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SHAPEZ_SCREENSHOT_DIR") {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("", "", "shapez-rs")
        .map(|dirs| dirs.data_dir().join("screenshots"))
        .unwrap_or_else(|| PathBuf::from("screenshots"))
}

/// Nom du fichier : `base-<secondes depuis 1970>.png`, le JS proposait `base.png`.
fn file_name() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("base-{secs}.png")
}

/// Message envoyé une fois l'image écrite (ou l'écriture ratée).
#[derive(Message, Debug, Clone)]
struct ExportDone;

#[derive(Debug, Default)]
pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ExportDone>()
            .add_systems(OnExit(AppState::InGame), cancel_export)
            .add_systems(
                Update,
                (start_export, capture, finish_export)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn start_export(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    exporting: Option<Res<Exporting>>,
    sim: Res<Sim>,
    settings: Res<Settings>,
    mut images: ResMut<Assets<Image>>,
    mut editor: ResMut<Editor>,
) {
    if !keys.just_pressed(KeyCode::F3) || exporting.is_some() {
        return;
    }
    let area = ExportArea::around(sim.buildings.iter().map(|(_, b)| b.bounds()));
    let size = area.size();
    let image = images.add(Image::new_target_texture(
        size.x,
        size.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                // Avant la caméra de jeu, qui n'a pas besoin de cette image.
                order: -1,
                clear_color: ClearColorConfig::Custom(active(&settings).background),
                ..default()
            },
            RenderTarget::Image(ImageRenderTarget::from(image.clone())),
            Projection::Orthographic(OrthographicProjection {
                scale: area.world_per_pixel(),
                ..OrthographicProjection::default_2d()
            }),
            Transform::from_translation(area.world_center().extend(0.0)),
        ))
        .id();
    let path = screenshot_dir().join(file_name());
    info!(
        "export de la base : {}x{} pixels vers {}",
        size.x,
        size.y,
        path.display()
    );
    editor.say(t("port.notify.screenshotStarted"));
    commands.insert_resource(Exporting {
        area,
        camera,
        image,
        path,
        frames_left: WARMUP_FRAMES,
        captured: false,
    });
}

/// Après quelques images, capture le rendu de la seconde caméra.
fn capture(mut commands: Commands, exporting: Option<ResMut<Exporting>>) {
    let Some(mut exporting) = exporting else {
        return;
    };
    if exporting.captured {
        return;
    }
    if exporting.frames_left > 0 {
        exporting.frames_left -= 1;
        return;
    }
    exporting.captured = true;
    if let Some(dir) = exporting.path.parent()
        && let Err(err) = std::fs::create_dir_all(dir)
    {
        warn!("dossier des captures {} : {err}", dir.display());
    }
    commands
        .spawn(Screenshot::image(exporting.image.clone()))
        .observe(save_to_disk(exporting.path.clone()))
        .observe(
            |_: On<ScreenshotCaptured>, mut done: MessageWriter<ExportDone>| {
                done.write(ExportDone);
            },
        );
}

fn finish_export(
    mut commands: Commands,
    mut done: MessageReader<ExportDone>,
    exporting: Option<Res<Exporting>>,
    mut images: ResMut<Assets<Image>>,
    mut editor: ResMut<Editor>,
) {
    if done.read().count() == 0 {
        return;
    }
    let Some(exporting) = exporting else {
        return;
    };
    // `save_to_disk` ne signale pas ses erreurs : le fichier fait foi.
    let path = exporting.path.display().to_string();
    editor.say(if exporting.path.exists() {
        tf("port.notify.screenshotSaved", &[("path", &path)])
    } else {
        tf("port.notify.screenshotFailed", &[("path", &path)])
    });
    commands.entity(exporting.camera).despawn();
    images.remove(&exporting.image);
    commands.remove_resource::<Exporting>();
}

fn cancel_export(
    mut commands: Commands,
    exporting: Option<Res<Exporting>>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(exporting) = exporting {
        commands.entity(exporting.camera).despawn();
        images.remove(&exporting.image);
        commands.remove_resource::<Exporting>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: i32, y: i32, w: i32, h: i32) -> (TilePos, (i32, i32)) {
        (TilePos::new(x, y), (w, h))
    }

    #[test]
    fn area_always_includes_the_origin_and_rounds_to_chunks() {
        // Le hub du JS : 4 × 4 tuiles autour de l'origine.
        let area = ExportArea::around([bounds(-2, -2, 4, 4)]);
        assert_eq!(area.min, ChunkPos::new(-1, -1));
        assert_eq!(area.max, ChunkPos::new(1, 1));
        assert_eq!(area.chunk_pixels, 128);
        assert_eq!(area.size(), UVec2::new(256, 256));

        let area = ExportArea::around([bounds(20, 3, 1, 1)]);
        assert_eq!(area.min, ChunkPos::new(0, 0));
        assert_eq!(area.max, ChunkPos::new(2, 1));
        assert!(area.contains(ChunkPos::new(1, 0)));
        assert!(!area.contains(ChunkPos::new(2, 0)));
    }

    #[test]
    fn empty_base_still_gives_one_chunk() {
        let area = ExportArea::around([]);
        assert_eq!(area.size(), UVec2::new(128, 128));
    }

    #[test]
    fn big_bases_shrink_chunks_to_fit_a_texture() {
        // 100 chunks : 128 px ne tient pas dans 8192, on descend à 81.
        let area = ExportArea::around([bounds(0, 0, 1600, 16)]);
        assert_eq!(area.chunk_pixels, 81);
        assert!(area.size().x <= MAX_TEXTURE_SIDE);
        // 300 chunks : la règle du JS (128 × 128 / 300 = 54) puis la limite (27).
        let area = ExportArea::around([bounds(0, 0, 4800, 16)]);
        assert_eq!(area.chunk_pixels, 27);
        assert!(area.size().x <= MAX_TEXTURE_SIDE);
    }
}
