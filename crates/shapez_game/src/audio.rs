//! Effets sonores et musique (port de `platform/sound.js` et `game/sound_proxy.js`).
//!
//! Les effets viennent de `assets/sounds/sfx` (importés par `cargo xtask import-assets`),
//! la musique de `assets/sounds/music` (importée seulement avec `--music`) : sans ces
//! fichiers, le jeu reste simplement muet.

use std::{collections::VecDeque, path::PathBuf};

use bevy::{
    audio::Volume,
    prelude::*,
    ui_widgets::{Activate, Button},
};
use shapez_sim::{building::BuildingKind, placement::SimEvent};

use crate::{
    AppState,
    settings::Settings,
    sim::{SimEventMsg, SimEventsSet},
};

/// `SOUNDS` du JS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    UiClick,
    UiError,
    BadgeNotification,
    LevelComplete,
    DestroyBuilding,
    PlaceBuilding,
    PlaceBelt,
    Copy,
    UnlockUpgrade,
}

impl Sfx {
    pub const fn file(self) -> &'static str {
        match self {
            Self::UiClick => "ui_click",
            Self::UiError => "ui_error",
            Self::BadgeNotification => "badge_notification",
            Self::LevelComplete => "level_complete",
            Self::DestroyBuilding => "destroy_building",
            Self::PlaceBuilding => "place_building",
            Self::PlaceBelt => "place_belt",
            Self::Copy => "copy",
            Self::UnlockUpgrade => "unlock_upgrade",
        }
    }
}

/// Demande de lecture d'un effet.
#[derive(Message, Debug, Clone, Copy)]
pub struct PlaySfx(pub Sfx);

/// Morceaux de `MUSIC` : celui du menu et celui de la partie (`theme-full`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Track {
    Menu,
    Theme,
}

impl Track {
    const fn file(self) -> &'static str {
        match self {
            Self::Menu => "sounds/music/menu.wav",
            Self::Theme => "sounds/music/theme-full.mp3",
        }
    }
}

/// Au plus 5 effets lancés dans la dernière durée moyenne d'un effet
/// (`maxOngoingUiSounds`, `avgSoundDurationSeconds`).
const MAX_ONGOING: usize = 5;
const AVG_SOUND_SECONDS: f64 = 0.1;

/// Limiteur de `SoundProxy.playUi`.
#[derive(Debug, Default)]
pub struct SoundLimiter {
    started: VecDeque<f64>,
}

impl SoundLimiter {
    /// Vrai si un effet peut partir à l'instant `now` (en secondes) ; il est alors compté.
    pub fn try_start(&mut self, now: f64) -> bool {
        while self
            .started
            .front()
            .is_some_and(|&t| now - t > AVG_SOUND_SECONDS)
        {
            self.started.pop_front();
        }
        if self.started.len() > MAX_ONGOING {
            return false;
        }
        self.started.push_back(now);
        true
    }
}

#[derive(Component, Debug)]
struct Music(Track);

#[derive(Debug)]
pub struct AudioPlugin {
    /// Dossier `assets/`, pour savoir si la musique a été importée.
    pub asset_dir: String,
}

#[derive(Resource, Debug)]
struct MusicDir(PathBuf);

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySfx>()
            .insert_resource(MusicDir(PathBuf::from(&self.asset_dir)))
            .add_observer(click_sound)
            .add_systems(
                Update,
                (
                    sim_sounds
                        .after(SimEventsSet)
                        .run_if(in_state(AppState::InGame)),
                    play_sounds,
                    play_music,
                    music_volume,
                )
                    .chain(),
            );
    }
}

/// Les boutons de l'interface cliquent (`ClickDetector`, `clickSound = uiClick`).
fn click_sound(
    activate: On<Activate>,
    buttons: Query<(), With<Button>>,
    mut out: MessageWriter<PlaySfx>,
) {
    if buttons.contains(activate.event_target()) {
        out.write(PlaySfx(Sfx::UiClick));
    }
}

/// Effet d'un évènement de la simulation, comme les parties du HUD du JS.
pub fn sfx_for(event: &SimEvent) -> Option<Sfx> {
    match event {
        SimEvent::BuildingPlaced { building, .. } => Some(match building.kind {
            BuildingKind::Belt | BuildingKind::Wire => Sfx::PlaceBelt,
            _ => Sfx::PlaceBuilding,
        }),
        SimEvent::BuildingRemoved { .. } => Some(Sfx::DestroyBuilding),
        SimEvent::LevelCompleted(_) => Some(Sfx::LevelComplete),
        SimEvent::UpgradePurchased { .. } => Some(Sfx::UnlockUpgrade),
        SimEvent::BuildingChanged { .. } => None,
    }
}

fn sim_sounds(mut events: MessageReader<SimEventMsg>, mut out: MessageWriter<PlaySfx>) {
    for SimEventMsg(event) in events.read() {
        if let Some(sfx) = sfx_for(event) {
            out.write(PlaySfx(sfx));
        }
    }
}

fn play_sounds(
    mut commands: Commands,
    mut requests: MessageReader<PlaySfx>,
    settings: Res<Settings>,
    assets: Res<AssetServer>,
    time: Res<Time<Real>>,
    mut limiter: Local<SoundLimiter>,
) {
    for &PlaySfx(sfx) in requests.read() {
        if settings.sound_volume <= 0.0 || !limiter.try_start(time.elapsed_secs_f64()) {
            continue;
        }
        commands.spawn((
            AudioPlayer::new(assets.load(format!("sounds/sfx/{}.wav", sfx.file()))),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(settings.sound_volume)),
        ));
    }
}

/// `playThemeMusic` : la musique du menu dans les menus, le thème en partie.
fn play_music(
    mut commands: Commands,
    state: Res<State<AppState>>,
    dir: Res<MusicDir>,
    settings: Res<Settings>,
    assets: Res<AssetServer>,
    playing: Query<(Entity, &Music)>,
) {
    let wanted = match state.get() {
        AppState::InGame => Some(Track::Theme),
        AppState::MainMenu
        | AppState::Settings
        | AppState::Keybindings
        | AppState::ShapeGallery => Some(Track::Menu),
        AppState::Boot | AppState::Loading => None,
    }
    .filter(|track| dir.0.join(track.file()).exists());
    let mut already = false;
    for (entity, Music(track)) in &playing {
        if Some(*track) == wanted {
            already = true;
        } else {
            commands.entity(entity).despawn();
        }
    }
    if let Some(track) = wanted
        && !already
    {
        commands.spawn((
            Music(track),
            AudioPlayer::new(assets.load(track.file())),
            PlaybackSettings::LOOP.with_volume(Volume::Linear(settings.music_volume)),
        ));
    }
}

fn music_volume(settings: Res<Settings>, mut sinks: Query<&mut AudioSink, With<Music>>) {
    if !settings.is_changed() {
        return;
    }
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(settings.music_volume));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_allows_bursts_of_six_then_waits() {
        let mut limiter = SoundLimiter::default();
        let started = (0..10).filter(|_| limiter.try_start(1.0)).count();
        assert_eq!(started, MAX_ONGOING + 1);
        assert!(!limiter.try_start(1.05));
        assert!(limiter.try_start(1.2));
    }
}
