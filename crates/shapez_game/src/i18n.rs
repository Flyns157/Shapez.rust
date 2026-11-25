//! Traductions (port de `translations.js` et `languages.js`).
//!
//! Les textes viennent des fichiers YAML du jeu d'origine (`assets/i18n/base-*.yaml`,
//! importés par `cargo xtask import-assets`), complétés par les textes propres au
//! portage (`crates/shapez_game/i18n/port-*.yaml`, sous la clé `port`). Une clé absente
//! de la langue choisie retombe sur l'anglais, puis sur la clé elle-même.
//!
//! ```ignore
//! t("mainMenu.play");
//! tf("ingame.levelCompleteNotification.levelTitle", &[("level", "3")]);
//! ```

use std::{
    collections::HashMap,
    path::Path,
    sync::{OnceLock, RwLock},
};

use bevy::prelude::*;
use serde_norway::Value;
use shapez_sim::{
    building::{BuildingKind, Variant},
    hub_goals::Reward,
    upgrades::UpgradeKind,
};

/// Une langue proposée par le jeu (`LANGUAGES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// Identifiant du JS (`"fr"`, `"pt-BR"`, `"kor"`…).
    pub id: &'static str,
    /// Nom dans la langue elle-même.
    pub name: &'static str,
    /// Code ISO servant à la détection (`code` du JS).
    pub code: &'static str,
    /// Fichier `base-<file>.yaml`.
    pub file: &'static str,
    /// Écriture couverte par la police du jeu (Barlow, alphabet latin seulement).
    pub latin: bool,
}

const fn lang(
    id: &'static str,
    name: &'static str,
    code: &'static str,
    file: &'static str,
) -> Language {
    Language {
        id,
        name,
        code,
        file,
        latin: true,
    }
}

const fn non_latin(
    id: &'static str,
    name: &'static str,
    code: &'static str,
    file: &'static str,
) -> Language {
    Language {
        latin: false,
        ..lang(id, name, code, file)
    }
}

/// Les langues du jeu d'origine, dans son ordre.
pub const LANGUAGES: [Language; 24] = [
    lang("en", "English", "en", "en"),
    non_latin("zh-CN", "简体中文", "zh", "zh-CN"),
    non_latin("zh-TW", "繁體中文", "zh", "zh-TW"),
    non_latin("ja", "日本語", "ja", "ja"),
    non_latin("kor", "한국어", "ko", "kor"),
    lang("cs", "Čeština", "cs", "cz"),
    lang("da", "Dansk", "da", "da"),
    lang("de", "Deutsch", "de", "de"),
    lang("es-419", "Español", "es", "es"),
    lang("fr", "Français", "fr", "fr"),
    lang("it", "Italiano", "it", "it"),
    lang("hu", "Magyar", "hu", "hu"),
    lang("nl", "Nederlands", "nl", "nl"),
    lang("no", "Norsk", "no", "no"),
    lang("pl", "Polski", "pl", "pl"),
    lang("pt-PT", "Português", "pt", "pt-PT"),
    lang("pt-BR", "Português - Brasil", "pt-BR", "pt-BR"),
    lang("ro", "Română", "ro", "ro"),
    non_latin("ru", "Русский", "ru", "ru"),
    lang("fi", "Suomi", "fi", "fi"),
    lang("sv", "Svenska", "sv", "sv"),
    lang("tr", "Türkçe", "tr", "tr"),
    non_latin("uk", "Українська", "uk", "uk"),
    non_latin("he", "עברית", "he", "he"),
];

/// Textes propres au portage, embarqués dans le binaire.
const PORT_FILES: [(&str, &str); 2] = [
    ("en", include_str!("../i18n/port-en.yaml")),
    ("fr", include_str!("../i18n/port-fr.yaml")),
];

/// Table des textes d'une langue, clés pointées (`mainMenu.play`).
#[derive(Debug, Clone, Default)]
pub struct Translations {
    pub language: Option<Language>,
    texts: HashMap<String, String>,
}

impl Translations {
    /// Charge `language` depuis `dir` (le dossier `assets/i18n`), par-dessus l'anglais.
    /// Un fichier manquant ou illisible est signalé et ignoré.
    pub fn load(dir: &Path, language: Language) -> Self {
        let mut out = Self {
            language: Some(language),
            texts: HashMap::new(),
        };
        let english = LANGUAGES[0];
        for lang in [english, language] {
            let path = dir.join(format!("base-{}.yaml", lang.file));
            match std::fs::read_to_string(&path) {
                Ok(text) => out.merge_yaml(&text),
                Err(e) => warn!("traductions introuvables ({}) : {e}", path.display()),
            }
            if let Some((_, text)) = PORT_FILES.iter().find(|(id, _)| *id == lang.id) {
                out.merge_yaml(text);
            }
            if language == english {
                break;
            }
        }
        out
    }

    /// Ajoute (en écrasant) les textes d'un document YAML.
    pub fn merge_yaml(&mut self, text: &str) {
        match serde_norway::from_str::<Value>(text) {
            Ok(value) => flatten("", &value, &mut self.texts),
            Err(e) => warn!("traductions illisibles : {e}"),
        }
    }

    /// Texte brut (avec balises et paramètres) de `key`.
    pub fn raw(&self, key: &str) -> Option<&str> {
        self.texts.get(key).map(String::as_str)
    }

    /// Texte de `key`, paramètres `<nom>` remplacés et balises HTML retirées.
    pub fn get(&self, key: &str, args: &[(&str, &str)]) -> String {
        match self.raw(key) {
            Some(raw) => render(raw, args),
            None => key.to_owned(),
        }
    }
}

fn flatten(prefix: &str, value: &Value, out: &mut HashMap<String, String>) {
    let key = |k: &str| {
        if prefix.is_empty() {
            k.to_owned()
        } else {
            format!("{prefix}.{k}")
        }
    };
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                let k = match k {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                flatten(&key(&k), v, out);
            }
        }
        Value::Sequence(items) => {
            for (i, v) in items.iter().enumerate() {
                flatten(&key(&i.to_string()), v, out);
            }
        }
        Value::String(s) => {
            out.insert(prefix.to_owned(), s.clone());
        }
        Value::Number(n) => {
            out.insert(prefix.to_owned(), n.to_string());
        }
        Value::Bool(b) => {
            out.insert(prefix.to_owned(), b.to_string());
        }
        _ => {}
    }
}

/// Remplace les paramètres `<nom>`, transforme `<br>` en saut de ligne et retire les
/// autres balises (`<strong>`, `<span class=…>`…), que le jeu d'origine rendait en HTML.
pub fn render(raw: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let tag = &after[..end];
        if let Some((_, value)) = args.iter().find(|(name, _)| *name == tag) {
            out.push_str(value);
        } else if tag.eq_ignore_ascii_case("br") || tag.eq_ignore_ascii_case("br/") {
            out.push('\n');
        } else if !is_markup(tag) {
            // Pas une balise (`a < b > c`) : texte gardé tel quel.
            out.push('<');
            out.push_str(tag);
            out.push('>');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// `strong`, `/strong`, `span class='x'`, `key`… : un nom de balise sans espace avant
/// ses attributs. Les paramètres non fournis sont aussi retirés.
fn is_markup(tag: &str) -> bool {
    let name = tag.strip_prefix('/').unwrap_or(tag);
    if name.starts_with(char::is_whitespace) {
        return false;
    }
    let name = name.split_whitespace().next().unwrap_or("");
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/')
}

/// `mapLanguageCodeToId` : `de-DE`, `pt_BR.UTF-8`, `fr`… vers une langue du jeu.
pub fn language_for_code(code: &str) -> Option<Language> {
    let key = code
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .replace('_', "-")
        .to_lowercase();
    if key.is_empty() {
        return None;
    }
    let short = key.split('-').next().unwrap_or("");
    let find = |pred: &dyn Fn(&str) -> bool| {
        LANGUAGES
            .iter()
            .copied()
            .find(|l| pred(&l.code.to_lowercase()))
    };
    find(&|c| c == key)
        .or_else(|| {
            LANGUAGES
                .iter()
                .copied()
                .find(|l| l.id.to_lowercase() == key)
        })
        .or_else(|| find(&|c| c == short))
        .or_else(|| find(&|c| c.split('-').next() == Some(short)))
}

/// Langue demandée : `SHAPEZ_LANG`, puis la langue du système (`LC_ALL`,
/// `LC_MESSAGES`, `LANG`), puis l'anglais. Les langues non latines retombent pour
/// l'instant sur l'anglais, faute de police.
pub fn detect_language() -> Language {
    let wanted = ["SHAPEZ_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find_map(|code| language_for_code(&code));
    match wanted {
        Some(lang) if lang.latin => lang,
        Some(lang) => {
            warn!(
                "langue {} pas encore prise en charge (police manquante) : anglais utilisé",
                lang.id
            );
            LANGUAGES[0]
        }
        None => LANGUAGES[0],
    }
}

fn global() -> &'static RwLock<Translations> {
    static TRANSLATIONS: OnceLock<RwLock<Translations>> = OnceLock::new();
    TRANSLATIONS.get_or_init(Default::default)
}

/// Remplace les traductions utilisées par [`t`] et [`tf`].
pub fn set_translations(translations: Translations) {
    *global().write().unwrap_or_else(|e| e.into_inner()) = translations;
}

/// Langue active.
pub fn current_language() -> Option<Language> {
    global().read().unwrap_or_else(|e| e.into_inner()).language
}

/// Texte traduit de `key`.
pub fn t(key: &str) -> String {
    tf(key, &[])
}

/// Texte traduit de `key` avec ses paramètres `<nom>`.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    global()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(key, args)
}

/// Clé de variante du JS (`buildings.<id>.<variante>`).
pub fn variant_key(kind: BuildingKind, variant: Variant) -> &'static str {
    use BuildingKind as K;
    match (kind, variant) {
        (_, Variant::Default) => "default",
        (K::UndergroundBelt, Variant::Tier2) => "tier2",
        (K::Balancer, Variant::Merger) => "merger",
        (K::Balancer, Variant::MergerInverse) => "merger-inverse",
        (K::Balancer, Variant::Splitter) => "splitter",
        (K::Balancer, Variant::SplitterInverse) => "splitter-inverse",
        (K::Miner, Variant::Chainable) => "chainable",
        (K::Cutter, Variant::Quad) => "quad",
        (K::Rotater, Variant::Ccw) => "ccw",
        (K::Rotater, Variant::Rotate180) => "rotate180",
        (K::Painter, Variant::Mirrored) => "mirrored",
        (K::Painter, Variant::Double) => "double",
        (K::Painter, Variant::Quad) => "quad",
        (K::Wire, Variant::Second) => "second",
        (K::LogicGate, Variant::Not) => "not",
        (K::LogicGate, Variant::Xor) => "xor",
        (K::LogicGate, Variant::Or) => "or",
        (K::Transistor, Variant::Mirrored) => "mirrored",
        (K::VirtualProcessor, Variant::Rotate) => "rotater",
        (K::VirtualProcessor, Variant::Unstack) => "unstacker",
        (K::VirtualProcessor, Variant::Stack) => "stacker",
        (K::VirtualProcessor, Variant::Paint) => "painter",
        _ => "default",
    }
}

/// Nom traduit d'un bâtiment (`buildings.<id>.<variante>.name`).
pub fn building_name(kind: BuildingKind, variant: Variant) -> String {
    t(&format!(
        "buildings.{}.{}.name",
        kind.js_id(),
        variant_key(kind, variant)
    ))
}

/// Nom traduit d'une récompense de niveau (`storyRewards.<id>.title`).
pub fn reward_name(reward: Reward) -> String {
    let id = match reward {
        Reward::CutterAndTrash => "reward_cutter_and_trash",
        Reward::Rotater => "reward_rotater",
        Reward::Painter => "reward_painter",
        Reward::Mixer => "reward_mixer",
        Reward::Stacker => "reward_stacker",
        Reward::Balancer => "reward_balancer",
        Reward::Tunnel => "reward_tunnel",
        Reward::RotaterCcw => "reward_rotater_ccw",
        Reward::Rotater180 => "reward_rotater_180",
        Reward::MinerChainable => "reward_miner_chainable",
        Reward::UndergroundBeltTier2 => "reward_underground_belt_tier_2",
        Reward::BeltReader => "reward_belt_reader",
        Reward::Splitter => "reward_splitter",
        Reward::CutterQuad => "reward_cutter_quad",
        Reward::PainterDouble => "reward_painter_double",
        Reward::Storage => "reward_storage",
        Reward::Merger => "reward_merger",
        Reward::WiresPainterAndLevers => "reward_wires_painter_and_levers",
        Reward::Display => "reward_display",
        Reward::ConstantSignal => "reward_constant_signal",
        Reward::LogicGates => "reward_logic_gates",
        Reward::VirtualProcessing => "reward_virtual_processing",
        Reward::Filter => "reward_filter",
        Reward::Blueprints => "reward_blueprints",
        Reward::Freeplay => "reward_freeplay",
        Reward::None => "no_reward",
        Reward::NoRewardFreeplay => "no_reward_freeplay",
    };
    t(&format!("storyRewards.{id}.title"))
}

/// Nom traduit d'une famille d'améliorations (`shopUpgrades.<id>.name`).
pub fn upgrade_name(kind: UpgradeKind) -> String {
    t(&format!("shopUpgrades.{}.name", kind.js_id()))
}

/// Nombre décimal avec le séparateur de la langue (`global.decimalSeparator`).
pub fn decimal(value: f64, digits: usize) -> String {
    let text = format!("{value:.digits$}");
    let separator = t("global.decimalSeparator");
    if separator == "global.decimalSeparator" {
        text
    } else {
        text.replace('.', &separator)
    }
}

/// `formatBigNumber` : `999`, `1,3k`, `5M`… selon la langue.
pub fn format_big_number(n: u32) -> String {
    if n < 1000 {
        return n.to_string();
    }
    let mut lead = f64::from(n);
    let mut suffix = "";
    for s in ["thousands", "millions", "billions", "trillions"] {
        lead /= 1000.0;
        suffix = s;
        if lead < 1000.0 {
            break;
        }
    }
    let rounded = (lead * 10.0).round() / 10.0;
    let digits = if rounded.fract() == 0.0 { 0 } else { 1 };
    format!(
        "{}{}",
        decimal(rounded, digits),
        t(&format!("global.suffix.{suffix}"))
    )
}

/// Charge la langue détectée et remplace la police par défaut par celle du jeu.
#[derive(Debug)]
pub struct I18nPlugin {
    /// Dossier `assets/` (celui de l'`AssetPlugin`).
    pub asset_dir: String,
}

impl Plugin for I18nPlugin {
    fn build(&self, app: &mut App) {
        let dir = Path::new(&self.asset_dir);
        let language = detect_language();
        info!("langue : {} ({})", language.name, language.id);
        set_translations(Translations::load(&dir.join("i18n"), language));

        // Police du jeu (Barlow), convertie en TTF par `cargo xtask import-assets`.
        let font = dir.join("fonts/GameFont.ttf");
        match std::fs::read(&font) {
            Ok(bytes) => {
                if let Some(mut fonts) = app.world_mut().get_resource_mut::<Assets<Font>>() {
                    let _ = fonts.insert(AssetId::default(), Font::from_bytes(bytes));
                }
            }
            Err(e) => warn!(
                "police du jeu introuvable ({}) : {e} ; relancez `cargo xtask import-assets`",
                font.display()
            ),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Textes de test : toujours les mêmes, donc sans conflit entre tests parallèles.
    pub(crate) fn use_test_translations() {
        let mut tr = Translations::default();
        tr.merge_yaml(
            "global:\n    decimalSeparator: \",\"\n    suffix:\n        thousands: k\n        millions: M\n",
        );
        tr.merge_yaml(PORT_FILES[1].1);
        set_translations(tr);
    }

    #[test]
    fn big_numbers_use_the_language_suffixes() {
        use_test_translations();
        assert_eq!(format_big_number(999), "999");
        assert_eq!(format_big_number(1000), "1k");
        assert_eq!(format_big_number(1250), "1,3k");
        assert_eq!(format_big_number(5000), "5k");
        assert_eq!(format_big_number(2_500_000), "2,5M");
    }

    #[test]
    fn rendering_replaces_parameters_and_strips_markup() {
        assert_eq!(render("Level <level>", &[("level", "3")]), "Level 3");
        assert_eq!(
            render("This is <strong>NOT</strong> idle!<br><br>Tip", &[]),
            "This is NOT idle!\n\nTip"
        );
        assert_eq!(render("<span class='x'>a</span> < b > c", &[]), "a < b > c");
        assert_eq!(render("Unlocked <reward>!", &[]), "Unlocked !");
    }

    #[test]
    fn system_locales_map_to_game_languages() {
        let id = |code| language_for_code(code).map(|l| l.id);
        assert_eq!(id("fr_FR.UTF-8"), Some("fr"));
        assert_eq!(id("de-DE"), Some("de"));
        assert_eq!(id("pt_BR"), Some("pt-BR"));
        assert_eq!(id("pt_PT"), Some("pt-PT"));
        assert_eq!(id("es_MX"), Some("es-419"));
        assert_eq!(id("ko_KR"), Some("kor"));
        assert_eq!(id("C"), None);
        assert_eq!(id(""), None);
    }

    #[test]
    fn missing_keys_fall_back_to_english_then_to_the_key() {
        let mut tr = Translations::default();
        tr.merge_yaml("mainMenu:\n    play: Play\n    newGame: New Game\n");
        tr.merge_yaml("mainMenu:\n    play: Jouer\n");
        assert_eq!(tr.get("mainMenu.play", &[]), "Jouer");
        assert_eq!(tr.get("mainMenu.newGame", &[]), "New Game");
        assert_eq!(tr.get("nope.nope", &[]), "nope.nope");
    }

    #[test]
    fn port_files_parse_and_share_their_keys() {
        let keys = |text: &str| {
            let mut tr = Translations::default();
            tr.merge_yaml(text);
            let mut keys: Vec<_> = tr.texts.into_keys().collect();
            keys.sort();
            keys
        };
        let en = keys(PORT_FILES[0].1);
        assert!(!en.is_empty());
        for (id, text) in PORT_FILES {
            assert_eq!(keys(text), en, "clés de port-{id}.yaml");
        }
    }

    /// Les fichiers du jeu d'origine, quand ils ont été importés.
    #[test]
    fn game_files_provide_every_key_used_by_the_port() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/i18n");
        if !dir.join("base-en.yaml").exists() {
            eprintln!("assets non importés : test ignoré");
            return;
        }
        for language in LANGUAGES {
            let tr = Translations::load(&dir, language);
            assert!(!tr.get("mainMenu.play", &[]).is_empty());
            for kind in BuildingKind::ALL
                .into_iter()
                .filter(|&k| k != BuildingKind::Hub)
            {
                for &variant in kind.variants() {
                    let key = format!(
                        "buildings.{}.{}.name",
                        kind.js_id(),
                        variant_key(kind, variant)
                    );
                    assert!(tr.raw(&key).is_some(), "{} : {key}", language.id);
                }
            }
        }
    }
}
