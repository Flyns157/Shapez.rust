//! Paramètres de l'application (port partiel de `profile/application_settings.js`) :
//! langue, volumes des effets et de la musique, plein écran. Ils sont enregistrés en
//! JSON à chaque changement.

use std::path::{Path, PathBuf};

use bevy::{
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, WindowMode},
};
use serde::{Deserialize, Serialize};

use crate::theme::ThemeId;

/// Pas des boutons de volume.
pub const VOLUME_STEP: f32 = 0.1;

/// Paramètres enregistrés. Un champ absent du fichier prend sa valeur par défaut.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Identifiant de langue du jeu d'origine (`fr`, `pt-BR`…), `None` = langue du
    /// système (`auto-detect` du JS).
    pub language: Option<String>,
    /// Volume des effets, de 0 à 1.
    pub sound_volume: f32,
    /// Volume de la musique, de 0 à 1.
    pub music_volume: f32,
    pub fullscreen: bool,
    pub theme: ThemeId,
    /// `enableColorBlindHelper`.
    pub color_blind: bool,
}

impl Default for Settings {
    /// Valeurs du JS, sauf le plein écran : désactivé tant que le portage se teste
    /// surtout en fenêtre.
    fn default() -> Self {
        Self {
            language: None,
            sound_volume: 1.0,
            music_volume: 1.0,
            fullscreen: false,
            theme: ThemeId::Dark,
            color_blind: false,
        }
    }
}

impl Settings {
    /// Fichier des paramètres : `SHAPEZ_SETTINGS`, sinon le dossier de configuration
    /// de la plateforme (`~/.config/shapez-rs/settings.json` sous Linux), ou
    /// `settings.json` dans le `localStorage` du navigateur.
    pub fn default_path() -> Option<PathBuf> {
        if cfg!(target_arch = "wasm32") {
            return Some(PathBuf::from("settings.json"));
        }
        if let Some(path) = std::env::var_os("SHAPEZ_SETTINGS") {
            return Some(PathBuf::from(path));
        }
        directories::ProjectDirs::from("", "", "shapez-rs")
            .map(|dirs| dirs.config_dir().join("settings.json"))
    }

    /// Lit `path` ; un fichier absent ou illisible donne les valeurs par défaut.
    pub fn load(path: &Path) -> Self {
        match shapez_save::disk::read(path) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes)
                .map(Self::clamped)
                .unwrap_or_else(|e| {
                    warn!("paramètres illisibles ({}) : {e}", path.display());
                    Self::default()
                }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        shapez_save::disk::write_atomic(path, text.as_bytes())
    }

    fn clamped(mut self) -> Self {
        self.sound_volume = clamp_volume(self.sound_volume);
        self.music_volume = clamp_volume(self.music_volume);
        self
    }
}

/// Volume ramené entre 0 et 1 et arrondi au pas des boutons.
pub fn clamp_volume(v: f32) -> f32 {
    if v.is_nan() {
        return 1.0;
    }
    ((v.clamp(0.0, 1.0) / VOLUME_STEP).round() * VOLUME_STEP).clamp(0.0, 1.0)
}

/// Langue des paramètres au lancement : une autre demande de relancer le jeu.
#[derive(Resource, Debug, Clone)]
pub struct LaunchLanguage(pub Option<String>);

/// Emplacement du fichier, `None` si la plateforme n'en fournit pas.
#[derive(Resource, Debug, Clone)]
pub struct SettingsPath(pub Option<PathBuf>);

#[derive(Debug)]
pub struct SettingsPlugin {
    pub settings: Settings,
    pub path: Option<PathBuf>,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.settings.clone())
            .insert_resource(SettingsPath(self.path.clone()))
            .insert_resource(LaunchLanguage(self.settings.language.clone()))
            .add_systems(Update, (save_settings, apply_fullscreen));
    }
}

/// Enregistre les paramètres quand ils changent.
fn save_settings(settings: Res<Settings>, path: Res<SettingsPath>) {
    if !settings.is_changed() || settings.is_added() {
        return;
    }
    if let Some(path) = &path.0
        && let Err(e) = settings.save(path)
    {
        warn!("paramètres non enregistrés ({}) : {e}", path.display());
    }
}

fn apply_fullscreen(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    let wanted = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != wanted {
        window.mode = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_fill_missing_fields() {
        let dir = std::env::temp_dir().join(format!("shapez-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let settings = Settings {
            language: Some("de".into()),
            sound_volume: 0.3,
            music_volume: 0.0,
            fullscreen: true,
            theme: ThemeId::Light,
            color_blind: true,
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);

        std::fs::write(&path, r#"{"musicVolume": 7}"#).unwrap();
        let partial = Settings::load(&path);
        assert_eq!(partial.music_volume, 1.0);
        assert_eq!(partial.sound_volume, 1.0);
        assert_eq!(partial.language, None);

        std::fs::write(&path, "pas du json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn volumes_snap_to_the_button_step() {
        assert_eq!(clamp_volume(0.34), 0.3);
        assert_eq!(clamp_volume(-1.0), 0.0);
        assert_eq!(clamp_volume(f32::NAN), 1.0);
        assert!((clamp_volume(0.7) - 0.7).abs() < 1e-6);
    }
}
