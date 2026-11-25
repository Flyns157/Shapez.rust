//! Sauvegarde de la partie en cours : autosave périodique écrite en tâche de fond,
//! et sauvegarde à la sortie (retour au menu ou fermeture de la fenêtre).

use bevy::{prelude::*, tasks::IoTaskPool};
use shapez_save::{SaveMeta, encode, storage::SaveDir};

use crate::{
    AppState,
    sim::{CurrentSave, Sim, SimLifecycle},
};

/// Intervalle de l'autosave (réglage par défaut du jeu d'origine : 2 minutes).
pub const AUTOSAVE_SECONDS: f32 = 120.0;

/// Dossier des sauvegardes ; `None` si la plateforme n'en fournit pas.
#[derive(Resource, Debug, Clone)]
pub struct Saves(pub Option<SaveDir>);

/// Dernière sauvegarde réussie (ou erreur), pour le HUD.
#[derive(Resource, Debug, Default, Clone)]
pub struct LastSave(pub Option<Result<String, String>>);

#[derive(Resource, Debug)]
struct AutosaveTimer(Timer);

#[derive(Debug, Default)]
pub struct SavesPlugin;

impl Plugin for SavesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Saves(SaveDir::default_location()))
            .init_resource::<LastSave>()
            .insert_resource(AutosaveTimer(Timer::from_seconds(
                AUTOSAVE_SECONDS,
                TimerMode::Repeating,
            )))
            .add_systems(
                OnEnter(AppState::InGame),
                name_new_game.after(SimLifecycle::Start),
            )
            .add_systems(
                OnExit(AppState::InGame),
                save_on_exit.before(SimLifecycle::Stop),
            )
            .add_systems(Update, autosave.run_if(in_state(AppState::InGame)))
            .add_systems(Last, save_on_app_exit.run_if(in_state(AppState::InGame)));
    }
}

/// Une nouvelle partie reçoit le premier nom libre.
fn name_new_game(
    mut commands: Commands,
    saves: Res<Saves>,
    current: Option<Res<CurrentSave>>,
    mut timer: ResMut<AutosaveTimer>,
    mut last: ResMut<LastSave>,
) {
    timer.0.reset();
    last.0 = None;
    if current.is_none() {
        let name = saves
            .0
            .as_ref()
            .map_or_else(|| "partie".to_owned(), SaveDir::fresh_name);
        commands.insert_resource(CurrentSave(name));
    }
}

/// Encode le monde ; l'écriture peut ensuite partir en tâche de fond.
fn snapshot(sim: &Sim) -> Result<Vec<u8>, String> {
    encode(sim, &SaveMeta::of(sim)).map_err(|e| e.to_string())
}

fn autosave(
    time: Res<Time<Real>>,
    mut timer: ResMut<AutosaveTimer>,
    saves: Res<Saves>,
    sim: Res<Sim>,
    current: Option<Res<CurrentSave>>,
    mut last: ResMut<LastSave>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    let (Some(dir), Some(current)) = (saves.0.clone(), current) else {
        return;
    };
    match snapshot(&sim) {
        Ok(bytes) => {
            let name = current.0.clone();
            IoTaskPool::get()
                .spawn(async move {
                    if let Err(e) = dir.write(&name, &bytes) {
                        error!("autosave de {name} impossible : {e}");
                    }
                })
                .detach();
            last.0 = Some(Ok(current.0.clone()));
        }
        Err(e) => {
            error!("autosave impossible : {e}");
            last.0 = Some(Err(e));
        }
    }
}

/// Écriture synchrone : l'application ou la partie se ferme juste après.
fn save_now(saves: &Saves, sim: &Sim, current: Option<&CurrentSave>) {
    let (Some(dir), Some(current)) = (&saves.0, current) else {
        return;
    };
    match snapshot(sim).and_then(|bytes| dir.write(&current.0, &bytes).map_err(|e| e.to_string())) {
        Ok(()) => info!(
            "partie sauvegardée dans {}",
            dir.path_of(&current.0).display()
        ),
        Err(e) => error!("sauvegarde de {} impossible : {e}", current.0),
    }
}

fn save_on_exit(saves: Res<Saves>, sim: Option<Res<Sim>>, current: Option<Res<CurrentSave>>) {
    if let Some(sim) = sim {
        save_now(&saves, &sim, current.as_deref());
    }
}

fn save_on_app_exit(
    mut exits: MessageReader<AppExit>,
    saves: Res<Saves>,
    sim: Option<Res<Sim>>,
    current: Option<Res<CurrentSave>>,
) {
    if exits.read().next().is_some()
        && let Some(sim) = sim
    {
        save_now(&saves, &sim, current.as_deref());
    }
}
