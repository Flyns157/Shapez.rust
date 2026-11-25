//! HUD de jeu : objectif du hub, notifications de niveau et d'amélioration, touches
//! générales (vitesse, pause, retour au menu).

use bevy::prelude::*;

use shapez_sim::{building::BuildingKind, hub_goals::Reward, placement::SimEvent};

use crate::{
    AppState,
    placement_ui::{PlacementState, TOOLBAR},
    shape_textures::ShapeTextures,
    shop::{OpenPanel, upgrade_name},
    sim::{GameSpeed, Sim, SimEventMsg, SimEventsSet},
};

/// Durée d'affichage d'un message de fin de niveau, en secondes.
const NOTIFICATION_SECONDS: f32 = 6.0;

#[derive(Debug, Default)]
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (
                    update_hud,
                    notify_levels.after(SimEventsSet),
                    game_keys
                        .before(crate::camera::CameraSet)
                        .before(crate::shop::PanelKeys),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Component, Debug)]
struct TickText;

#[derive(Component, Debug)]
struct GoalText;

#[derive(Component, Debug)]
struct GoalIcon;

#[derive(Component, Debug)]
struct NotificationText;

#[derive(Component, Debug)]
struct NotificationTimer(Timer);

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
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    ..default()
                },
                children![
                    (
                        GoalIcon,
                        ImageNode::default(),
                        Node {
                            width: px(40),
                            height: px(40),
                            ..default()
                        },
                    ),
                    (
                        GoalText,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(24.0),
                            ..default()
                        },
                    ),
                ],
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
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    mut tick: Single<&mut Text, (With<TickText>, Without<GoalText>)>,
    mut goal: Single<&mut Text, (With<GoalText>, Without<TickText>)>,
    mut icon: Single<&mut ImageNode, With<GoalIcon>>,
) {
    tick.0 = format!(
        "Tick {} - vitesse x{} - graine {} (ZQSD/fleches, molette, clic milieu ; Tab = vitesse, espace = pause, Echap = menu)",
        sim.tick_count(),
        speed.0,
        sim.map.seed()
    );
    let g = sim.hub.goal();
    let text = format!(
        "{} {} - livrer {} : {} / {}{}",
        if sim.hub.is_free_play() {
            "Mode libre, niveau"
        } else {
            "Niveau"
        },
        sim.hub.level,
        sim.registry.get(g.shape),
        sim.hub.goal_delivered(),
        g.required,
        if g.throughput_only {
            " par seconde"
        } else {
            ""
        }
    );
    if goal.0 != text {
        goal.0 = text;
    }
    let handle = textures.get_or_render(*sim.registry.get(g.shape), &mut images);
    if icon.image != handle {
        icon.image = handle;
    }
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
        Reward::None | Reward::NoRewardFreeplay => "aucune",
    }
}

/// Bâtiments débloqués par une récompense, avec leur touche, pour la notification.
fn unlocked_buildings(reward: Reward) -> Vec<String> {
    let kinds: &[BuildingKind] = match reward {
        Reward::CutterAndTrash => &[BuildingKind::Cutter, BuildingKind::Trash],
        Reward::Rotater => &[BuildingKind::Rotater],
        Reward::Painter => &[BuildingKind::Painter],
        Reward::Mixer => &[BuildingKind::Mixer],
        Reward::Stacker => &[BuildingKind::Stacker],
        Reward::Balancer => &[BuildingKind::Balancer],
        Reward::Tunnel => &[BuildingKind::UndergroundBelt],
        _ => &[],
    };
    kinds
        .iter()
        .filter_map(|kind| {
            let i = TOOLBAR.iter().position(|(_, k, _)| k == kind)?;
            Some(format!("{} (touche {})", TOOLBAR[i].2, (i + 1) % 10))
        })
        .collect()
}

fn notify_levels(
    time: Res<Time>,
    mut events: MessageReader<SimEventMsg>,
    mut text: Single<(&mut Text, &mut NotificationTimer), With<NotificationText>>,
) {
    let (text, timer) = &mut *text;
    for SimEventMsg(event) in events.read() {
        let message = match *event {
            SimEvent::LevelCompleted(done) if done.reward == Reward::NoRewardFreeplay => {
                format!("Niveau {} termine !", done.level)
            }
            SimEvent::LevelCompleted(done) => {
                let mut m = format!(
                    "Niveau {} termine ! Debloque : {}",
                    done.level,
                    reward_name(done.reward)
                );
                let buildings = unlocked_buildings(done.reward);
                if !buildings.is_empty() {
                    m.push_str(&format!("\nNouveau : {}", buildings.join(", ")));
                }
                m
            }
            SimEvent::UpgradePurchased { kind, level } => {
                format!(
                    "Amelioration achetee : {} niveau {}",
                    upgrade_name(kind),
                    level + 1
                )
            }
            _ => continue,
        };
        text.0 = message;
        timer.0 = Timer::from_seconds(NOTIFICATION_SECONDS, TimerMode::Once);
    }
    if timer.0.tick(time.delta()).just_finished() {
        text.0.clear();
    }
}

fn game_keys(
    keys: Res<ButtonInput<KeyCode>>,
    placement: Res<PlacementState>,
    panel: Res<OpenPanel>,
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
    // Echap ferme d'abord le panneau ouvert, puis désélectionne le bâtiment en cours ;
    // sinon retour au menu.
    if keys.just_pressed(KeyCode::Escape) && placement.selected.is_none() && !panel.is_open() {
        next.set(AppState::MainMenu);
    }
}
