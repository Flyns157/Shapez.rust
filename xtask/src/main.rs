//! Tâches de développement du workspace.
//!
//! ```text
//! cargo xtask import-assets [--music] [--fonts]
//! cargo xtask dist [--target <triplet>] [--music]
//! cargo xtask web [--music]
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
        Some("import-assets") => import_assets(
            args.iter().any(|a| a == "--music"),
            args.iter().any(|a| a == "--fonts"),
        ),
        Some("dist") => dist(
            args.windows(2)
                .find(|w| w[0] == "--target")
                .map(|w| w[1].as_str()),
            args.iter().any(|a| a == "--music"),
        ),
        Some("web") => web(args.iter().any(|a| a == "--music")),
        _ => {
            eprintln!(
                "usage : cargo xtask import-assets [--music] [--fonts]\n        \
                 cargo xtask dist [--target <triplet>] [--music]\n        \
                 cargo xtask web [--music]"
            );
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

fn import_assets(with_music: bool, with_fonts: bool) -> io::Result<()> {
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
    if with_fonts {
        for line in download_fonts(&assets.join("fonts-noto"))? {
            println!("{line}");
            manifest.push_str(&format!("{line}\n"));
        }
    }
    fs::write(assets.join("IMPORTED.txt"), manifest)
}

/// Polices Noto (SIL Open Font License 1.1) des langues que la police du jeu ne couvre
/// pas : (URL, fichier dans `assets/fonts-noto`, hors de `assets/fonts` que chaque import remplace).
const NOTO_FONTS: &[(&str, &str)] = &[
    (
        "https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/NotoSans/hinted/ttf/NotoSans-Regular.ttf",
        "NotoSans-Regular.ttf",
    ),
    (
        "https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/NotoSansHebrew/hinted/ttf/NotoSansHebrew-Regular.ttf",
        "NotoSansHebrew-Regular.ttf",
    ),
    (
        "https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/LICENSE",
        "LICENSE-NotoSans.txt",
    ),
    (
        "https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/SubsetOTF/SC/NotoSansSC-Regular.otf",
        "NotoSansSC-Regular.otf",
    ),
    (
        "https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/SubsetOTF/TC/NotoSansTC-Regular.otf",
        "NotoSansTC-Regular.otf",
    ),
    (
        "https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/SubsetOTF/JP/NotoSansJP-Regular.otf",
        "NotoSansJP-Regular.otf",
    ),
    (
        "https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/SubsetOTF/KR/NotoSansKR-Regular.otf",
        "NotoSansKR-Regular.otf",
    ),
    (
        "https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/LICENSE",
        "LICENSE-NotoSansCJK.txt",
    ),
];

/// Télécharge les polices Noto avec `curl` (fourni par Linux, macOS et Windows 10+).
/// Un fichier déjà présent n'est pas retéléchargé.
fn download_fonts(dir: &Path) -> io::Result<Vec<String>> {
    fs::create_dir_all(dir)?;
    let mut done = Vec::new();
    for (url, file) in NOTO_FONTS {
        let target = dir.join(file);
        if target.exists() {
            done.push(format!("{file} : déjà présent"));
            continue;
        }
        let partial = dir.join(format!("{file}.part"));
        let status = std::process::Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(&partial)
            .arg(url)
            .status()
            .map_err(|e| io::Error::new(e.kind(), format!("curl introuvable : {e}")))?;
        if !status.success() {
            let _ = fs::remove_file(&partial);
            return Err(io::Error::other(format!(
                "téléchargement de {url} impossible ({status})"
            )));
        }
        fs::rename(&partial, &target)?;
        done.push(format!("{url} -> assets/fonts-noto/{file}"));
    }
    Ok(done)
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

/// Nom des archives distribuées. Le plan (§2) demande d'éviter « shapez » pour une
/// diffusion publique : à changer ici le jour où un nom définitif est choisi.
const DIST_NAME: &str = "shapez-rs";

/// Compile le jeu avec le profil `dist` et prépare `target/packages/<nom>-<version>-<triplet>/` :
/// l'exécutable, `assets/` (que le jeu cherche à côté de lui), LICENSE, NOTICE et README.
/// La musique n'est copiée qu'avec `--music`, ses droits n'étant pas vérifiés (NOTICE).
/// Le dossier est ensuite archivé par la CI (`.github/workflows/release.yml`).
fn dist(target: Option<&str>, with_music: bool) -> io::Result<()> {
    let workspace = workspace_root();
    check_assets(&workspace)?;
    let triple = match target {
        Some(t) => t.to_string(),
        None => host_triple()?,
    };
    build_game(&workspace, "dist", target)?;

    let release = match target {
        Some(t) => workspace.join("target").join(t).join("dist"),
        None => workspace.join("target/dist"),
    };
    let (exe, exe_out) = if triple.contains("windows") {
        ("shapez_game.exe", format!("{DIST_NAME}.exe"))
    } else {
        ("shapez_game", DIST_NAME.to_string())
    };
    let (out, count) = package_dir(&workspace, &triple, with_music)?;
    fs::copy(release.join(exe), out.join(&exe_out))?;
    println!(
        "{} : {exe_out} et {count} fichiers d'assets{}",
        out.display(),
        if with_music { "" } else { " (sans la musique)" }
    );
    Ok(())
}

/// Version web : compile pour `wasm32-unknown-unknown` avec le profil `dist`, génère le
/// JavaScript avec `wasm-bindgen` (à installer dans la version du `Cargo.lock`), et
/// prépare `target/packages/<nom>-<version>-web/` avec `web/index.html` et `assets/`.
/// Le dossier se sert tel quel par n'importe quel serveur HTTP statique.
fn web(with_music: bool) -> io::Result<()> {
    let workspace = workspace_root();
    check_assets(&workspace)?;
    let target = "wasm32-unknown-unknown";
    // Profil `dist` : wasm-bindgen avertit que `strip` retire la section
    // `target_features`, mais le résultat fonctionne et pèse 51 Mo au lieu de 82.
    build_game(&workspace, "dist", Some(target))?;

    let (out, count) = package_dir(&workspace, "web", with_music)?;
    let wasm = workspace
        .join("target")
        .join(target)
        .join("dist/shapez_game.wasm");
    let status = std::process::Command::new("wasm-bindgen")
        .args([
            "--target",
            "web",
            "--no-typescript",
            "--out-name",
            "shapez",
            "--out-dir",
        ])
        .arg(&out)
        .arg(&wasm)
        .status()
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "wasm-bindgen introuvable ({e}) : `cargo install wasm-bindgen-cli \
                     --version <version de wasm-bindgen dans Cargo.lock>`"
                ),
            )
        })?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "échec de wasm-bindgen ({status})"
        )));
    }
    fs::copy(workspace.join("web/index.html"), out.join("index.html"))?;
    println!(
        "{} : index.html, shapez.js, shapez_bg.wasm et {count} fichiers d'assets{}",
        out.display(),
        if with_music { "" } else { " (sans la musique)" }
    );
    Ok(())
}

fn check_assets(workspace: &Path) -> io::Result<()> {
    let assets = workspace.join("assets");
    if assets.join("sprites").is_dir() && assets.join("i18n").is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "assets absents : lancez d'abord `cargo xtask import-assets --fonts`",
        ))
    }
}

/// `cargo build --profile <profile> -p shapez_game [--target <triplet>]`.
fn build_game(workspace: &Path, profile: &str, target: Option<&str>) -> io::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut build = std::process::Command::new(cargo);
    build
        .current_dir(workspace)
        .args(["build", "--profile", profile, "-p", "shapez_game"]);
    if let Some(t) = target {
        build.args(["--target", t]);
    }
    let status = build.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "échec de la compilation ({status})"
        )))
    }
}

/// Crée (vide) `target/packages/<nom>-<version>-<suffixe>/` avec LICENSE, NOTICE, README
/// et `assets/`. Renvoie le dossier et le nombre de fichiers d'assets copiés.
fn package_dir(workspace: &Path, suffix: &str, with_music: bool) -> io::Result<(PathBuf, usize)> {
    let out = workspace.join("target/packages").join(format!(
        "{DIST_NAME}-{}-{suffix}",
        env!("CARGO_PKG_VERSION")
    ));
    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    fs::create_dir_all(out.join("assets"))?;
    for file in ["LICENSE", "NOTICE", "README.md"] {
        fs::copy(workspace.join(file), out.join(file))?;
    }
    let mut count = 0;
    for entry in fs::read_dir(workspace.join("assets"))? {
        let entry = entry?;
        let target = out.join("assets").join(entry.file_name());
        if entry.file_type()?.is_dir() {
            count += copy_dist_tree(&entry.path(), &target, with_music)?;
        } else {
            fs::copy(entry.path(), target)?;
            count += 1;
        }
    }
    Ok((out, count))
}

/// Copie récursive de tout un dossier, sauf `sounds/music` si `with_music` est faux.
fn copy_dist_tree(from: &Path, to: &Path, with_music: bool) -> io::Result<usize> {
    if !with_music && from.ends_with("sounds/music") {
        return Ok(0);
    }
    fs::create_dir_all(to)?;
    let mut count = 0;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            count += copy_dist_tree(&entry.path(), &target, with_music)?;
        } else {
            fs::copy(entry.path(), target)?;
            count += 1;
        }
    }
    Ok(count)
}

/// Triplet de la machine, lu dans `rustc -vV`.
fn host_triple() -> io::Result<String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = std::process::Command::new(rustc).arg("-vV").output()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(str::to_string)
        .ok_or_else(|| io::Error::other("triplet de la machine introuvable dans `rustc -vV`"))
}
