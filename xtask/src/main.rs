//! Tâches de développement du workspace.
//!
//! ```text
//! cargo xtask import-assets [--music]
//! ```

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

/// Ce que l'on importe depuis le jeu JS : (source relative à la racine du jeu d'origine,
/// destination relative à `rust/assets`).
const ASSETS: &[(&str, &str)] = &[
    ("res_raw/sprites", "sprites"),
    ("res_raw/sounds/sfx", "sounds/sfx"),
    ("res/fonts", "fonts"),
    ("translations", "i18n"),
];

/// Les musiques pèsent ~185 Mo : importées seulement sur demande.
const MUSIC: (&str, &str) = ("res_raw/sounds/music", "sounds/music");

/// Extensions copiées ; le reste (scripts Python, fichiers de travail) est ignoré.
const EXTENSIONS: &[&str] = &["png", "wav", "mp3", "ogg", "ttf", "woff", "woff2", "yaml"];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("import-assets") => import_assets(args.iter().any(|a| a == "--music")),
        _ => {
            eprintln!("usage : cargo xtask import-assets [--music]");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("erreur : {err}");
            ExitCode::FAILURE
        }
    }
}

/// Racine du workspace Rust (`rust/`).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask est dans le workspace")
        .to_path_buf()
}

/// Clone du jeu d'origine : `SHAPEZ_JS_ROOT`, sinon `../shapez.io` à côté du workspace.
fn original_repo(workspace: &Path) -> io::Result<PathBuf> {
    let repo = env::var_os("SHAPEZ_JS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("../shapez.io"));
    if repo.join("res_raw").is_dir() {
        Ok(repo)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "jeu d'origine introuvable dans {} : clonez https://github.com/Flyns157/shapez.io \
                 à côté de ce dépôt, ou définissez SHAPEZ_JS_ROOT",
                repo.display()
            ),
        ))
    }
}

fn import_assets(with_music: bool) -> io::Result<()> {
    let workspace = workspace_root();
    let repo = original_repo(&workspace)?;
    let assets = workspace.join("assets");

    let mut list: Vec<(&str, &str)> = ASSETS.to_vec();
    if with_music {
        list.push(MUSIC);
    }

    let mut manifest = String::from(
        "# Généré par `cargo xtask import-assets`. Assets repris de shapez.io (GPL-3.0),\n\
         # voir NOTICE. Source -> destination.\n",
    );
    for (src, dst) in list {
        let (from, to) = (repo.join(src), assets.join(dst));
        fs::create_dir_all(&assets)?;
        if to.exists() {
            fs::remove_dir_all(&to)?;
        }
        let count = copy_tree(&from, &to)?;
        println!("{src} -> assets/{dst} ({count} fichiers)");
        manifest.push_str(&format!("{src} -> assets/{dst} ({count} fichiers)\n"));
    }
    for font in convert_fonts(&assets.join("fonts"))? {
        println!("{font}");
        manifest.push_str(&format!("{font}\n"));
    }
    fs::write(assets.join("IMPORTED.txt"), manifest)
}

/// Bevy ne lit pas le WOFF2 : chaque police `.woff2` est décompressée en `.ttf` à côté.
fn convert_fonts(dir: &Path) -> io::Result<Vec<String>> {
    let mut done = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("woff2") {
            continue;
        }
        let data = fs::read(&path)?;
        let ttf = woff2_patched::convert_woff2_to_ttf(&mut data.as_slice())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;
        let target = path.with_extension("ttf");
        fs::write(&target, ttf)?;
        done.push(format!(
            "{} -> {} (conversion WOFF2 -> TTF)",
            path.file_name().unwrap_or_default().to_string_lossy(),
            target.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    done.sort();
    Ok(done)
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<usize> {
    let mut count = 0;
    let mut entries: Vec<_> = fs::read_dir(from)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            count += copy_tree(&path, &target)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        {
            fs::create_dir_all(to)?;
            fs::copy(&path, &target)?;
            count += 1;
        }
    }
    Ok(count)
}
