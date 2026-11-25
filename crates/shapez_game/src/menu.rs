//! Menu principal minimal et écran de jeu provisoire (affiche le compteur de ticks).

use bevy::{
    prelude::*,
    ui_widgets::{Activate, ActivateOnPress, Button},
};

use shapez_sim::{hub_goals::Reward, placement::SimEvent};

use crate::{
    AppState,
    placement_ui::PlacementState,
    sim::{GameSpeed, Sim, SimEventMsg, SimEventsSet},
};

/// Durée d'affichage d'un message de fin de niveau, en secondes.
const NOTIFICATION_SECONDS: f32 = 6.0;

#[derive(Debug, Default)]
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_menu)
            .add_observer(start_game)
            .add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (
                    update_hud,
                    notify_levels.after(SimEventsSet),
                    game_keys.before(crate::camera::CameraSet),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Component, Debug)]
struct PlayButton;

#[derive(Component, Debug)]
struct GalleryButton;

#[derive(Component, Debug)]
struct TickText;

#[derive(Component, Debug)]
struct GoalText;

#[derive(Component, Debug)]
struct NotificationText;

#[derive(Component, Debug)]
struct NotificationTimer(Timer);

fn spawn_menu(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::MainMenu),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(24),
            ..default()
        },
        children![
            (
                Text::new("shapez-rs"),
                TextFont {
                    font_size: FontSize::Px(48.0),
                    ..default()
                },
            ),
            (
                PlayButton,
                Button,
                ActivateOnPress,
                Node {
                    width: px(180),
                    height: px(60),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.2, 0.25)),
                children![Text::new("Jouer")],
            ),
            (
                GalleryButton,
                Button,
                ActivateOnPress,
                Node {
                    width: px(180),
                    height: px(48),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.2, 0.25)),
                children![Text::new("Formes")],
            ),
        ],
    ));
}

fn start_game(
    activate: On<Activate>,
    play: Query<(), With<PlayButton>>,
    gallery: Query<(), With<GalleryButton>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let target = activate.event_target();
    if play.contains(target) {
        next.set(AppState::InGame);
    } else if gallery.contains(target) {
        next.set(AppState::ShapeGallery);
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
        children![
            (TickText, Text::new("")),
            (
                GoalText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
            ),
            (
                NotificationText,
                NotificationTimer(Timer::from_seconds(0.0, TimerMode::Once)),
                Text::new(""),
                TextColor(Color::srgb(1.0, 0.85, 0.3)),
                TextFont {
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
            ),
        ],
    ));
}

fn update_hud(
    sim: Res<Sim>,
    speed: Res<GameSpeed>,
    mut tick: Single<&mut Text, (With<TickText>, Without<GoalText>)>,
    mut goal: Single<&mut Text, (With<GoalText>, Without<TickText>)>,
) {
    tick.0 = format!(
        "Tick {} - vitesse x{} - graine {} (ZQSD/fleches, molette, clic milieu ; Tab = vitesse, espace = pause, Echap = menu)",
        sim.tick_count(),
        speed.0,
        sim.map.seed()
    );
    goal.0 = match sim.hub.goal() {
        Some(g) => format!(
            "Niveau {} - livrer {} au hub : {} / {}{}",
            sim.hub.level,
            sim.registry.get(g.shape),
            sim.hub.stored(g.shape),
            g.required,
            if g.throughput_only {
                " par seconde (M5)"
            } else {
                ""
            }
        ),
        None => format!(
            "Niveau {} - histoire terminee (freeplay en M5)",
            sim.hub.level
        ),
    };
}

/// Nom affiché d'une récompense (sans accents : police par défaut).
fn reward_name(reward: Reward) -> &'static str {
    match reward {
        Reward::CutterAndTrash => "decoupeur et poubelle",
        Reward::Rotater => "rotateur",
        Reward::Painter => "peintre",
        Reward::Mixer => "melangeur",
        Reward::Stacker => "empileur",
        Reward::Balancer => "equilibreur",
        Reward::Tunnel => "tunnel",
        Reward::RotaterCcw => "rotateur anti-horaire",
        Reward::Rotater180 => "rotateur 180",
        Reward::MinerChainable => "extracteur chainable",
        Reward::UndergroundBeltTier2 => "tunnel niveau 2",
        Reward::BeltReader => "lecteur de convoyeur",
        Reward::Splitter => "separateur",
        Reward::CutterQuad => "decoupeur quadruple",
        Reward::PainterDouble => "peintre double",
        Reward::Storage => "stockage",
        Reward::Merger => "fusionneur",
        Reward::WiresPainterAndLevers => "wires, peintre quadruple et leviers",
        Reward::Display => "afficheur",
        Reward::ConstantSignal => "signal constant",
        Reward::LogicGates => "portes logiques",
        Reward::VirtualProcessing => "traitement virtuel",
        Reward::Filter => "filtre",
        Reward::Blueprints => "plans",
        Reward::Freeplay => "mode libre",
        Reward::None => "aucune",
    }
}

fn notify_levels(
    time: Res<Time>,
    mut events: MessageReader<SimEventMsg>,
    mut text: Single<(&mut Text, &mut NotificationTimer), With<NotificationText>>,
) {
    let (text, timer) = &mut *text;
    for SimEventMsg(event) in events.read() {
        if let SimEvent::LevelCompleted(done) = event {
            text.0 = format!(
                "Niveau {} termine ! Recompense : {}",
                done.level,
                reward_name(done.reward)
            );
            timer.0 = Timer::from_seconds(NOTIFICATION_SECONDS, TimerMode::Once);
        }
    }
    if timer.0.tick(time.delta()).just_finished() {
        text.0.clear();
    }
}

fn game_keys(
    keys: Res<ButtonInput<KeyCode>>,
    placement: Res<PlacementState>,
    mut speed: ResMut<GameSpeed>,
    mut next: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::Space) {
        speed.0 = if speed.0 == 0 { 1 } else { 0 };
    } else if keys.just_pressed(KeyCode::Tab) {
        // x1 -> x2 -> x4 -> x1
        speed.0 = match speed.0 {
            1 => 2,
            2 => 4,
            _ => 1,
        };
    }
    // Echap désélectionne d'abord le bâtiment en cours ; sinon retour au menu.
    if keys.just_pressed(KeyCode::Escape) && placement.selected.is_none() {
        next.set(AppState::MainMenu);
    }
}
