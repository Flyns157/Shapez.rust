//! Fichiers persistants (sauvegardes, paramètres) : le disque, ou le `localStorage` du
//! navigateur dans la version web, où chaque chemin devient une clé `shapez-rs:<chemin>`
//! et le contenu est encodé en base64.

use std::{
    io,
    path::{Path, PathBuf},
};

pub use imp::{exists, list, read, remove, write_atomic};

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::{fs, io::Write};

    use super::*;

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    /// Écrit dans un fichier temporaire, synchronise, puis renomme : un plantage en
    /// cours d'écriture laisse l'ancien fichier intact.
    pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
        let dir = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let tmp = dir.join(format!(".{name}.tmp"));
        {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, path)
    }

    pub fn remove(path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    /// Fichiers directement dans `dir` ; vide si le dossier n'existe pas.
    pub fn list(dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .collect()
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use web_sys::Storage;

    use super::*;

    const PREFIX: &str = "shapez-rs:";

    fn storage() -> io::Result<Storage> {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "localStorage indisponible"))
    }

    fn key(path: &Path) -> String {
        format!("{PREFIX}{}", path.to_string_lossy().replace('\\', "/"))
    }

    fn js_error(e: wasm_bindgen::JsValue) -> io::Error {
        // Le plus souvent `QuotaExceededError` : le navigateur limite le stockage (~5 Mo).
        io::Error::other(format!("localStorage : {e:?}"))
    }

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        let text = storage()?
            .get_item(&key(path))
            .map_err(js_error)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, key(path)))?;
        STANDARD
            .decode(text)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn exists(path: &Path) -> bool {
        storage()
            .ok()
            .and_then(|s| s.get_item(&key(path)).ok().flatten())
            .is_some()
    }

    /// `setItem` remplace la valeur d'un coup : l'écriture est déjà atomique.
    pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
        storage()?
            .set_item(&key(path), &STANDARD.encode(bytes))
            .map_err(js_error)
    }

    pub fn remove(path: &Path) -> io::Result<()> {
        storage()?.remove_item(&key(path)).map_err(js_error)
    }

    pub fn list(dir: &Path) -> Vec<PathBuf> {
        let Ok(storage) = storage() else {
            return Vec::new();
        };
        let prefix = format!("{}/", key(dir));
        let len = storage.length().unwrap_or(0);
        (0..len)
            .filter_map(|i| storage.key(i).ok().flatten())
            .filter_map(|k| {
                let rest = k.strip_prefix(&prefix)?;
                (!rest.contains('/')).then(|| dir.join(rest))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_list_remove() {
        let dir = std::env::temp_dir().join(format!("shapez-disk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list(&dir).is_empty());
        let file = dir.join("a.bin");
        write_atomic(&file, b"un").unwrap();
        write_atomic(&file, b"deux").unwrap();
        assert!(exists(&file));
        assert_eq!(read(&file).unwrap(), b"deux");
        // Pas de fichier temporaire laissé derrière.
        assert_eq!(list(&dir), vec![file.clone()]);
        remove(&file).unwrap();
        assert!(!exists(&file));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
