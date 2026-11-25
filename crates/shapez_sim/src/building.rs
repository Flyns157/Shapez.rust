//! Définitions des bâtiments : variantes, couche, dimensions, slots d'entrée/sortie et
//! pins de la couche wires (port des `MetaBuilding` de `src/js/game/buildings/`).
//!
//! Les positions et directions sont **locales** (bâtiment non tourné, orienté vers le
//! haut). Comme dans le JS, un slot d'acceptation indique le côté *d'où vient* l'item,
//! et un slot d'éjection le côté *vers lequel* il part.

use shapez_core::{Direction, Item, Rotation, TilePos};

/// Couche d'un bâtiment (`getLayer`) : chacune a sa propre grille d'occupation.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum Layer {
    #[default]
    Regular,
    Wires,
}

/// Type de bâtiment (`MetaBuilding`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum BuildingKind {
    Belt,
    Balancer,
    UndergroundBelt,
    Cutter,
    Rotater,
    Stacker,
    Painter,
    Mixer,
    Miner,
    Trash,
    Storage,
    Hub,
    /// Câble (couche wires).
    Wire,
    /// Croisement de câbles.
    WireTunnel,
    /// Émetteur de signal constant.
    ConstantSignal,
    /// Levier (couche normale, pin vers la couche wires).
    Lever,
    /// Portes ET (variante par défaut), OU, NON et XOR.
    LogicGate,
    Transistor,
    /// Filtre : laisse passer vers le haut les items égaux au signal, rejette les
    /// autres à droite.
    Filter,
    /// Lecteur de convoyeur : laisse passer les items et publie le dernier lu.
    Reader,
    /// Afficheur de la valeur d'un réseau.
    Display,
    /// Analyseur de forme (couche wires).
    Analyzer,
    /// Comparateur (couche wires).
    Comparator,
    /// Processeurs virtuels : découpe (variante par défaut), rotation, empilage,
    /// peinture et désempilage, sur des signaux.
    VirtualProcessor,
}

/// Variante d'un bâtiment (`enum*Variants` du JS). Chaque type n'accepte que les
/// siennes, voir [`BuildingKind::variants`].
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum Variant {
    #[default]
    Default,
    /// Tunnel de niveau 2.
    Tier2,
    Merger,
    MergerInverse,
    Splitter,
    SplitterInverse,
    /// Découpeur ou peintre quadruple.
    Quad,
    /// Rotateur anti-horaire.
    Ccw,
    Rotate180,
    /// Peintre miroir.
    Mirrored,
    /// Peintre double.
    Double,
    /// Extracteur chaînable.
    Chainable,
    /// Câble de la seconde couleur (`enumWireVariant.second`).
    Second,
    Or,
    Not,
    Xor,
    /// Processeur virtuel de rotation (`enumVirtualProcessorVariants.rotater`).
    Rotate,
    /// Processeur virtuel d'empilage.
    Stack,
    /// Processeur virtuel de peinture.
    Paint,
    /// Processeur virtuel de désempilage.
    Unstack,
}

impl BuildingKind {
    pub const ALL: [Self; 24] = [
        Self::Belt,
        Self::Balancer,
        Self::UndergroundBelt,
        Self::Cutter,
        Self::Rotater,
        Self::Stacker,
        Self::Painter,
        Self::Mixer,
        Self::Miner,
        Self::Trash,
        Self::Storage,
        Self::Hub,
        Self::Wire,
        Self::WireTunnel,
        Self::ConstantSignal,
        Self::Lever,
        Self::LogicGate,
        Self::Transistor,
        Self::Filter,
        Self::Reader,
        Self::Display,
        Self::Analyzer,
        Self::Comparator,
        Self::VirtualProcessor,
    ];

    /// Identifiant du JS (`MetaBuilding.getId()`), utile pour les sauvegardes.
    pub const fn js_id(self) -> &'static str {
        match self {
            Self::Belt => "belt",
            Self::Balancer => "balancer",
            Self::UndergroundBelt => "underground_belt",
            Self::Cutter => "cutter",
            Self::Rotater => "rotater",
            Self::Stacker => "stacker",
            Self::Painter => "painter",
            Self::Mixer => "mixer",
            Self::Miner => "miner",
            Self::Trash => "trash",
            Self::Storage => "storage",
            Self::Hub => "hub",
            Self::Wire => "wire",
            Self::WireTunnel => "wire_tunnel",
            Self::ConstantSignal => "constant_signal",
            Self::Lever => "lever",
            Self::LogicGate => "logic_gate",
            Self::Transistor => "transistor",
            Self::Filter => "filter",
            Self::Reader => "reader",
            Self::Display => "display",
            Self::Analyzer => "analyzer",
            Self::Comparator => "comparator",
            Self::VirtualProcessor => "virtual_processor",
        }
    }

    /// Variantes disponibles, dans l'ordre de la barre d'outils (touche T).
    pub const fn variants(self) -> &'static [Variant] {
        use Variant::*;
        match self {
            Self::Balancer => &[Default, Merger, MergerInverse, Splitter, SplitterInverse],
            Self::UndergroundBelt => &[Default, Tier2],
            Self::Cutter => &[Default, Quad],
            Self::Rotater => &[Default, Ccw, Rotate180],
            Self::Painter => &[Default, Mirrored, Double, Quad],
            Self::Miner => &[Default, Chainable],
            Self::Wire => &[Default, Second],
            Self::LogicGate => &[Default, Or, Not, Xor],
            Self::Transistor => &[Default, Mirrored],
            Self::VirtualProcessor => &[Default, Rotate, Stack, Paint, Unstack],
            _ => &[Default],
        }
    }

    pub fn has_variant(self, variant: Variant) -> bool {
        self.variants().contains(&variant)
    }

    /// `getLayer`.
    pub const fn layer(self) -> Layer {
        match self {
            Self::Wire
            | Self::WireTunnel
            | Self::ConstantSignal
            | Self::LogicGate
            | Self::Transistor
            | Self::Analyzer
            | Self::Comparator
            | Self::VirtualProcessor => Layer::Wires,
            _ => Layer::Regular,
        }
    }

    /// `getIsReplaceable` : seuls les convoyeurs et les câbles peuvent être écrasés par
    /// un placement.
    pub const fn is_replaceable(self) -> bool {
        matches!(self, Self::Belt | Self::Wire)
    }

    /// Le bâtiment porte un réglage ([`Building::config`]) : signal d'un émetteur
    /// constant, état d'un levier.
    pub const fn is_configurable(self) -> bool {
        matches!(self, Self::ConstantSignal | Self::Lever)
    }

    /// `getIsRemovable` : le hub ne peut pas être supprimé.
    pub const fn is_removable(self) -> bool {
        !matches!(self, Self::Hub)
    }

    /// `getIsRotateable`.
    pub const fn is_rotatable(self) -> bool {
        !matches!(self, Self::Hub | Self::WireTunnel)
    }
}

/// Filtre d'un slot d'entrée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SlotFilter {
    Any,
    Shape,
    Color,
}

/// Slot d'entrée : l'item arrive dans la tuile `pos` depuis le côté `from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AcceptorSlot {
    pub pos: TilePos,
    pub from: Direction,
    pub filter: SlotFilter,
}

/// Slot de sortie : l'item quitte la tuile `pos` vers le côté `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EjectorSlot {
    pub pos: TilePos,
    pub to: Direction,
}

/// Slots d'un bâtiment, en coordonnées locales.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slots {
    pub acceptors: Vec<AcceptorSlot>,
    pub ejectors: Vec<EjectorSlot>,
}

/// Variante de rotation d'un convoyeur (`arrayBeltVariantToRotation`).
pub mod belt_variant {
    pub const STRAIGHT: u8 = 0;
    pub const LEFT: u8 = 1;
    pub const RIGHT: u8 = 2;
}

/// Variante de rotation d'un tunnel (`arrayUndergroundRotationVariantToMode`).
pub mod tunnel_variant {
    pub const SENDER: u8 = 0;
    pub const RECEIVER: u8 = 1;
}

/// Forme d'un câble, choisie par l'orientation automatique
/// (`arrayWireRotationVariantToType`).
pub mod wire_type {
    pub const FORWARD: u8 = 0;
    pub const TURN: u8 = 1;
    pub const SPLIT: u8 = 2;
    pub const CROSS: u8 = 3;
}

/// Rôle d'un pin de la couche wires (`enumPinSlotType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinKind {
    /// Le bâtiment envoie une valeur sur le réseau.
    Ejector,
    /// Le bâtiment lit la valeur du réseau.
    Acceptor,
}

/// Pin : se connecte au câble de la tuile voisine de `pos` dans la direction
/// `direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinSlot {
    pub pos: TilePos,
    pub direction: Direction,
    pub kind: PinKind,
}

/// Pins locaux, dans l'ordre des `slots` de `WiredPinsComponent` (les portes
/// écrivent leur résultat dans le premier).
pub fn pins(kind: BuildingKind, variant: Variant) -> Vec<PinSlot> {
    use Direction::{Bottom, Left, Right, Top};
    use PinKind::{Acceptor, Ejector};
    let pin = |direction, kind| PinSlot {
        pos: TilePos::ZERO,
        direction,
        kind,
    };
    match kind {
        BuildingKind::ConstantSignal | BuildingKind::Lever => vec![pin(Top, Ejector)],
        BuildingKind::Display => vec![pin(Bottom, Acceptor)],
        BuildingKind::Filter => vec![pin(Left, Acceptor)],
        // Sortie 0 : un item est passé dans la dernière seconde ; sortie 1 : dernier item.
        BuildingKind::Reader => vec![pin(Right, Ejector), pin(Left, Ejector)],
        // Sortie 0 : l'item stocké ; sortie 1 : le stockage est plein.
        BuildingKind::Storage => vec![
            PinSlot {
                pos: TilePos::new(1, 1),
                direction: Right,
                kind: Ejector,
            },
            PinSlot {
                pos: TilePos::new(0, 1),
                direction: Left,
                kind: Ejector,
            },
        ],
        BuildingKind::Painter if variant == Variant::Quad => (0..4)
            .map(|x| PinSlot {
                pos: TilePos::new(x, 0),
                direction: Bottom,
                kind: Acceptor,
            })
            .collect(),
        BuildingKind::Comparator => {
            vec![pin(Top, Ejector), pin(Left, Acceptor), pin(Right, Acceptor)]
        }
        BuildingKind::Analyzer => {
            vec![
                pin(Left, Ejector),
                pin(Right, Ejector),
                pin(Bottom, Acceptor),
            ]
        }
        BuildingKind::VirtualProcessor => match variant {
            Variant::Rotate => vec![pin(Top, Ejector), pin(Bottom, Acceptor)],
            Variant::Stack | Variant::Paint => {
                vec![
                    pin(Top, Ejector),
                    pin(Bottom, Acceptor),
                    pin(Right, Acceptor),
                ]
            }
            _ => vec![
                pin(Left, Ejector),
                pin(Right, Ejector),
                pin(Bottom, Acceptor),
            ],
        },
        BuildingKind::LogicGate if variant == Variant::Not => {
            vec![pin(Top, Ejector), pin(Bottom, Acceptor)]
        }
        BuildingKind::LogicGate => {
            vec![pin(Top, Ejector), pin(Left, Acceptor), pin(Right, Acceptor)]
        }
        BuildingKind::Transistor => {
            let side = if variant == Variant::Mirrored {
                Right
            } else {
                Left
            };
            vec![
                pin(Top, Ejector),
                pin(side, Acceptor),
                pin(Bottom, Acceptor),
            ]
        }
        _ => Vec::new(),
    }
}

/// Portée des tunnels par niveau (`undergroundBeltMaxTilesByTier`).
pub const fn tunnel_range(variant: Variant) -> i32 {
    match variant {
        Variant::Tier2 => 9,
        _ => 5,
    }
}

/// Taille du bâtiment non tourné (`getDimensions`).
pub const fn dimensions(kind: BuildingKind, variant: Variant) -> (i32, i32) {
    use BuildingKind::*;
    match (kind, variant) {
        (Balancer, Variant::Default) => (2, 1),
        (Cutter, Variant::Default) => (2, 1),
        (Cutter, Variant::Quad) => (4, 1),
        (Painter, Variant::Double) => (2, 2),
        (Painter, Variant::Quad) => (4, 1),
        (Painter, _) => (2, 1),
        (Stacker | Mixer | Filter, _) => (2, 1),
        (Storage, _) => (2, 2),
        (Hub, _) => (4, 4),
        _ => (1, 1),
    }
}

const fn at(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y)
}

fn acc(x: i32, y: i32, from: Direction, filter: SlotFilter) -> AcceptorSlot {
    AcceptorSlot {
        pos: at(x, y),
        from,
        filter,
    }
}

fn ej(x: i32, y: i32, to: Direction) -> EjectorSlot {
    EjectorSlot { pos: at(x, y), to }
}

/// Slots locaux (`setupEntityComponents` + `updateVariants`). Pour un convoyeur, ce
/// sont les « faux » slots du JS (`getFakeAcceptorSlot` / `getFakeEjectorSlot`).
pub fn slots(kind: BuildingKind, variant: Variant, rotation_variant: u8) -> Slots {
    use BuildingKind::*;
    use Direction::{Bottom, Left, Right, Top};
    use SlotFilter::{Any, Color, Shape};

    let (acceptors, ejectors) = match kind {
        Belt => {
            let to = match rotation_variant {
                belt_variant::LEFT => Left,
                belt_variant::RIGHT => Right,
                _ => Top,
            };
            (vec![acc(0, 0, Bottom, Any)], vec![ej(0, 0, to)])
        }
        Balancer => match variant {
            Variant::Merger | Variant::MergerInverse => {
                let side = if variant == Variant::MergerInverse {
                    Left
                } else {
                    Right
                };
                (
                    vec![acc(0, 0, Bottom, Any), acc(0, 0, side, Any)],
                    vec![ej(0, 0, Top)],
                )
            }
            Variant::Splitter | Variant::SplitterInverse => {
                let side = if variant == Variant::SplitterInverse {
                    Left
                } else {
                    Right
                };
                (
                    vec![acc(0, 0, Bottom, Any)],
                    vec![ej(0, 0, Top), ej(0, 0, side)],
                )
            }
            _ => (
                vec![acc(0, 0, Bottom, Any), acc(1, 0, Bottom, Any)],
                vec![ej(0, 0, Top), ej(1, 0, Top)],
            ),
        },
        UndergroundBelt => match rotation_variant {
            tunnel_variant::RECEIVER => (vec![], vec![ej(0, 0, Top)]),
            _ => (vec![acc(0, 0, Bottom, Any)], vec![]),
        },
        Cutter => {
            let n = if variant == Variant::Quad { 4 } else { 2 };
            (
                vec![acc(0, 0, Bottom, Shape)],
                (0..n).map(|x| ej(x, 0, Top)).collect(),
            )
        }
        Rotater => (vec![acc(0, 0, Bottom, Shape)], vec![ej(0, 0, Top)]),
        Stacker => (
            vec![acc(0, 0, Bottom, Shape), acc(1, 0, Bottom, Shape)],
            vec![ej(0, 0, Top)],
        ),
        Painter => match variant {
            Variant::Double => (
                vec![
                    acc(0, 0, Left, Shape),
                    acc(0, 1, Left, Shape),
                    acc(1, 0, Top, Color),
                ],
                vec![ej(1, 0, Right)],
            ),
            Variant::Quad => {
                let mut a = vec![acc(0, 0, Left, Shape)];
                a.extend((0..4).map(|x| acc(x, 0, Bottom, Color)));
                (a, vec![ej(0, 0, Top)])
            }
            _ => {
                let color_from = if variant == Variant::Mirrored {
                    Bottom
                } else {
                    Top
                };
                (
                    vec![acc(0, 0, Left, Shape), acc(1, 0, color_from, Color)],
                    vec![ej(1, 0, Right)],
                )
            }
        },
        Mixer => (
            vec![acc(0, 0, Bottom, Color), acc(1, 0, Bottom, Color)],
            vec![ej(0, 0, Top)],
        ),
        Miner => (vec![], vec![ej(0, 0, Top)]),
        Trash => (
            [Top, Right, Bottom, Left]
                .into_iter()
                .map(|d| acc(0, 0, d, Any))
                .collect(),
            vec![],
        ),
        Storage => (
            vec![acc(0, 1, Bottom, Any), acc(1, 1, Bottom, Any)],
            vec![ej(0, 0, Top), ej(1, 0, Top)],
        ),
        Hub => {
            let mut a = Vec::new();
            for i in 0..4 {
                a.push(acc(i, 0, Top, Shape));
                a.push(acc(i, 3, Bottom, Shape));
                a.push(acc(0, i, Left, Shape));
                a.push(acc(3, i, Right, Shape));
            }
            (a, vec![])
        }
        Filter => (
            vec![acc(0, 0, Bottom, Any)],
            vec![ej(0, 0, Top), ej(1, 0, Right)],
        ),
        Reader => (vec![acc(0, 0, Bottom, Any)], vec![ej(0, 0, Top)]),
        Wire | WireTunnel | ConstantSignal | Lever | LogicGate | Transistor | Display
        | Analyzer | Comparator | VirtualProcessor => (vec![], vec![]),
    };
    Slots {
        acceptors,
        ejectors,
    }
}

/// Bâtiment posé sur la carte (`StaticMapEntityComponent` + type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Building {
    pub kind: BuildingKind,
    pub variant: Variant,
    pub rotation_variant: u8,
    pub origin: TilePos,
    pub rotation: Rotation,
    /// Rotation choisie par le joueur, avant l'orientation automatique des convoyeurs
    /// (`originalRotation`).
    pub original_rotation: Rotation,
    /// Réglage copié avec le bâtiment (`copyAdditionalStateTo`) : signal d'un émetteur
    /// constant, `Bool(true)` pour un levier enclenché.
    #[serde(default)]
    pub config: Option<Item>,
}

impl Building {
    /// Coin haut-gauche et taille en tuiles monde (`getTileSpaceBounds`).
    pub fn bounds(&self) -> (TilePos, (i32, i32)) {
        let (w, h) = dimensions(self.kind, self.variant);
        let o = self.origin;
        match self.rotation {
            Rotation::R0 => (o, (w, h)),
            Rotation::R90 => (TilePos::new(o.x - h + 1, o.y), (h, w)),
            Rotation::R180 => (TilePos::new(o.x - w + 1, o.y - h + 1), (w, h)),
            Rotation::R270 => (TilePos::new(o.x, o.y - w + 1), (h, w)),
        }
    }

    /// Toutes les tuiles occupées, ligne par ligne.
    pub fn tiles(&self) -> impl Iterator<Item = TilePos> {
        let (min, (w, h)) = self.bounds();
        (0..h).flat_map(move |dy| (0..w).map(move |dx| TilePos::new(min.x + dx, min.y + dy)))
    }

    /// `localTileToWorld`.
    pub fn local_to_world(&self, local: TilePos) -> TilePos {
        self.origin + local.rotated(self.rotation)
    }

    pub fn local_direction_to_world(&self, dir: Direction) -> Direction {
        dir.rotated(self.rotation)
    }

    /// Pins en coordonnées monde.
    pub fn world_pins(&self) -> Vec<PinSlot> {
        pins(self.kind, self.variant)
            .into_iter()
            .map(|p| PinSlot {
                pos: self.local_to_world(p.pos),
                direction: self.local_direction_to_world(p.direction),
                kind: p.kind,
            })
            .collect()
    }

    /// Slots en coordonnées monde.
    pub fn world_slots(&self) -> Slots {
        let local = slots(self.kind, self.variant, self.rotation_variant);
        Slots {
            acceptors: local
                .acceptors
                .into_iter()
                .map(|s| AcceptorSlot {
                    pos: self.local_to_world(s.pos),
                    from: self.local_direction_to_world(s.from),
                    filter: s.filter,
                })
                .collect(),
            ejectors: local
                .ejectors
                .into_iter()
                .map(|s| EjectorSlot {
                    pos: self.local_to_world(s.pos),
                    to: self.local_direction_to_world(s.to),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(kind: BuildingKind, variant: Variant, x: i32, y: i32, rot: Rotation) -> Building {
        Building {
            kind,
            variant,
            rotation_variant: 0,
            origin: TilePos::new(x, y),
            rotation: rot,
            original_rotation: rot,
            config: None,
        }
    }

    #[test]
    fn bounds_match_js_for_each_rotation() {
        // Découpeur 2×1 posé en (5, 5).
        let cases = [
            (Rotation::R0, (5, 5), (2, 1)),
            (Rotation::R90, (5, 5), (1, 2)),
            (Rotation::R180, (4, 5), (2, 1)),
            (Rotation::R270, (5, 4), (1, 2)),
        ];
        for (rot, (x, y), size) in cases {
            let (min, s) = b(BuildingKind::Cutter, Variant::Default, 5, 5, rot).bounds();
            assert_eq!((min, s), (TilePos::new(x, y), size), "{rot:?}");
        }
    }

    #[test]
    fn tiles_are_the_rotated_local_tiles() {
        for rot in Rotation::ALL {
            let building = b(BuildingKind::Painter, Variant::Double, 3, -2, rot);
            let mut tiles: Vec<_> = building.tiles().collect();
            let mut expected: Vec<_> = (0..2)
                .flat_map(|x| (0..2).map(move |y| TilePos::new(x, y)))
                .map(|p| building.local_to_world(p))
                .collect();
            tiles.sort();
            expected.sort();
            assert_eq!(tiles, expected, "{rot:?}");
        }
    }

    #[test]
    fn variants_have_dimensions_and_slots() {
        for kind in BuildingKind::ALL {
            for &variant in kind.variants() {
                let (w, h) = dimensions(kind, variant);
                let s = slots(kind, variant, 0);
                for p in s
                    .acceptors
                    .iter()
                    .map(|a| a.pos)
                    .chain(s.ejectors.iter().map(|e| e.pos))
                    .chain(pins(kind, variant).iter().map(|p| p.pos))
                {
                    assert!(
                        p.x >= 0 && p.x < w && p.y >= 0 && p.y < h,
                        "{kind:?} {variant:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn rotated_belt_ejects_sideways() {
        let mut belt = b(BuildingKind::Belt, Variant::Default, 0, 0, Rotation::R90);
        belt.rotation_variant = belt_variant::LEFT;
        let s = belt.world_slots();
        assert_eq!(s.acceptors[0].from, Direction::Left);
        assert_eq!(s.ejectors[0].to, Direction::Top);
    }
}
