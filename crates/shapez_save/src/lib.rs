//! Sauvegardes natives (plan, §8).
//!
//! Disposition d'un fichier :
//!
//! | Octets | Contenu |
//! |---|---|
//! | 8 | [`MAGIC`] |
//! | 4 | version du format (`u32` petit-boutiste) |
//! | 4 | longueur `n` des métadonnées |
//! | n | [`SaveMeta`] en JSON, lisible sans décompresser le reste |
//! | … | état du monde : JSON compressé en zstd |
//! | 4 | CRC32 de tout ce qui précède |
//!
//! L'état est du JSON pour que les migrations travaillent sur un arbre, comme les
//! `savegame_interface_10xx.js` du jeu d'origine, sans garder d'anciens types Rust.
//! Le CRC détecte les fichiers tronqués ou corrompus : le chargeur renvoie alors une
//! erreur au lieu de paniquer.

pub mod migrations;
pub mod storage;

use serde::{Deserialize, Serialize};
use shapez_sim::SimWorld;

/// Signature des fichiers de sauvegarde.
pub const MAGIC: &[u8; 8] = b"SHAPEZRS";
/// Version courante du format de l'état ; chaque changement de structure de la
/// simulation l'incrémente et ajoute une migration.
pub const FORMAT_VERSION: u32 = 1;
/// Niveau de compression zstd : rapide, et déjà très efficace sur du JSON répétitif.
const ZSTD_LEVEL: i32 = 3;
/// Plafond de la taille décompressée, contre les bombes de décompression.
const MAX_STATE_BYTES: usize = 1 << 30;

/// Résumé affiché dans le menu des sauvegardes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMeta {
    /// Version du jeu qui a écrit le fichier.
    pub game_version: String,
    pub seed: u32,
    pub level: u32,
    /// Durée de jeu, en ticks.
    pub ticks: u64,
    /// Date d'écriture, en secondes Unix.
    pub saved_at: u64,
}

impl SaveMeta {
    /// Métadonnées du monde, datées de maintenant.
    pub fn of(world: &SimWorld) -> Self {
        Self {
            game_version: env!("CARGO_PKG_VERSION").to_owned(),
            seed: world.map.seed(),
            level: world.hub.level,
            ticks: world.tick_count(),
            saved_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        }
    }
}

/// Échec de lecture ou d'écriture d'une sauvegarde.
#[derive(Debug)]
pub enum SaveError {
    /// Pas un fichier de sauvegarde (signature absente).
    NotASave,
    /// Fichier tronqué, ou CRC incorrect.
    Corrupted,
    /// Écrit par une version plus récente du jeu.
    TooNew {
        version: u32,
    },
    /// JSON invalide ou ne correspondant pas au schéma.
    Format(serde_json::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotASave => f.write_str("ce fichier n'est pas une sauvegarde"),
            Self::Corrupted => f.write_str("sauvegarde corrompue"),
            Self::TooNew { version } => write!(
                f,
                "sauvegarde au format {version}, plus récent que ce jeu ({FORMAT_VERSION})"
            ),
            Self::Format(e) => write!(f, "contenu illisible : {e}"),
            Self::Io(e) => write!(f, "erreur d'entrée-sortie : {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<serde_json::Error> for SaveError {
    fn from(e: serde_json::Error) -> Self {
        Self::Format(e)
    }
}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Encode le monde au format courant.
pub fn encode(world: &SimWorld, meta: &SaveMeta) -> Result<Vec<u8>, SaveError> {
    let meta_json = serde_json::to_vec(meta)?;
    let state = zstd::bulk::compress(&serde_json::to_vec(world)?, ZSTD_LEVEL)?;
    let mut out = Vec::with_capacity(20 + meta_json.len() + state.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(meta_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&meta_json);
    out.extend_from_slice(&state);
    let crc = crc32fast::hash(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

/// Parties d'un fichier dont le CRC est vérifié.
struct Parts<'a> {
    version: u32,
    meta: &'a [u8],
    state: &'a [u8],
}

fn split(bytes: &[u8]) -> Result<Parts<'_>, SaveError> {
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
        return Err(SaveError::NotASave);
    }
    if bytes.len() < 20 {
        return Err(SaveError::Corrupted);
    }
    let (body, crc) = bytes.split_at(bytes.len() - 4);
    if crc32fast::hash(body) != u32::from_le_bytes(crc.try_into().expect("4 octets")) {
        return Err(SaveError::Corrupted);
    }
    let u32_at = |i: usize| u32::from_le_bytes(body[i..i + 4].try_into().expect("4 octets"));
    let version = u32_at(8);
    let meta_len = u32_at(12) as usize;
    let rest = &body[16..];
    if meta_len > rest.len() {
        return Err(SaveError::Corrupted);
    }
    let (meta, state) = rest.split_at(meta_len);
    Ok(Parts {
        version,
        meta,
        state,
    })
}

/// Lit seulement les métadonnées (menu des sauvegardes).
pub fn read_meta(bytes: &[u8]) -> Result<SaveMeta, SaveError> {
    Ok(serde_json::from_slice(split(bytes)?.meta)?)
}

/// Décode un fichier, en migrant l'état si besoin.
pub fn decode(bytes: &[u8]) -> Result<(SaveMeta, SimWorld), SaveError> {
    let parts = split(bytes)?;
    if parts.version > FORMAT_VERSION {
        return Err(SaveError::TooNew {
            version: parts.version,
        });
    }
    let meta = serde_json::from_slice(parts.meta)?;
    let json =
        zstd::bulk::decompress(parts.state, MAX_STATE_BYTES).map_err(|_| SaveError::Corrupted)?;
    let mut state: serde_json::Value = serde_json::from_slice(&json)?;
    migrations::migrate(parts.version, &mut state)?;
    let mut world: SimWorld = serde_json::from_value(state)?;
    world.announce_buildings();
    Ok((meta, world))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_foreign_and_damaged_files() {
        let world = SimWorld::new_game(7);
        let bytes = encode(&world, &SaveMeta::of(&world)).unwrap();
        assert!(matches!(decode(b"hello"), Err(SaveError::NotASave)));
        assert!(matches!(
            decode(&bytes[..bytes.len() - 1]),
            Err(SaveError::Corrupted)
        ));
        // Chaque octet modifié est détecté par le CRC.
        for i in (8..bytes.len()).step_by(7) {
            let mut damaged = bytes.clone();
            damaged[i] ^= 0x5a;
            assert!(decode(&damaged).is_err(), "octet {i}");
        }
        let meta = read_meta(&bytes).unwrap();
        assert_eq!((meta.seed, meta.level), (7, 1));
    }

    #[test]
    fn refuses_saves_from_the_future() {
        let world = SimWorld::new_game(1);
        let mut bytes = encode(&world, &SaveMeta::of(&world)).unwrap();
        bytes[8..12].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        let len = bytes.len();
        let crc = crc32fast::hash(&bytes[..len - 4]);
        bytes[len - 4..].copy_from_slice(&crc.to_le_bytes());
        assert!(matches!(decode(&bytes), Err(SaveError::TooNew { .. })));
    }
}
