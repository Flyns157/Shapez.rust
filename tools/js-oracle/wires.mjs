// Vecteurs golden de la couche wires : réseaux, émetteurs constants, leviers et portes.
//
//   node tools/js-oracle/wires.mjs [N=60] > crates/shapez_sim/tests/golden/wires.json
//
// Chaque cas pose au hasard, sur une petite grille, des câbles (deux couleurs), des
// tunnels de câbles, des portes ET/OU/NON/XOR, des transistors, des émetteurs constants
// (couche wires) et des leviers (couche normale). Les systèmes du jeu (`WireSystem`,
// `LogicGateSystem`, `LeverSystem`, `ConstantSignalSystem`) tournent ensuite tels quels,
// tick après tick, pendant qu'on bascule des leviers et qu'on retire ou ajoute des
// bâtiments. On relève à chaque tick la sortie des portes et l'état du réseau de chaque
// câble.

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

// Carte et gestionnaire d'entités minimaux.
let entities = [];
const root = {
    gameInitialized: true,
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
    return COLOR_ITEM_SINGLETONS[colors.enumShortcodeToColor[code]];
}

function encode(item) {
    if (!item) return "";
    if (item.getItemType() === "boolean") return item.value ? "1" : "0";
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
const SIGNALS = ["", "0", "1", "1", "r", "u"];

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
    const signal = SIGNALS[Math.floor(rand() * SIGNALS.length)];
    return { kind: "constant_signal", variant: "default", x, y, rot, signal };
}

const N = Number(process.argv[2] || 60);
const rand = mulberry32(0x3172e5);
const cases = [];
for (let c = 0; c < N; ++c) {
    entities = [];
    const buildings = [];
    const density = 0.45 + rand() * 0.4;
    for (let y = 0; y < H; ++y) {
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
        if (r < 0.25 && levers.length > 0) {
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

        // Sortie de chaque porte, puis état du réseau de chaque câble, tuile par tuile
        // (ligne par ligne) ; "-" = pas de réseau, "x" = conflit, sinon la valeur (vide
        // = rien).
        const sorted = [...entities].sort((a, b) => a.spec.y - b.spec.y || a.spec.x - b.spec.x);
        const gateOut = sorted
            .filter(e => e.components.LogicGate)
            .map(e => encode(e.components.WiredPins.slots[0].value));
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
