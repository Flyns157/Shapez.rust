//! Pont entre Bevy et `shapez_sim` : le monde avance dans `FixedUpdate` à 60 Hz.

use bevy::prelude::*;
use shapez_sim::{SimWorld, TICKS_PER_SECOND, placement::SimEvent};

use crate::AppState;

/// Monde simulé, possédé par Bevy comme ressource.
#[derive(Resource, Debug, Default, Deref, DerefMut)]
pub struct Sim(pub SimWorld);

/// Vitesse de jeu : nombre de ticks exécutés par pas fixe (0 = pause).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameSpeed(pub u32);

impl Default for GameSpeed {
    fn default() -> Self {
        Self(1)
    }
}

/// Partie à lancer au prochain passage en jeu, choisie par le menu.
#[derive(Resource, Debug, Default)]
pub enum NextGame {
    #[default]
    New,
    /// Partie chargée depuis la sauvegarde `name`.
    Load { name: String, world: Box<SimWorld> },
}

/// Nom de la sauvegarde de la partie en cours.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct CurrentSave(pub String);

/// Mise en place et retrait du monde ; les sauvegardes s'ordonnent autour.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimLifecycle {
    Start,
    Stop,
}

/// Événement de la simulation, relayé aux systèmes de rendu et d'interface.
#[derive(Message, Debug, Clone, Copy)]
pub struct SimEventMsg(pub SimEvent);

/// Les systèmes qui lisent [`SimEventMsg`] s'exécutent après ce set.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimEventsSet;

#[derive(Debug, Default)]
pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(f64::from(TICKS_PER_SECOND)))
            .init_resource::<GameSpeed>()
            .init_resource::<NextGame>()
            .add_systems(
                OnEnter(AppState::InGame),
                start_world.in_set(SimLifecycle::Start),
            )
            .add_systems(
                OnExit(AppState::InGame),
                (|mut commands: Commands| {
                    commands.remove_resource::<Sim>();
                    commands.remove_resource::<CurrentSave>();
                })
                .in_set(SimLifecycle::Stop),
            )
            .add_message::<SimEventMsg>()
            .add_systems(FixedUpdate, step_sim.run_if(in_state(AppState::InGame)))
            .add_systems(
                Update,
                forward_events
                    .in_set(SimEventsSet)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn start_world(mut commands: Commands, mut next: ResMut<NextGame>) {
    match std::mem::take(&mut *next) {
        NextGame::New => {
            let mut world = SimWorld::new_game(new_game_seed());
            if std::env::var_os("SHAPEZ_SANDBOX").is_some() {
                sandbox(&mut world);
            }
            commands.insert_resource(Sim(world));
        }
        NextGame::Load { name, world } => {
            commands.insert_resource(Sim(*world));
            commands.insert_resource(CurrentSave(name));
        }
    }
}

/// Bac à sable de développement (`SHAPEZ_SANDBOX=1`) : toutes les récompenses de
/// l'histoire et de quoi payer les plans, pour essayer chaque outil sans jouer.
fn sandbox(world: &mut SimWorld) {
    for def in shapez_sim::hub_goals::LEVELS {
        world.hub.grant_reward(def.reward);
    }
    let shape = world.registry.intern(
        shapez_core::ShapeDef::from_short_key(shapez_sim::editing::BLUEPRINT_SHAPE)
            .expect("forme de plan valide"),
    );
    world.hub.add_stored(shape, 1_000_000);
}

/// Graine d'une nouvelle partie, entre 0 et 100 000 comme `randomInt(0, 100000)` du JS.
/// `SHAPEZ_SEED` permet de la fixer (pour comparer avec une partie JS).
fn new_game_seed() -> u32 {
    if let Some(seed) = std::env::var("SHAPEZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        return seed;
    }
    let nanos = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    nanos % 100_001
}

fn forward_events(mut sim: ResMut<Sim>, mut out: MessageWriter<SimEventMsg>) {
    for event in sim.drain_events() {
        out.write(SimEventMsg(event));
    }
}

/// On ne modifie jamais `dt` : accélérer revient à exécuter plusieurs ticks.
fn step_sim(mut sim: ResMut<Sim>, speed: Res<GameSpeed>) {
    sim.advance(speed.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    fn app_in_game(speed: u32) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, SimPlugin))
            .insert_state(AppState::InGame)
            .insert_resource(GameSpeed(speed));
        app.update();
        app
    }

    #[test]
    fn speed_runs_n_ticks_per_fixed_step() {
        let mut app = app_in_game(4);
        let before = app.world().resource::<Sim>().tick_count();
        app.world_mut().run_schedule(FixedUpdate);
        let after = app.world().resource::<Sim>().tick_count();
        assert_eq!(after - before, 4);
    }

    #[test]
    fn paused_runs_no_tick() {
        let mut app = app_in_game(0);
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(app.world().resource::<Sim>().tick_count(), 0);
    }
}
