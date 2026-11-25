// Vecteurs golden de la couche wires : réseaux, émetteurs constants, leviers, portes et
// processeurs virtuels.
//
//   node tools/js-oracle/wires.mjs [N=60] > crates/shapez_sim/tests/golden/wires.json
//
// Chaque cas pose au hasard, sur une petite grille, des câbles (deux couleurs), des
// tunnels de câbles, des portes ET/OU/NON/XOR, des transistors, des analyseurs, des
// comparateurs, des processeurs virtuels, des émetteurs constants (booléens, couleurs
// ou formes, couche wires) et des leviers (couche normale). Les systèmes du jeu
// (`WireSystem`, `LogicGateSystem`, `LeverSystem`, `ConstantSignalSystem`) tournent
// ensuite tels quels, tick après tick, pendant qu'on bascule des leviers et qu'on
// retire ou ajoute des bâtiments. On relève à chaque tick toutes les sorties des
// portes et l'état du réseau de chaque câble.

import { loadModule } from "./load_js.mjs";

const globalConfig = { mapChunkSize: 16, tileSize: 32, debug: {} };
const safeModulo = (n, m) => ((n % m) + m) % m;
const vector = loadModule("core/vector.js", { globalConfig, safeModulo });
const { Vector, enumDirection } = vector;

class BasicSerializableObject {}
const { BaseItem } = loadModule("game/base_item.js", { globalConfig, BasicSerializableObject });
const colors = loadModule("game/colors.js");
const booleans = loadModule("game/items/boolean_item.js", { BaseItem, globalConfig, types: {} });
const colorItems = loadModule(
    "game/items/color_item.js",
    { BaseItem, globalConfig, types: {}, ...colors },
    { stubImports: true }
);
// `COLOR_ITEM_SINGLETONS` est rempli à la fin du module.
const { COLOR_ITEM_SINGLETONS } = colorItems;

const { ShapeDefinition } = loadModule("game/shape_definition.js", {
    ...colors,
    BasicSerializableObject,
    Vector,
    globalConfig: {},
    THEME: {},
    types: {},
    makeOffscreenBuffer: () => {
        throw new Error("non utilisé");
    },
    smoothenDpi: x => x,
    DrawParameters: class {},
});
const { ShapeItem } = loadModule(
    "game/items/shape_item.js",
    { BaseItem, ShapeDefinition, globalConfig, types: {}, THEME: {} },
    { stubImports: true }
);

class Component {}
const types = { fixedSizeArray: () => null, structured: () => null, nullable: () => null, bool: null };
const wireComp = loadModule("game/components/wire.js", { Component });
const { WireTunnelComponent } = loadModule("game/components/wire_tunnel.js", { Component });
const pins = loadModule("game/components/wired_pins.js", {
    Component,
    BaseItem,
    types,
    typeItemSingleton: null,
    ...vector,
});
const gates = loadModule("game/components/logic_gate.js", { Component });
const { LeverComponent } = loadModule("game/components/lever.js", { Component, types });
const { ConstantSignalComponent } = loadModule("game/components/constant_signal.js", {
    Component,
    BaseItem,
    types,
    typeItemSingleton: null,
});

class MetaBuilding {
    constructor(id) {
        this.id = id;
    }
}
const buildingScope = {
    ...vector,
    ...pins,
    ...gates,
    LeverComponent,
    ConstantSignalComponent,
    WireTunnelComponent,
    MetaBuilding,
    defaultBuildingVariant: "default",
    generateMatrixRotations: () => [],
    enumHubGoalRewards: {},
};
const { MetaLogicGateBuilding } = loadModule("game/buildings/logic_gate.js", buildingScope, {
    stubImports: true,
});
const { MetaTransistorBuilding } = loadModule("game/buildings/transistor.js", buildingScope, {
    stubImports: true,
});
const { MetaLeverBuilding } = loadModule("game/buildings/lever.js", buildingScope, { stubImports: true });
const { MetaConstantSignalBuilding } = loadModule("game/buildings/constant_signal.js", buildingScope, {
    stubImports: true,
});
const { MetaWireTunnelBuilding } = loadModule("game/buildings/wire_tunnel.js", buildingScope, {
    stubImports: true,
});
const { MetaAnalyzerBuilding } = loadModule("game/buildings/analyzer.js", buildingScope, {
    stubImports: true,
});
const { MetaComparatorBuilding } = loadModule("game/buildings/comparator.js", buildingScope, {
    stubImports: true,
});
const silhouette = class {
    getSilhouetteColor() {}
};
const { MetaVirtualProcessorBuilding } = loadModule(
    "game/buildings/virtual_processor.js",
    {
        ...buildingScope,
        MetaCutterBuilding: silhouette,
        MetaRotaterBuilding: silhouette,
        MetaStackerBuilding: silhouette,
        MetaPainterBuilding: silhouette,
    },
    { stubImports: true }
);

// Carte et gestionnaire d'entités minimaux.
let entities = [];
const root = {
    gameInitialized: true,
    shapeDefinitionMgr: null,
    signals: new Proxy({}, { get: () => ({ add() {}, dispatch() {} }) }),
    entityMgr: {
        getAllWithComponent: C => entities.filter(e => e.components[C.getId()]),
    },
    map: {
        getLayersContentsMultipleXY(x, y) {
            const regular = entities.find(e => e.layer === "regular" && e.at(x, y));
            const wires = entities.find(e => e.layer === "wires" && e.at(x, y));
            return [regular, wires].filter(Boolean);
        },
    },
};

class GameSystem {
    constructor(root) {
        this.root = root;
    }
}
class GameSystemWithFilter extends GameSystem {
    constructor(root, required) {
        super(root);
        this.required = required;
    }
    get allEntities() {
        return entities.filter(e => this.required.every(C => e.components[C.getId()]));
    }
}
const systemScope = {
    ...vector,
    ...pins,
    ...gates,
    ...booleans,
    ...colorItems,
    ...colors,
    ...wireComp,
    WireTunnelComponent,
    LeverComponent,
    ConstantSignalComponent,
    GameSystem,
    GameSystemWithFilter,
    globalConfig,
    Loader: { getSprite: () => null },
    StaleAreaDetector: class {
        update() {}
        invalidate() {}
    },
    createLogger: () => ({ log() {}, warn() {}, error() {} }),
    fastArrayDeleteValueIfContained(array, value) {
        const i = array.indexOf(value);
        if (i >= 0) {
            array[i] = array[array.length - 1];
            array.pop();
        }
    },
    ACHIEVEMENTS: {},
};
// Le vrai gestionnaire de formes : opérations des processeurs virtuels.
const { ShapeDefinitionManager } = loadModule(
    "game/shape_definition_manager.js",
    {
        ...colors,
        ShapeDefinition,
        ShapeItem,
        BasicSerializableObject,
        ACHIEVEMENTS: {},
        createLogger: () => ({ log() {}, warn() {}, error() {} }),
    },
    { stubImports: true }
);
root.shapeDefinitionMgr = new ShapeDefinitionManager(root);
systemScope.ShapeDefinition = ShapeDefinition;

const { WireSystem } = loadModule("game/systems/wire.js", systemScope, { stubImports: true });
const { LogicGateSystem } = loadModule("game/systems/logic_gate.js", systemScope, { stubImports: true });
const { LeverSystem } = loadModule("game/systems/lever.js", systemScope, { stubImports: true });
const { ConstantSignalSystem } = loadModule("game/systems/constant_signal.js", systemScope, {
    stubImports: true,
});

const angleToDir = { 0: "top", 90: "right", 180: "bottom", 270: "left" };

function makeEntity(spec) {
    const origin = new Vector(spec.x, spec.y);
    const rotation = spec.rot;
    const entity = {
        uid: ++makeEntity.uid,
        spec,
        layer: spec.kind === "lever" ? "regular" : "wires",
        components: {
            StaticMapEntity: {
                origin,
                rotation,
                originalRotation: rotation,
                localTileToWorld(local) {
                    const r = local.rotateFastMultipleOf90(rotation);
                    return new Vector(r.x + origin.x, r.y + origin.y);
                },
                localDirectionToWorld(dir) {
                    return Vector.transformDirectionFromMultipleOf90(dir, rotation);
                },
            },
        },
        at: (x, y) => x === spec.x && y === spec.y,
        addComponent(c) {
            this.components[c.constructor.getId()] = c;
        },
    };
    switch (spec.kind) {
        case "wire":
            entity.addComponent(new wireComp.WireComponent({ variant: spec.variant }));
            break;
        case "wire_tunnel":
            new MetaWireTunnelBuilding().setupEntityComponents(entity);
            break;
        case "logic_gate": {
            const meta = new MetaLogicGateBuilding();
            meta.setupEntityComponents(entity);
            meta.updateVariants(entity, 0, spec.variant);
            break;
        }
        case "transistor": {
            const meta = new MetaTransistorBuilding();
            meta.setupEntityComponents(entity);
            meta.updateVariants(entity, 0, spec.variant);
            break;
        }
        case "analyzer":
            new MetaAnalyzerBuilding().setupEntityComponents(entity);
            break;
        case "comparator":
            new MetaComparatorBuilding().setupEntityComponents(entity);
            break;
        case "virtual_processor": {
            const meta = new MetaVirtualProcessorBuilding();
            meta.setupEntityComponents(entity);
            meta.updateVariants(entity, 0, spec.variant);
            break;
        }
        case "lever":
            new MetaLeverBuilding().setupEntityComponents(entity);
            entity.components.Lever.toggled = !!spec.on;
            break;
        case "constant_signal":
            new MetaConstantSignalBuilding().setupEntityComponents(entity);
            entity.components.ConstantSignal.signal = decode(spec.signal);
            break;
    }
    return entity;
}
makeEntity.uid = 0;

function decode(code) {
    if (code === "") return null;
    if (code === "0") return booleans.BOOL_FALSE_SINGLETON;
    if (code === "1") return booleans.BOOL_TRUE_SINGLETON;
    if (code.length > 1) return root.shapeDefinitionMgr.getShapeItemFromShortKey(code);
    return COLOR_ITEM_SINGLETONS[colors.enumShortcodeToColor[code]];
}

function encode(item) {
    if (!item) return "";
    if (item.getItemType() === "boolean") return item.value ? "1" : "0";
    if (item.getItemType() === "shape") return item.definition.getHash();
    return colors.enumColorToShortcode[item.color];
}

function mulberry32(seed) {
    return () => {
        seed |= 0;
        seed = (seed + 0x6d2b79f5) | 0;
        let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}

const W = 9;
const H = 7;
const SIGNALS = [
    "",
    "0",
    "1",
    "1",
    "r",
    "u",
    "CuCuCuCu",
    "RrSg--Cb",
    "--Rw----",
    "Cu------:SrSr----",
    "CgRgSgWg:Cu--Cu--:--Ry----",
];

function randomWireSpec(rand, x, y) {
    const r = rand();
    const rot = [0, 90, 180, 270][Math.floor(rand() * 4)];
    if (r < 0.55) return { kind: "wire", variant: rand() < 0.75 ? "first" : "second", x, y, rot: 0 };
    if (r < 0.62) return { kind: "wire_tunnel", variant: "default", x, y, rot: 0 };
    if (r < 0.8) {
        const variant = ["default", "or", "not", "xor"][Math.floor(rand() * 4)];
        return { kind: "logic_gate", variant, x, y, rot };
    }
    if (r < 0.87) {
        return { kind: "transistor", variant: rand() < 0.5 ? "default" : "mirrored", x, y, rot };
    }
    if (r < 0.91) {
        const kind = rand() < 0.5 ? "analyzer" : "comparator";
        return { kind, variant: "default", x, y, rot };
    }
    if (r < 0.96) {
        const variant = ["default", "rotater", "stacker", "painter", "unstacker"][Math.floor(rand() * 5)];
        return { kind: "virtual_processor", variant, x, y, rot };
    }
    const signal = SIGNALS[Math.floor(rand() * SIGNALS.length)];
    return { kind: "constant_signal", variant: "default", x, y, rot, signal };
}

// Signaux des circuits « alimentés » : surtout des formes et des couleurs.
const FEED_SIGNALS = [...SIGNALS.slice(6), ...SIGNALS.slice(6), "r", "g", "b", "w", "u", "1", "0", ""];
const PROCESSORS = [
    ["analyzer", "default"],
    ["comparator", "default"],
    ["virtual_processor", "default"],
    ["virtual_processor", "rotater"],
    ["virtual_processor", "stacker"],
    ["virtual_processor", "painter"],
    ["virtual_processor", "unstacker"],
    ["transistor", "default"],
    ["logic_gate", "xor"],
];
const dirToAngle = { top: 0, right: 90, bottom: 180, left: 270 };
const dirOffset = { top: [0, -1], right: [1, 0], bottom: [0, 1], left: [-1, 0] };
const inverse = { top: "bottom", bottom: "top", left: "right", right: "left" };

// Circuit « alimenté » : des processeurs dont chaque entrée reçoit un émetteur
// constant collé au pin et chaque sortie un câble, qui peut en toucher d'autres.
function feedBuildings(rand) {
    const specs = [];
    const used = new Set();
    const key = (x, y) => x + "," + y;
    const count = 3 + Math.floor(rand() * 5);
    for (let i = 0; i < count; ++i) {
        const x = 1 + Math.floor(rand() * (W - 2));
        const y = 1 + Math.floor(rand() * (H - 2));
        if (used.has(key(x, y))) continue;
        const [kind, variant] = PROCESSORS[Math.floor(rand() * PROCESSORS.length)];
        const rot = [0, 90, 180, 270][Math.floor(rand() * 4)];
        const spec = { kind, variant, x, y, rot };
        used.add(key(x, y));
        specs.push(spec);
    }
    for (const spec of [...specs]) {
        const entity = makeEntity(spec);
        for (const slot of entity.components.WiredPins.slots) {
            const dir = entity.components.StaticMapEntity.localDirectionToWorld(slot.direction);
            const [dx, dy] = dirOffset[dir];
            const x = spec.x + dx;
            const y = spec.y + dy;
            if (x < 0 || y < 0 || x >= W || y >= H || used.has(key(x, y))) continue;
            used.add(key(x, y));
            if (slot.type === pins.enumPinSlotType.logicalAcceptor) {
                const signal = FEED_SIGNALS[Math.floor(rand() * FEED_SIGNALS.length)];
                const rot = dirToAngle[inverse[dir]];
                specs.push({ kind: "constant_signal", variant: "default", x, y, rot, signal });
            } else {
                specs.push({ kind: "wire", variant: "first", x, y, rot: 0 });
            }
        }
    }
    return specs;
}

const N = Number(process.argv[2] || 60);
const rand = mulberry32(0x3172e5);
const cases = [];
for (let c = 0; c < N; ++c) {
    entities = [];
    const feed = c % 2 === 1;
    const buildings = feed ? feedBuildings(rand) : [];
    const density = feed ? 0 : 0.45 + rand() * 0.4;
    for (let y = 0; y < H && !feed; ++y) {
        for (let x = 0; x < W; ++x) {
            if (rand() < density) buildings.push(randomWireSpec(rand, x, y));
            if (rand() < 0.12) {
                const rot = [0, 90, 180, 270][Math.floor(rand() * 4)];
                buildings.push({ kind: "lever", variant: "default", x, y, rot, on: rand() < 0.5 });
            }
        }
    }
    for (const spec of buildings) entities.push(makeEntity(spec));

    const wireSystem = new WireSystem(root);
    const systems = [
        new ConstantSignalSystem(root),
        new LeverSystem(root),
        wireSystem,
        new LogicGateSystem(root),
    ];

    const ticks = 40 + Math.floor(rand() * 40);
    const ops = [];
    const frames = [];
    for (let t = 0; t < ticks; ++t) {
        // Avant le tick : on bascule un levier, ou on retire / ajoute un bâtiment.
        const r = rand();
        const levers = entities.filter(e => e.components.Lever);
        const signals = entities.filter(e => e.components.ConstantSignal);
        if (feed && r < 0.2 && signals.length > 0) {
            const emitter = signals[Math.floor(rand() * signals.length)];
            const signal = FEED_SIGNALS[Math.floor(rand() * FEED_SIGNALS.length)];
            emitter.components.ConstantSignal.signal = decode(signal);
            ops.push({ t, op: "signal", x: emitter.spec.x, y: emitter.spec.y, signal });
        } else if (r < 0.25 && levers.length > 0) {
            const lever = levers[Math.floor(rand() * levers.length)];
            lever.components.Lever.toggled = !lever.components.Lever.toggled;
            ops.push({ t, op: "toggle", x: lever.spec.x, y: lever.spec.y });
        } else if (r < 0.3 && entities.length > 0) {
            const victim = entities[Math.floor(rand() * entities.length)];
            entities = entities.filter(e => e !== victim);
            wireSystem.queueRecomputeIfWire(victim);
            ops.push({ t, op: "remove", x: victim.spec.x, y: victim.spec.y, layer: victim.layer });
        } else if (r < 0.35) {
            const x = Math.floor(rand() * W);
            const y = Math.floor(rand() * H);
            if (!entities.some(e => e.layer === "wires" && e.at(x, y))) {
                const spec = randomWireSpec(rand, x, y);
                const entity = makeEntity(spec);
                entities.push(entity);
                wireSystem.queueRecomputeIfWire(entity);
                ops.push({ t, op: "place", building: spec });
            }
        }

        for (const system of systems) system.update();

        // Sorties de chaque porte (pins émetteurs séparés par "/"), puis état du réseau
        // de chaque câble, tuile par tuile (ligne par ligne) ; "-" = pas de réseau,
        // "x" = conflit, sinon la valeur (vide = rien).
        const sorted = [...entities].sort((a, b) => a.spec.y - b.spec.y || a.spec.x - b.spec.x);
        const gateOut = sorted
            .filter(e => e.components.LogicGate)
            .map(e =>
                e.components.WiredPins.slots
                    .filter(s => s.type === pins.enumPinSlotType.logicalEjector)
                    .map(s => encode(s.value))
                    .join("/")
            );
        const wires = sorted
            .filter(e => e.components.Wire)
            .map(e => {
                const network = e.components.Wire.linkedNetwork;
                if (!network) return "-";
                if (network.valueConflict) return "x";
                return encode(network.currentValue);
            });
        frames.push({ gates: gateOut.join(","), wires: wires.join(",") });
    }
    cases.push({ buildings, ops, frames });
}

process.stdout.write(JSON.stringify({ cases }) + "\n");
