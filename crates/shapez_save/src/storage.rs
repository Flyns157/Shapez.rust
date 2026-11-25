//! Fichiers de sauvegarde sur disque : un fichier `<nom>.shapezsave` par partie dans
//! le dossier de données de l'utilisateur, écrit de façon atomique.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{SaveError, SaveMeta, read_meta};

/// Extension des fichiers de sauvegarde.
pub const EXTENSION: &str = "shapezsave";

/// Une sauvegarde trouvée sur disque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveEntry {
    /// Nom sans extension.
    pub name: String,
    pub path: PathBuf,
    /// `Err` si le fichier est illisible : le menu peut encore proposer de le supprimer.
    pub meta: Result<SaveMeta, String>,
}

/// Dossier des sauvegardes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveDir {
    root: PathBuf,
}

impl SaveDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Dossier par défaut : `SHAPEZ_SAVE_DIR`, sinon le dossier de données de la
    /// plateforme (`~/.local/share/shapez-rs/saves` sous Linux).
    pub fn default_location() -> Option<Self> {
        if let Some(dir) = std::env::var_os("SHAPEZ_SAVE_DIR") {
            return Some(Self::new(dir));
        }
        directories::ProjectDirs::from("", "", "shapez-rs")
            .map(|dirs| Self::new(dirs.data_dir().join("saves")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path_of(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.{EXTENSION}"))
    }

    /// Sauvegardes, de la plus récente à la plus ancienne.
    pub fn list(&self) -> Vec<SaveEntry> {
        let Ok(dir) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut entries: Vec<SaveEntry> = dir
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == EXTENSION))
            .filter_map(|path| {
                let name = path.file_stem()?.to_str()?.to_owned();
                let meta = fs::read(&path)
                    .map_err(SaveError::from)
                    .and_then(|bytes| read_meta(&bytes))
                    .map_err(|e| e.to_string());
                Some(SaveEntry { name, path, meta })
            })
            .collect();
        entries.sort_by(|a, b| {
            let time = |e: &SaveEntry| e.meta.as_ref().map_or(0, |m| m.saved_at);
            time(b).cmp(&time(a)).then_with(|| a.name.cmp(&b.name))
        });
        entries
    }

    /// Premier nom libre de la forme `partie-N`.
    pub fn fresh_name(&self) -> String {
        (1..)
            .map(|n| format!("partie-{n}"))
            .find(|name| !self.path_of(name).exists())
            .expect("un nom libre finit par exister")
    }

    /// Écrit dans un fichier temporaire, synchronise, puis renomme : un plantage en
    /// cours d'écriture laisse l'ancienne sauvegarde intacte.
    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), SaveError> {
        fs::create_dir_all(&self.root)?;
        let target = self.path_of(name);
        let tmp = self.root.join(format!(".{name}.{EXTENSION}.tmp"));
        {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &target)?;
        Ok(())
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, SaveError> {
        Ok(fs::read(self.path_of(name))?)
    }

    pub fn delete(&self, name: &str) -> Result<(), SaveError> {
        Ok(fs::remove_file(self.path_of(name))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode;
    use shapez_sim::SimWorld;

    fn temp_dir(tag: &str) -> SaveDir {
        let dir = std::env::temp_dir().join(format!("shapez-save-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        SaveDir::new(dir)
    }

    #[test]
    fn write_list_read_delete() {
        let dir = temp_dir("storage");
        assert!(dir.list().is_empty());
        let world = SimWorld::new_game(42);
        let bytes = encode(&world, &SaveMeta::of(&world)).unwrap();
        let name = dir.fresh_name();
        assert_eq!(name, "partie-1");
        dir.write(&name, &bytes).unwrap();
        // Un fichier étranger est listé, avec une erreur.
        fs::write(dir.path_of("abime"), b"rien").unwrap();
        let list = dir.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "partie-1");
        assert_eq!(list[0].meta.as_ref().unwrap().seed, 42);
        assert!(list[1].meta.is_err());
        assert_eq!(dir.fresh_name(), "partie-2");
        assert_eq!(dir.read(&name).unwrap(), bytes);
        dir.delete(&name).unwrap();
        dir.delete("abime").unwrap();
        assert!(dir.list().is_empty());
        let _ = fs::remove_dir_all(dir.root());
    }
}
