// Génère les vecteurs golden des chemins de convoyeurs à partir de belt_path.js.
//
//   node tools/js-oracle/belts.mjs [N=100] > crates/shapez_sim/tests/golden/belts.json
//
// Chaque cas construit un chemin (convoyeurs droits ou en virage), puis joue une suite
// de ticks : injection d'un item au début, ouverture ou non de la sortie, et de temps
// en temps une modification du chemin (ajout ou retrait au début ou à la fin). On
// relève l'état du chemin (espacement au premier item, distances entre items).

import { loadModule } from "./load_js.mjs";

// Le code JS tourne tel quel, mais dans les unités entières de la version Rust
// (1 tuile = 10 000) : toutes les valeurs restent des entiers exacts en flottant, ce
// qui supprime les erreurs d'arrondi (0.63 n'est pas représentable) et permet une
// comparaison stricte. 2 × (1/60) × 6300 vaut exactement 210.
const globalConfig = { itemSpacingOnBelts: 6300, tileSize: 32, debug: {} };
const fakeBounds = { getUnion: () => fakeBounds, allScaled: () => fakeBounds };

const { BeltPath } = loadModule("game/belt_path.js", {
    globalConfig,
    BasicSerializableObject: class {},
    types: {},
    typeItemSingleton: {},
    enumDirection: { top: "top", right: "right", bottom: "bottom", left: "left" },
    enumDirectionToVector: {},
    enumInvertedDirections: {},
    Vector: class {},
    createLogger: () => ({ log() {}, warn() {}, error() {} }),
    clamp: (v, a, b) => Math.max(a, Math.min(b, v)),
    epsilonCompare: () => 0,
    round4Digits: x => x,
    smoothenDpi: x => x,
});

// La cible du chemin est pilotée par le scénario.
let exitOpen = false;
BeltPath.prototype.computeAcceptingEntityAndSlot = () => ({ acceptor: () => exitOpen });

const root = {
    hubGoals: { getBeltBaseSpeed: () => 2 },
    dynamicTickrate: { deltaSeconds: 1 / 60 },
    gameMode: { throughputDoesNotMatter: () => false },
};

function makeBelt(curved) {
    return {
        components: {
            Belt: {
                direction: curved ? "left" : "top",
                getEffectiveLengthTiles: () => (curved ? 7800 : 10000),
                assignedPath: null,
            },
            StaticMapEntity: { getTileSpaceBounds: () => fakeBounds },
        },
    };
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

const toUnits = x => {
    if (!Number.isInteger(x)) throw new Error("valeur non entière : " + x);
    return x;
};

function snapshot(path, tick) {
    return {
        tick,
        total: toUnits(path.totalLength),
        spacing: toUnits(path.spacingToFirstItem),
        items: path.items.map(([next]) => toUnits(next)),
    };
}

const N = Number(process.argv[2] || 100);
const rand = mulberry32(0xbe17);
const cases = [];
for (let c = 0; c < N; ++c) {
    const nBelts = 1 + Math.floor(rand() * 8);
    const belts = [];
    for (let i = 0; i < nBelts; ++i) belts.push(rand() < 0.25);
    const path = new BeltPath(root, belts.map(makeBelt));
    const injectRate = 0.2 + rand() * 0.8;
    const openRate = rand();
    let steps = "";
    const ops = [];
    const snapshots = [];
    const ticks = 300 + Math.floor(rand() * 600);
    for (let t = 0; t < ticks; ++t) {
        const step = { t, inject: rand() < injectRate, open: rand() < openRate };
        const r = rand();
        if (r < 0.01) {
            step.op = rand() < 0.25 ? "end_curved" : "end";
            path.extendOnEnd(makeBelt(step.op === "end_curved"));
        } else if (r < 0.02) {
            step.op = "begin";
            path.extendOnBeginning(makeBelt(false));
        } else if (r < 0.025 && path.entityPath.length > 1) {
            step.op = "delete_start";
            path.deleteEntityOnStart(path.entityPath[0]);
        } else if (r < 0.03 && path.entityPath.length > 1) {
            step.op = "delete_end";
            path.deleteEntityOnEnd(path.entityPath[path.entityPath.length - 1]);
        } else if (r < 0.035 && path.entityPath.length > 2) {
            // On garde la première moitié ; la seconde est relevée puis jetée.
            const index = 1 + Math.floor(rand() * (path.entityPath.length - 2));
            step.op = "split";
            step.index = index;
            const second = path.deleteEntityOnPathSplitIntoTwo(path.entityPath[index]);
            step.second = snapshot(second, t);
        }
        exitOpen = step.open;
        if (step.inject) path.tryAcceptItem({});
        path.update();
        // Un chiffre par tick : bit 0 = injection, bit 1 = sortie ouverte.
        steps += String((step.inject ? 1 : 0) | (step.open ? 2 : 0));
        if (step.op) {
            const { inject, open, ...op } = step;
            ops.push(op);
        }
        if (t % 25 === 24 || step.op) snapshots.push(snapshot(path, t));
    }
    cases.push({ belts, steps, ops, snapshots });
}

process.stdout.write(JSON.stringify({ cases }) + "\n");
