# shapez-rs

shapez-rs est une réécriture en Rust, avec le moteur [Bevy](https://bevy.org) 0.20, du jeu
d'automatisation [shapez.io](https://github.com/tobspr-games/shapez.io) (shapez 1). La
simulation est portée depuis le code JavaScript d'origine et vérifiée contre lui. Le plan
complet du projet est dans [`docs/plan-rust-bevy.md`](docs/plan-rust-bevy.md).

## Sommaire

- [Démarrer](#démarrer)
  - [1. Installer Rust](#1-installer-rust)
  - [2. Installer les dépendances système](#2-installer-les-dépendances-système)
  - [3. Récupérer le code](#3-récupérer-le-code)
  - [4. Importer les assets du jeu d'origine](#4-importer-les-assets-du-jeu-dorigine)
  - [5. Lancer le jeu](#5-lancer-le-jeu)
    - [Raccourcis de construction](#raccourcis-de-construction)
  - [6. Lancer les tests](#6-lancer-les-tests)
- [Distribuer le jeu](#distribuer-le-jeu)
  - [Version web](#version-web)
- [Compiler pour Windows depuis Linux](#compiler-pour-windows-depuis-linux)
- [Variables d'environnement](#variables-denvironnement)
- [Sauvegardes](#sauvegardes)
- [Paramètres](#paramètres)
- [Organisation du code](#organisation-du-code)
- [Oracle JS](#oracle-js)
- [Licence](#licence)

## Démarrer

En résumé, il faut : Rust via `rustup`, les dépendances système de Bevy pour votre
plateforme, ce dépôt, et un clone du **code source** de shapez.io pour en copier les
images, sons et traductions. La première compilation prend plusieurs minutes, car Bevy
est volumineux ; les suivantes sont rapides.

### 1. Installer Rust

Installez [rustup](https://rustup.rs). Vous n'avez pas besoin de choisir une version de
Rust : le fichier [`rust-toolchain.toml`](rust-toolchain.toml) fixe la version utilisée
(1.97.1, avec `rustfmt` et `clippy`), et `rustup` la télécharge automatiquement à la
première commande `cargo` lancée dans le dépôt.

Linux et macOS :

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Windows : téléchargez et lancez `rustup-init.exe` depuis [rustup.rs](https://rustup.rs),
ou utilisez winget :

```powershell
winget install --id Rustlang.Rustup
```

Sous Windows, lisez d'abord la section [Windows](#windows) ci-dessous : il faut choisir
entre les outils MSVC et GNU avant de compiler.

### 2. Installer les dépendances système

#### Linux

Bevy a besoin d'un compilateur C, de `pkg-config`, des en-têtes ALSA (son), udev
(manettes) et Wayland/xkbcommon (fenêtre et clavier). Il faut aussi un pilote graphique
compatible Vulkan, déjà présent sur la plupart des bureaux (paquet Mesa).

Debian, Ubuntu et dérivées (ce sont les paquets installés par la CI) :

```sh
sudo apt install build-essential pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-0
```

Fedora :

```sh
sudo dnf install gcc-c++ pkgconf-pkg-config alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel libxkbcommon-x11
```

Arch Linux :

```sh
sudo pacman -S --needed base-devel alsa-lib systemd-libs wayland libxkbcommon libxkbcommon-x11
```

Pour d'autres distributions, la liste de référence est
[`linux_dependencies.md`](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)
dans le dépôt de Bevy.

#### macOS

Seuls les outils en ligne de commande de Xcode sont nécessaires :

```sh
xcode-select --install
```

#### Windows

Rust propose deux chaînes de compilation sous Windows. **MSVC est recommandée** : c'est la
cible par défaut de `rustup` et celle que Bevy teste en priorité.

**Option A : MSVC (recommandée)**

1. Installez les *Build Tools for Visual Studio 2022* avec la charge de travail
   « Développement Desktop en C++ » (elle inclut le compilateur MSVC et le SDK Windows).
   En une commande :

   ```powershell
   winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```

   Si Visual Studio est déjà installé, ajoutez simplement cette charge de travail depuis
   Visual Studio Installer.
2. Installez `rustup` en gardant le choix par défaut (`x86_64-pc-windows-msvc`).
3. Ouvrez un **nouveau** terminal pour que `cargo` soit dans le `PATH`.

**Option B : GNU (MinGW)**

À utiliser si vous ne voulez pas installer les outils Microsoft.

1. Installez [MSYS2](https://www.msys2.org), puis, dans le terminal « MSYS2 UCRT64 » :

   ```sh
   pacman -S --needed mingw-w64-ucrt-x86_64-gcc
   ```

2. Ajoutez `C:\msys64\ucrt64\bin` au `PATH` de Windows.
3. Indiquez à `rustup` d'utiliser la cible GNU par défaut :

   ```powershell
   rustup set default-host x86_64-pc-windows-gnu
   ```

Avec GNU, n'utilisez pas `--features dev` : le lien dynamique de Bevy dépasse la limite de
symboles exportés d'une DLL MinGW.

### 3. Récupérer le code

Le jeu n'embarque pas les images, sons et traductions de shapez.io : on les copie depuis
le **dépôt de code source** de shapez.io (le dossier qui contient `res_raw/`, `res/` et
`translations/`), et non depuis une version installée du jeu (Steam ou autre), qui ne
contient pas ces fichiers sous cette forme.

Le plus simple est de cloner les deux dépôts côte à côte, dans un même dossier parent :

```sh
mkdir shapez && cd shapez
git clone https://github.com/Flyns157/Shapez.rust.git
git clone https://github.com/Flyns157/shapez.io.git
cd Shapez.rust
```

Vous obtenez cette arborescence, où le clone de shapez.io est trouvé automatiquement à
l'emplacement par défaut `../shapez.io` :

```text
shapez/
├── Shapez.rust/   ← ce dépôt, où l'on lance les commandes
└── shapez.io/     ← code source du jeu d'origine
```

Si vous placez le clone ailleurs, indiquez son chemin dans la variable `SHAPEZ_JS_ROOT`
(voir [Variables d'environnement](#variables-denvironnement)). Un clone placé
directement dans ce dépôt, sous `Shapez.rust/shapez.io/`, est ignoré par git ; il faut
alors `SHAPEZ_JS_ROOT=shapez.io`.

> [!NOTE]
> Le clone de shapez.io n'est utile que pour importer les assets et pour régénérer les
> vecteurs de test de l'[oracle JS](#oracle-js). La compilation, les tests (`cargo test`)
> et la CI n'en ont pas besoin : les vecteurs de test sont versionnés dans ce dépôt.

### 4. Importer les assets du jeu d'origine

```sh
cargo xtask import-assets
```

Cette commande copie les sprites, effets sonores, polices et traductions dans `assets/`
(dossier ignoré par git) et liste ce qui a été copié dans `assets/IMPORTED.txt`. Elle
convertit aussi la police du jeu du format WOFF2 au format TTF, seul lu par Bevy : si vous
aviez importé les assets avant cette conversion, relancez la commande, sinon le jeu
utilise la police par défaut de Bevy, sans accents. Ajoutez
`--music` pour importer aussi les musiques, qui pèsent environ 185 Mo :

```sh
cargo xtask import-assets --music
```

Ajoutez `--fonts` pour télécharger aussi les polices Noto (SIL Open Font License 1.1,
environ 23 Mo, dans `assets/fonts-noto/`). La police du jeu ne couvre que l'alphabet
latin : elles servent au chinois, au japonais, au coréen, au russe, à l'ukrainien et à
l'hébreu. Le téléchargement passe par `curl`, présent sous Linux, macOS et Windows 10 et
suivants ; les fichiers déjà présents ne sont pas retéléchargés.

```sh
cargo xtask import-assets --fonts
```

Avec un clone placé ailleurs, définissez `SHAPEZ_JS_ROOT` pour la commande :

```sh
SHAPEZ_JS_ROOT=/chemin/vers/shapez.io cargo xtask import-assets
```

```powershell
$env:SHAPEZ_JS_ROOT = "C:\chemin\vers\shapez.io"
cargo xtask import-assets
```

Si le clone est introuvable, la commande s'arrête avec le message « jeu d'origine
introuvable » et le chemin qu'elle a essayé.

### 5. Lancer le jeu

```sh
cargo run
```

La fenêtre du jeu s'ouvre sur le menu principal. Pour le développement, la feature `dev`
active le lien dynamique de Bevy (recompilation plus rapide) et ses outils de debug :

```sh
cargo run --features dev
```

Pour une version optimisée :

```sh
cargo run --release
```

Le binaire produit est `target/release/shapez_game` (`shapez_game.exe` sous Windows).

#### Raccourcis de construction

| Action | Raccourci |
|---|---|
| Sélectionner une zone | Ctrl + glisser (ajouter Maj pour étendre la sélection) |
| Supprimer la sélection | Suppr ou X |
| Vider les items de la sélection | B |
| Copier, couper, coller un plan | Ctrl+C, Ctrl+X, Ctrl+V (débloqué au niveau 12, payé en formes de plan) |
| Tourner le plan | R |
| Annuler, rétablir | Ctrl+Z, Ctrl+Y |
| Poser un repère | M |
| Épingler une forme du shop | clic sur la forme |
| Changer de couche (normale, câbles) | E (couche câbles débloquée en terminant le niveau 20) |
| Basculer un levier | clic sur le levier |
| Changer le signal d'un émetteur constant | clic dessus sur la couche câbles, puis choisir une valeur ou taper un code (`1`, `false`, `red`, `CuCuCuCu`…) et Entrée |
| Lecteur, filtre, afficheur | U, O, P (couche normale ou câbles) |
| Processeur virtuel, analyseur, comparateur | 5, 6, 7 sur la couche câbles (T : variante du processeur) |
| Exporter la base en image | F3 |

Ce sont les touches par défaut. Les touches simples (bâtiments, déplacement, pipette,
rotation, variante, améliorations, statistiques, couche, repère, export, vider et supprimer
la sélection) se changent dans **Paramètres → Contrôles**. Les raccourcis Ctrl, Échap,
Espace, Tab, les flèches et X restent fixes.

### 6. Lancer les tests

```sh
cargo test --workspace
```

Avant de proposer une modification, lancez aussi les mêmes vérifications que la CI :

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Distribuer le jeu

`cargo xtask dist` compile le jeu avec le profil `dist` (release sans symboles de
débogage) et prépare un dossier prêt à archiver dans `target/packages/` :

```text
shapez-rs-0.1.0-x86_64-unknown-linux-gnu/
├── shapez-rs          (shapez-rs.exe sous Windows)
├── assets/            copie de assets/, sans la musique
├── LICENSE
├── NOTICE
└── README.md
```

Le jeu cherche `assets/` à côté de son exécutable : le dossier peut être déplacé n'importe
où. Les assets doivent avoir été importés avant (`cargo xtask import-assets --fonts`).
Options :

- `--target <triplet>` : compile pour une autre cible (par exemple
  `x86_64-pc-windows-gnu`, voir la section suivante) ;
- `--music` : copie aussi la musique, si elle a été importée. Elle est exclue par défaut,
  car ses droits restent à vérifier avant une diffusion publique (voir `NOTICE`).

Sous Linux, le binaire a besoin des bibliothèques ALSA et udev du système (`libasound2`,
`libudev1`), présentes sur la plupart des distributions de bureau.

Le workflow GitHub **Release** (`.github/workflows/release.yml`) fait la même chose pour
Linux x86_64, Windows x86_64, macOS (Apple Silicon et Intel) et le web :

- lancé à la main depuis l'onglet Actions (« Run workflow »), il produit les cinq
  archives comme artefacts du run ;
- sur un tag `v*` (`git tag v0.1.0 && git push origin v0.1.0`), il les joint en plus à
  une release GitHub **en brouillon**, à relire puis publier depuis l'onglet Releases.

### Version web

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130   # la version de wasm-bindgen dans Cargo.lock
cargo xtask web
cd target/packages/shapez-rs-0.1.0-web && python3 -m http.server 8000
```

puis ouvrir <http://localhost:8000>. Le dossier contient `index.html`, `shapez.js`,
`shapez_bg.wasm` (environ 50 Mo) et `assets/`. N'importe quel hébergement statique convient
(GitHub Pages, par exemple), mais pas l'ouverture directe du fichier : le jeu télécharge
ses assets par HTTP.

Différences avec la version de bureau :

- la langue par défaut est celle du navigateur ;
- les paramètres et les sauvegardes sont dans le `localStorage` du navigateur, propre au
  site, et limité à environ 5 Mo : une très grande usine peut ne plus pouvoir être
  sauvegardée (l'erreur s'affiche alors en jeu) ;
- les navigateurs n'autorisent le son qu'après une première interaction avec la page ;
- le rendu passe par WebGL 2 : il est nettement plus lent que la version de bureau.

Le workflow **Release** produit aussi cette archive (`shapez-rs-<version>-web.zip`), et la
CI vérifie à chaque PR que le jeu compile pour `wasm32-unknown-unknown`.

### Mesurer les performances

```sh
cargo bench -p shapez_sim
```

Ces benchmarks mesurent un tick complet de la simulation sur deux usines de 10 000 et
100 000 tuiles de convoyeur. Avec `SHAPEZ_BENCH_FULL=1`, la taille de 500 000 tuiles,
l'objectif du plan, s'ajoute ; sa préparation est plus longue.

- `rings` : des anneaux fermés dont tous les items tournent ;
- `blocked` : des lignes saturées dont plus rien ne bouge.

La génération d'un chunk est aussi mesurée. La CI lance ces benchmarks sur `main`
uniquement.

## Compiler pour Windows depuis Linux

On peut produire un `.exe` Windows depuis Linux avec la cible GNU et MinGW. Exemple sous
Debian ou Ubuntu :

```sh
sudo apt install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu      # à lancer dans le dépôt : ajoute la cible à la toolchain 1.97.1
cargo build --release --target x86_64-pc-windows-gnu -p shapez_game
```

Le binaire est dans `target/x86_64-pc-windows-gnu/release/shapez_game.exe`. Il ne dépend
que de DLL fournies par Windows : aucune DLL MinGW n'est à copier avec lui.

Pour le lancer sur un PC Windows, copiez à côté de lui le dossier `assets/` (rempli par
`cargo xtask import-assets`) : le jeu le trouve tout seul. Le plus simple est
`cargo xtask dist --target x86_64-pc-windows-gnu`, qui prépare ce dossier.

N'utilisez pas `--features dev` pour une compilation croisée : le binaire dépendrait de
DLL qui ne sont pas distribuées avec lui.

## Variables d'environnement

Toutes sont facultatives.

| Variable | Utilisée par | Effet | Valeur par défaut |
|---|---|---|---|
| `SHAPEZ_JS_ROOT` | `cargo xtask import-assets`, oracle JS | Chemin du clone du code source de shapez.io | `../shapez.io` |
| `SHAPEZ_ASSETS` | le jeu | Dossier des assets importés | `assets/` à côté de l'exécutable s'il existe, sinon à la racine de ce dépôt |
| `SHAPEZ_SAVE_DIR` | le jeu | Dossier des sauvegardes | voir [Sauvegardes](#sauvegardes) |
| `SHAPEZ_SANDBOX` | le jeu | Si elle est définie (par exemple `SHAPEZ_SANDBOX=1`), les nouvelles parties démarrent en bac à sable : tous les bâtiments et variantes débloqués, et 1 000 000 formes de plan en stock pour tester le copier-coller | désactivé |
| `SHAPEZ_SETTINGS` | le jeu | Fichier des paramètres | voir [Paramètres](#paramètres) |
| `SHAPEZ_SCREENSHOT_DIR` | le jeu | Dossier des exports de la base (F3) | voir [Paramètres](#paramètres) |
| `SHAPEZ_LANG` | le jeu | Langue de l'interface : `fr`, `de`, `pt-BR`… (identifiants du jeu d'origine, ou code de langue comme `fr_FR`). Elle passe avant la langue choisie dans les [paramètres](#paramètres), qui passe elle-même avant la langue du système (`LC_ALL`, `LC_MESSAGES`, `LANG`). Le chinois, le japonais, le coréen, le russe, l'ukrainien et l'hébreu demandent les polices Noto importées par `cargo xtask import-assets --fonts` ; sans elles, ces langues retombent sur l'anglais | langue du système, sinon anglais |
| `SHAPEZ_SEED` | le jeu | Graine des nouvelles parties, de 0 à 100 000, pour obtenir toujours la même carte | aléatoire |
| `SHAPEZ_GOLDEN_SHAPES`, `SHAPEZ_GOLDEN_BELTS` | tests | Fichier de vecteurs de test plus grand à vérifier (voir [Oracle JS](#oracle-js)) | fichiers de `tests/golden/` |

Sous Linux et macOS, on définit une variable pour une seule commande en la plaçant
devant : `SHAPEZ_SEED=1234 cargo run`. Sous Windows (PowerShell), on la définit pour la
session du terminal : `$env:SHAPEZ_SEED = "1234"`, puis `cargo run`.

## Sauvegardes

Les parties sont sauvegardées automatiquement toutes les 2 minutes et en quittant. Le
menu principal liste les sauvegardes existantes. Elles sont rangées dans le dossier de
données de la plateforme, ou dans `SHAPEZ_SAVE_DIR` s'il est défini :

| Plateforme | Dossier |
|---|---|
| Linux | `~/.local/share/shapez-rs/saves` |
| macOS | `~/Library/Application Support/shapez-rs/saves` |
| Windows | `%APPDATA%\shapez-rs\data\saves` |

## Paramètres

Le bouton « Paramètres » du menu principal règle la langue, le volume des effets, celui
de la musique, le plein écran, le thème (sombre ou clair) et l'aide daltonien. Tout
s'applique tout de suite, sauf la langue, qui s'applique au prochain lancement. Les
effets sonores viennent de `cargo xtask import-assets` ; la musique n'est jouée que si
elle a été importée avec `--music`.

Le thème change les couleurs de la carte en partie (fond, grille, gisements) et du texte
posé dessus ; les menus restent sombres. L'aide daltonien affiche le nom de la couleur
sous le curseur (item d'un convoyeur, item en sortie d'un bâtiment, ou gisement) et
entoure la tuile, comme dans le jeu d'origine.

En partie, **F3** exporte toute la base en PNG, comme le jeu d'origine : l'image couvre
tous les bâtiments (128 pixels par chunk, moins pour une très grande base). Elle est écrite
dans le dossier `screenshots` du dossier de données (à côté des sauvegardes), ou dans
celui indiqué par `SHAPEZ_SCREENSHOT_DIR`.

L'écran **Contrôles** liste les touches par catégorie, comme le jeu d'origine : « Changer »
attend la nouvelle touche (Échap annule), « Réinitialiser » revient à la touche par défaut.
Les touches sont physiques : celle à la place du W d'un clavier QWERTY reste « aller en
haut » sur un AZERTY. Leur libellé suit l'agencement du clavier dès qu'elles ont été
tapées une fois ; avant, c'est le nom QWERTY. Seules les touches changées sont
enregistrées, sous `keybindings`.

Les paramètres sont enregistrés à chaque changement dans `settings.json`, dans le dossier
de configuration de la plateforme, ou dans le fichier indiqué par `SHAPEZ_SETTINGS` :

| Plateforme | Fichier |
|---|---|
| Linux | `~/.config/shapez-rs/settings.json` |
| macOS | `~/Library/Application Support/shapez-rs/settings.json` |
| Windows | `%APPDATA%\shapez-rs\config\settings.json` |

## Organisation du code

| Crate | Rôle | Dépend de Bevy |
|---|---|---|
| `crates/shapez_core` | Grille, formes (codes courts, opérations), couleurs, items, registre | non |
| `crates/shapez_render_shapes` | Rastérisation des formes en RGBA (tiny-skia) | non |
| `crates/shapez_sim` | Simulation déterministe à 60 ticks/s, en entiers ; génération de carte identique au JS | non |
| `crates/shapez_save` | Sauvegardes : en-tête, état JSON compressé zstd, CRC32, migrations, dossier | non |
| `crates/shapez_game` | Binaire Bevy : rendu, entrées, UI, autosave | oui |
| `xtask` | Tâches de développement (`cargo xtask import-assets`) | non |

La crate `shapez_data` du plan sera ajoutée avec le jalon qui en a besoin.

## Oracle JS

Le code JavaScript d'origine sert de spécification. Les scripts de `tools/js-oracle/`
l'exécutent dans Node (sans webpack) pour produire des vecteurs de test, qui sont
versionnés dans le dépôt. Seules les personnes qui régénèrent ces vecteurs ont besoin de
Node et du clone de shapez.io ; la CI n'en a pas besoin. Le code JS est cherché dans
`../shapez.io/src/js`, ou dans `$SHAPEZ_JS_ROOT/src/js`.

Régénérer les vecteurs versionnés :

```sh
node tools/js-oracle/shapes.mjs > crates/shapez_core/tests/golden/shapes.json            # 2 000 cas
node tools/js-oracle/rng.mjs > crates/shapez_core/tests/golden/alea.json
node tools/js-oracle/map.mjs > crates/shapez_sim/tests/golden/map.json                   # chunks (-15..15)², 3 graines
node tools/js-oracle/belts.mjs > crates/shapez_sim/tests/golden/belts.json               # 100 chemins de convoyeurs
node tools/js-oracle/progression.mjs > crates/shapez_sim/tests/golden/progression.json   # paliers et freeplay
node tools/js-oracle/wires.mjs 80 > crates/shapez_sim/tests/golden/wires.json             # 80 circuits de câbles et processeurs virtuels
```

Vérifier un jeu de cas plus grand, sans le versionner :

```sh
node tools/js-oracle/shapes.mjs 10000 > /tmp/shapes.json
SHAPEZ_GOLDEN_SHAPES=/tmp/shapes.json cargo test -p shapez_core --test golden_shapes
node tools/js-oracle/belts.mjs 1500 > /tmp/belts.json
SHAPEZ_GOLDEN_BELTS=/tmp/belts.json cargo test -p shapez_sim --test golden_belts
```

## Licence

GPL-3.0-or-later, comme le jeu d'origine. Voir [`NOTICE`](NOTICE) et [`LICENSE`](LICENSE).
« shapez » est une marque de tobspr Games ; ce projet n'est ni affilié ni approuvé par
tobspr Games.
