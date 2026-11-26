//! Lecture des fichiers d'assets dont le jeu a besoin tout de suite, avant que
//! l'`AssetServer` ne tourne : traductions et polices.
//!
//! Sur le web, il n'y a pas de système de fichiers : [`preload`] les télécharge avec
//! `fetch` avant de démarrer Bevy, puis [`read_asset`] les sert depuis la mémoire.

use std::{io, path::Path};

/// Contenu d'un fichier d'assets (chemin complet, `asset_dir()` compris).
pub fn read_asset(path: &Path) -> io::Result<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(path)
    }
    #[cfg(target_arch = "wasm32")]
    {
        web::read(path)
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::preload;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::{
        collections::HashMap,
        io,
        path::{Path, PathBuf},
        sync::{OnceLock, RwLock},
    };

    use bevy::log::warn;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    fn cache() -> &'static RwLock<HashMap<PathBuf, Vec<u8>>> {
        static CACHE: OnceLock<RwLock<HashMap<PathBuf, Vec<u8>>>> = OnceLock::new();
        CACHE.get_or_init(Default::default)
    }

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        cache()
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{} non préchargé", path.display()),
                )
            })
    }

    /// Télécharge `paths` (relatifs à la page), l'un après l'autre : il n'y en a que
    /// quatre au plus. Un fichier introuvable est simplement absent du cache : [`read`]
    /// renverra alors `NotFound`, comme sur disque.
    pub async fn preload(paths: Vec<PathBuf>) {
        for path in paths {
            match fetch(&path).await {
                Ok(bytes) => {
                    cache()
                        .write()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(path, bytes);
                }
                Err(e) => warn!("{} : {e}", path.display()),
            }
        }
    }

    async fn fetch(path: &Path) -> Result<Vec<u8>, String> {
        let window = web_sys::window().ok_or("pas de fenêtre")?;
        let url = path.to_string_lossy().replace('\\', "/");
        let response: web_sys::Response = JsFuture::from(window.fetch_with_str(&url))
            .await
            .map_err(|e| format!("{e:?}"))?
            .dyn_into()
            .map_err(|e| format!("{e:?}"))?;
        if !response.ok() {
            return Err(format!("HTTP {}", response.status()));
        }
        let buffer = JsFuture::from(response.array_buffer().map_err(|e| format!("{e:?}"))?)
            .await
            .map_err(|e| format!("{e:?}"))?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }
}
