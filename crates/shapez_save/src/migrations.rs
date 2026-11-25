//! Migrations de l'état entre versions du format, sur le modèle des
//! `savegame_interface_10xx.js` : chaque étape transforme l'arbre JSON en place.

use serde_json::Value;

use crate::{FORMAT_VERSION, SaveError};

/// Transforme un état d'une version à la suivante.
type Migration = fn(&mut Value) -> Result<(), SaveError>;

/// `MIGRATIONS[i]` fait passer un état de la version `i + 1` à la version `i + 2`.
/// La version 1 est la première : aucune migration pour l'instant.
const MIGRATIONS: &[Migration] = &[];

const _: () = assert!(MIGRATIONS.len() as u32 + 1 == FORMAT_VERSION);

/// Amène un état de la version `from` à [`FORMAT_VERSION`].
pub fn migrate(from: u32, state: &mut Value) -> Result<(), SaveError> {
    if from == 0 {
        return Err(SaveError::Corrupted);
    }
    for step in &MIGRATIONS[(from - 1) as usize..] {
        step(state)?;
    }
    Ok(())
}
