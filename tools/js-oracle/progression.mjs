// Vecteurs golden de la progression : paliers d'amélioration (modes/regular.js) et
// formes du mode libre (HubGoals.computeFreeplayShape).
//
//   node tools/js-oracle/progression.mjs > crates/shapez_sim/tests/golden/progression.json

import { loadModule } from "./load_js.mjs";

const globals = {
    G_WEGAME_VERSION: false,
    G_CHINA_VERSION: false,
    G_IS_STANDALONE: true,
    G_IS_STEAM_DEMO: false,
    WEB_STEAM_SSO_AUTHENTICATED: false,
};

const utils = loadModule("core/utils.js", { ...globals }, { stubImports: true });
const colors = loadModule("game/colors.js");
const { RandomNumberGenerator } = loadModule("core/rng.js");
const { ShapeDefinition, enumSubShape } = loadModule(
    "game/shape_definition.js",
    {
        ...colors,
        BasicSerializableObject: class {},
        Vector: class {},
        globalConfig: {},
        THEME: {},
        types: {},
        smoothenDpi: x => x,
    },
    { stubImports: true }
);

const { generateUpgrades } = loadModule(
    "game/modes/regular.js",
    {
        ...globals,
        findNiceIntegerValue: utils.findNiceIntegerValue,
        finalGameShape: "RuCw--Cw:----Ru--",
        MOD_SIGNALS: { modifyUpgrades: { dispatch() {} } },
        ShapeDefinition,
    },
    { extraExports: ["generateUpgrades"], stubImports: true }
);

const { HubGoals } = loadModule(
    "game/hub_goals.js",
    {
        ...globals,
        ...colors,
        clamp: utils.clamp,
        RandomNumberGenerator,
        ShapeDefinition,
        enumSubShape,
        BasicSerializableObject: class {},
        globalConfig: {},
        types: {},
    },
    { stubImports: true }
);

const TIERS = 40;
const all = generateUpgrades(false, 1);
const upgrades = {};
for (const id of Object.keys(all)) {
    upgrades[id] = all[id].slice(0, TIERS).map(t => ({
        improvement: t.improvement,
        required: t.required.map(r => [r.shape, r.amount]),
    }));
}

const freeplay = [];
for (const seed of [0, 1234, 99999]) {
    for (let level = 27; level <= 160; ++level) {
        const fake = {
            level,
            root: { map: { seed }, shapeDefinitionMgr: { registerOrReturnHandle: d => d } },
            generateRandomColorSet: HubGoals.prototype.generateRandomColorSet,
        };
        const def = HubGoals.prototype.computeFreeplayShape.call(fake, level);
        freeplay.push({ seed, level, shape: def.getHash() });
    }
}

process.stdout.write(JSON.stringify({ upgrades, freeplay }) + "\n");
