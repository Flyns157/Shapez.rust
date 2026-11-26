//! Panneaux du HUD : améliorations (F, port de `hud/parts/shop.js`) et
//! statistiques (G, `hud/parts/statistics.js`).
//!
//! Chaque panneau calcule un petit modèle à chaque image et ne reconstruit ses nœuds
//! que lorsque ce modèle change.

use bevy::{
    prelude::*,
    ui_widgets::{Activate, ActivateOnPress, Button},
};
use shapez_core::ShapeId;
use shapez_sim::{
    analytics::{DataSource, SLICE_SECONDS},
    placement::SimCommand,
    units::IMPROVEMENT_ONE,
    upgrades::UpgradeKind,
};

use crate::{
    AppState,
    camera::CameraSet,
    i18n::{decimal, t, tf, upgrade_name},
    shape_textures::ShapeTextures,
    sim::Sim,
};

/// Panneaux ouverts. Un seul à la fois, comme dans le JS.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum OpenPanel {
    #[default]
    None,
    Shop,
    Statistics,
}

impl OpenPanel {
    pub fn is_open(self) -> bool {
        self != Self::None
    }
}

/// Source affichée par le panneau des statistiques.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
struct StatsSource(DataSource);

impl Default for StatsSource {
    fn default() -> Self {
        Self(DataSource::Produced)
    }
}

#[derive(Component, Debug)]
struct ShopPanel;

#[derive(Component, Debug)]
struct StatsPanel;

#[derive(Component, Debug, Clone, Copy)]
struct BuyButton(UpgradeKind);

#[derive(Component, Debug, Clone, Copy)]
struct SourceButton(DataSource);

/// Icône d'une forme requise : un clic l'épingle ou la désépingle.
#[derive(Component, Debug, Clone)]
struct PinButton(&'static str);

/// Modèle affiché, pour ne reconstruire qu'en cas de changement.
#[derive(Component, Debug, Default, PartialEq)]
struct Shown(Vec<String>);

/// Touches F, G et Echap des panneaux.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PanelKeys;

#[derive(Debug, Default)]
pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenPanel>()
            .init_resource::<StatsSource>()
            .add_systems(OnEnter(AppState::InGame), spawn_panels)
            .add_systems(OnExit(AppState::InGame), |mut open: ResMut<OpenPanel>| {
                *open = OpenPanel::None;
            })
            .add_observer(on_activate)
            .add_systems(
                Update,
                (
                    panel_keys.in_set(PanelKeys).before(CameraSet),
                    update_shop,
                    update_stats,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Chiffres romains des paliers (`getRomanNumber`).
pub fn roman(mut n: usize) -> String {
    const TABLE: [(usize, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut s = String::new();
    for (value, digits) in TABLE {
        while n >= value {
            s.push_str(digits);
            n -= value;
        }
    }
    s
}

fn multiplier(improvement: u64) -> String {
    format!(
        "x{}",
        decimal(improvement as f64 / IMPROVEMENT_ONE as f64, 2)
    )
}

fn panel_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: px(170),
        left: percent(15),
        right: percent(15),
        flex_direction: FlexDirection::Column,
        row_gap: px(12),
        padding: UiRect::all(px(16)),
        display: Display::None,
        ..default()
    }
}

fn spawn_panels(mut commands: Commands) {
    let background = BackgroundColor(Color::srgba(0.08, 0.08, 0.1, 0.94));
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        ShopPanel,
        Shown::default(),
        panel_node(),
        background,
        GlobalZIndex(10),
    ));
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        StatsPanel,
        Shown::default(),
        panel_node(),
        background,
        GlobalZIndex(10),
    ));
}

fn panel_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<OpenPanel>,
    mut shop: Single<&mut Node, (With<ShopPanel>, Without<StatsPanel>)>,
    mut stats: Single<&mut Node, (With<StatsPanel>, Without<ShopPanel>)>,
) {
    let toggle = |open: &mut OpenPanel, panel| {
        *open = if *open == panel {
            OpenPanel::None
        } else {
            panel
        };
    };
    if keys.just_pressed(KeyCode::KeyF) {
        toggle(&mut open, OpenPanel::Shop);
    } else if keys.just_pressed(KeyCode::KeyG) {
        toggle(&mut open, OpenPanel::Statistics);
    } else if keys.just_pressed(KeyCode::Escape) && open.is_open() {
        *open = OpenPanel::None;
    }
    let display = |visible: bool| {
        if visible {
            Display::Flex
        } else {
            Display::None
        }
    };
    let shop_display = display(*open == OpenPanel::Shop);
    if shop.display != shop_display {
        shop.display = shop_display;
    }
    let stats_display = display(*open == OpenPanel::Statistics);
    if stats.display != stats_display {
        stats.display = stats_display;
    }
}

fn on_activate(
    activate: On<Activate>,
    buy: Query<&BuyButton>,
    source_buttons: Query<&SourceButton>,
    pins: Query<&PinButton>,
    // Absent hors partie (boutons du menu principal).
    sim: Option<ResMut<Sim>>,
    mut source: ResMut<StatsSource>,
) {
    let target = activate.event_target();
    let mut sim = sim;
    if let Ok(&BuyButton(kind)) = buy.get(target)
        && let Some(sim) = sim.as_deref_mut()
    {
        // Refus silencieux si les formes manquent : le bouton est alors grisé.
        let _ = sim.apply(SimCommand::UnlockUpgrade(kind));
    } else if let Ok(&SourceButton(s)) = source_buttons.get(target) {
        source.0 = s;
    } else if let Ok(PinButton(key)) = pins.get(target)
        && let Some(sim) = sim.as_deref_mut()
    {
        let def = shapez_core::ShapeDef::from_short_key(key).expect("forme valide");
        let id = sim.registry.intern(def);
        sim.marks.toggle_pin(id);
    }
}

fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn icon(handle: Handle<Image>, size: f32) -> impl Bundle {
    (
        ImageNode::new(handle),
        Node {
            width: px(size),
            height: px(size),
            ..default()
        },
    )
}

fn button(marker: impl Bundle, text: impl Into<String>, enabled: bool) -> impl Bundle {
    let color = if enabled {
        Color::srgb(0.2, 0.55, 0.3)
    } else {
        Color::srgb(0.25, 0.25, 0.28)
    };
    (
        marker,
        Button,
        ActivateOnPress,
        Node {
            padding: UiRect::axes(px(12), px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(color),
        children![label(text, 16.0, Color::WHITE)],
    )
}

const ENOUGH: Color = Color::srgb(0.5, 0.9, 0.5);
const MISSING: Color = Color::srgb(0.95, 0.5, 0.45);

fn update_shop(
    mut commands: Commands,
    open: Res<OpenPanel>,
    sim: Res<Sim>,
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    panel: Single<(Entity, &mut Shown), With<ShopPanel>>,
) {
    if *open != OpenPanel::Shop {
        return;
    }
    // Modèle : niveau et stock de chaque exigence.
    let hub = &sim.hub;
    let mut model = Vec::new();
    for kind in UpgradeKind::ALL {
        let mut line = format!("{kind:?}:{}", hub.upgrade_level(kind));
        if let Some(tier) = hub.next_tier(kind) {
            for &(key, _) in &tier.required {
                let def = shapez_core::ShapeDef::from_short_key(key).expect("forme valide");
                let id = sim.registry.find(&def);
                let have = id.map_or(0, |id| hub.stored(id));
                let pinned = id.is_some_and(|id| sim.marks.is_pinned(id));
                line.push_str(&format!(":{have}:{pinned}"));
            }
        }
        model.push(line);
    }
    let (entity, mut shown) = panel.into_inner();
    if shown.0 == model {
        return;
    }
    shown.0 = model;

    let upgrades = hub.upgrades();
    let current = |kind| match kind {
        UpgradeKind::Belt => upgrades.belt,
        UpgradeKind::Miner => upgrades.miner,
        UpgradeKind::Processors => upgrades.processors,
        UpgradeKind::Painting => upgrades.painting,
    };
    let mut rows = Vec::new();
    for kind in UpgradeKind::ALL {
        let level = hub.upgrade_level(kind);
        let tier = hub.next_tier(kind);
        let title = match tier {
            Some(tier) => tf(
                "port.shop.tier",
                &[
                    ("name", &upgrade_name(kind)),
                    ("tier", &tf("ingame.shop.tier", &[("x", &roman(level + 1))])),
                    ("from", &multiplier(current(kind))),
                    ("to", &multiplier(current(kind) + tier.improvement)),
                ],
            ),
            None => tf(
                "port.shop.maxTier",
                &[
                    ("name", &upgrade_name(kind)),
                    ("mult", &multiplier(current(kind))),
                ],
            ),
        };
        let requirements: Vec<(&'static str, Handle<Image>, String, bool)> = tier
            .map(|tier| {
                tier.required
                    .iter()
                    .map(|&(key, amount)| {
                        let def = shapez_core::ShapeDef::from_short_key(key).expect("forme valide");
                        let have = sim.registry.find(&def).map_or(0, |id| hub.stored(id));
                        let pinned = sim
                            .registry
                            .find(&def)
                            .is_some_and(|id| sim.marks.is_pinned(id));
                        (
                            key,
                            textures.get_or_render(def, &mut images),
                            if pinned {
                                format!("{have} / {amount} {}", t("port.shop.pinned"))
                            } else {
                                format!("{have} / {amount}")
                            },
                            have >= amount,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        rows.push((
            kind,
            title,
            requirements,
            hub.can_unlock_upgrade(kind, &sim.registry),
        ));
    }

    commands
        .entity(entity)
        .despawn_children()
        .with_children(|p| {
            p.spawn(label(
                format!("{} {}", t("ingame.shop.title"), t("port.shop.help")),
                22.0,
                Color::WHITE,
            ));
            for (kind, title, requirements, affordable) in rows {
                p.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(label(title, 18.0, Color::WHITE));
                    row.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: px(8),
                        ..default()
                    })
                    .with_children(|line| {
                        for (key, handle, text, enough) in requirements {
                            line.spawn((
                                PinButton(key),
                                Button,
                                ActivateOnPress,
                                icon(handle, 32.0),
                            ));
                            line.spawn(label(text, 16.0, if enough { ENOUGH } else { MISSING }));
                        }
                        line.spawn(button(
                            BuyButton(kind),
                            t("ingame.shop.buttonUnlock"),
                            affordable,
                        ));
                    });
                });
            }
        });
}

fn update_stats(
    mut commands: Commands,
    open: Res<OpenPanel>,
    source: Res<StatsSource>,
    sim: Res<Sim>,
    mut textures: ResMut<ShapeTextures>,
    mut images: ResMut<Assets<Image>>,
    panel: Single<(Entity, &mut Shown), With<StatsPanel>>,
) {
    if *open != OpenPanel::Statistics {
        return;
    }
    let hub = &sim.hub;
    let mut entries: Vec<(ShapeId, u64)> = match source.0 {
        DataSource::Stored => hub.stored_shapes().iter().map(|(&k, &v)| (k, v)).collect(),
        s => hub.analytics.current_counts(s).into_iter().collect(),
    };
    entries.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| {
            sim.registry
                .get(a.0)
                .short_key()
                .cmp(&sim.registry.get(b.0).short_key())
        })
    });
    entries.truncate(24);
    let mut model = vec![format!("{:?}", source.0)];
    model.extend(entries.iter().map(|(id, n)| format!("{id:?}:{n}")));
    let (entity, mut shown) = panel.into_inner();
    if shown.0 == model {
        return;
    }
    shown.0 = model;

    let cells: Vec<(Handle<Image>, String)> = entries
        .iter()
        .map(|&(id, n)| {
            let text = match source.0 {
                DataSource::Stored => n.to_string(),
                // Formes sur la dernière tranche de 10 s, ramenées à la minute.
                _ => tf(
                    "ingame.statistics.shapesDisplayUnits.minute",
                    &[("shapes", &(n * 60 / SLICE_SECONDS).to_string())],
                ),
            };
            (
                textures.get_or_render(*sim.registry.get(id), &mut images),
                text,
            )
        })
        .collect();
    let empty = cells.is_empty();
    let current = source.0;

    commands
        .entity(entity)
        .despawn_children()
        .with_children(|p| {
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            })
            .with_children(|head| {
                head.spawn(label(
                    format!(
                        "{} {}",
                        t("ingame.statistics.title"),
                        t("port.statistics.help")
                    ),
                    22.0,
                    Color::WHITE,
                ));
                for (s, name) in [
                    (DataSource::Produced, "produced"),
                    (DataSource::Delivered, "delivered"),
                    (DataSource::Stored, "stored"),
                ] {
                    let name = t(&format!("ingame.statistics.dataSources.{name}.title"));
                    head.spawn(button(SourceButton(s), name, s == current));
                }
            });
            if empty {
                p.spawn(label(
                    t("port.statistics.empty"),
                    16.0,
                    Color::srgb(0.7, 0.7, 0.7),
                ));
            }
            p.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(16),
                row_gap: px(8),
                ..default()
            })
            .with_children(|grid| {
                for (handle, text) in cells {
                    grid.spawn(Node {
                        width: px(170),
                        align_items: AlignItems::Center,
                        column_gap: px(8),
                        ..default()
                    })
                    .with_children(|cell| {
                        cell.spawn(icon(handle, 36.0));
                        cell.spawn(label(text, 16.0, Color::WHITE));
                    });
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_numerals() {
        assert_eq!(roman(1), "I");
        assert_eq!(roman(4), "IV");
        assert_eq!(roman(14), "XIV");
        assert_eq!(roman(999), "CMXCIX");
    }
}
