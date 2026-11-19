# Load Testing

Application de bureau native (Rust + GPUI) pour concevoir et exécuter des tests de charge HTTP / SOAP — sans Docker, sans k6, sans serveur Node.

| Binaire | Rôle |
|---|---|
| **Load Testing** (`load-testing`) | Interface graphique : scénarios, auth, lancement, historique, rapports |
| **`lt`** | CLI pour CI / scripts |

Les scénarios, authentifications et historiques vivent dans un **workspace** (dossier `data/` + `reports/`), compatible avec l’ancienne plateforme Node.

## Prérequis

- Rust stable (édition 2024)
- macOS 13+ ou Windows 10+ pour l’app graphique
- Python 3 (uniquement pour générer les configs `cargo-packager`)

## Installation serveur (CLI)

Le binaire **`lt`** s’installe seul (sans GUI) sur Linux, Windows Server et macOS. Les Releases GitHub publient des archives CLI pour :

| Cible | Archive |
|---|---|
| Linux x86_64 (statique musl) | `lt-v*-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 (statique musl) | `lt-v*-aarch64-unknown-linux-musl.tar.gz` |
| Windows x64 | `lt-v*-x86_64-pc-windows-msvc.zip` |
| macOS Apple Silicon | `lt-v*-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `lt-v*-x86_64-apple-darwin.tar.gz` |

### One-liner

Le dépôt est **privé** : il faut un token GitHub (`repo`) et une **Release** (`v*`) contenant les archives `lt-v*`.

```bash
# Linux / macOS — créer un PAT puis :
export GITHUB_TOKEN=ghp_xxx   # ou fine-grained avec Contents: Read

# Récupère le script via l’API (raw.githubusercontent.com renvoie 404 sans auth)
curl -fsSL \
  -H "Authorization: Bearer $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.raw" \
  "https://api.github.com/repos/Bipolaire/load-testing/contents/scripts/install.sh?ref=develop" \
  | sh

# Version épinglée
curl -fsSL \
  -H "Authorization: Bearer $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.raw" \
  "https://api.github.com/repos/Bipolaire/load-testing/contents/scripts/install.sh?ref=develop" \
  | LT_VERSION=v0.1.0 sh
```

```powershell
# Windows
$env:GITHUB_TOKEN = 'ghp_xxx'
$headers = @{
  Authorization = "Bearer $env:GITHUB_TOKEN"
  Accept = 'application/vnd.github.raw'
}
irm "https://api.github.com/repos/Bipolaire/load-testing/contents/scripts/install.ps1?ref=develop" -Headers $headers | iex
```

Sans token : clonez le dépôt (SSH/HTTPS authentifié) puis `./scripts/install.sh` (toujours avec `GITHUB_TOKEN` pour télécharger la Release).

### Archive manuelle

1. Téléchargez l’archive + `SHA256SUMS` depuis la Release.
2. Extrayez `lt` / `lt.exe` et placez-le dans un répertoire du `PATH`.

```bash
tar -xzf lt-v0.1.0-x86_64-unknown-linux-musl.tar.gz
sudo install -m 755 lt /usr/local/bin/lt
lt --version
```

### Workspace sur serveur

```bash
# Option A — variable d’environnement
export LT_WORKSPACE=/opt/load-tests
lt list
lt run -s "Mon API" -p smoke

# Option B — se placer dans le workspace (dossier contenant data/tests.json)
cd /opt/load-tests
lt list
lt where
```

Copiez le workspace (scénarios + auth) depuis la machine où vous les avez conçus dans l’app, ou importez un ancien projet : `lt import /chemin/ancien --force`.

## Lancer en développement

```bash
# Interface graphique
cargo run -p lt-app

# CLI
cargo run -p lt-cli -- list
cargo run -p lt-cli -- run -s "Démo" -p smoke
```

Au premier lancement, un workspace par défaut est créé dans `Documents/Load Testing/` (ou le chemin mémorisé dans les réglages). Depuis l’app : **Ouvrir…** / **Importer…** pour pointer vers un ancien dossier de projet (celui qui contient `data/tests.json`).

Variable d’environnement utile pour la CLI :

```bash
export LT_WORKSPACE=/chemin/vers/mon/workspace
lt list
lt where
```

## Fonctionnalités

| Onglet | Contenu |
|---|---|
| **Lancer** | Choix du scénario et du profil (smoke, load, stress, spike, soak, tps, breakpoint), options VUs / TPS / durée / itérations. Logs et graphiques en direct, KPIs, seuils. |
| **Scénarios** | Chaînes de requêtes REST ou SOAP : URL, méthode, en-têtes, corps, variables `{{nom}}`, checks, extractions, objectifs. Import **curl**, bouton **Essayer** (un appel sans charge). Dossiers / sous-dossiers. |
| **Authentifications** | OAuth2 (client credentials ou password), Bearer fixe, clé d’API. Secrets dans `data/auth.json` (hors git, permissions 0600). |
| **Historique** | Exécutions passées, résumés, ouverture des rapports HTML/PDF. |

Thème Clair / Sombre / Auto mémorisé dans les réglages de l’app.

Un scénario marqué **effets réels** (achat, création…) exige une confirmation avant chaque lancement (double clic dans l’UI, `--yes` en CLI).

## Profils de charge

| Profil | Objectif | Forme |
|---|---|---|
| `smoke` | Vérifier que le scénario fonctionne | peu de VUs, courte durée |
| `load` | Charge nominale | montée → plateau → descente |
| `stress` | Point de rupture | paliers de VUs croissants |
| `spike` | Pic soudain | montée rapide puis retour |
| `soak` | Endurance | plateau long |
| `tps` | Débit cible constant | N transactions/s pendant la durée |
| `breakpoint` | Débit max soutenable | rampe jusqu’à N TPS, arrêt sur seuils |

Une **transaction** = une exécution complète du scénario (toutes les étapes). Les seuils par défaut font échouer le run si erreurs / latence / checks dépassent les objectifs du scénario.

## CLI (`lt`)

```bash
lt list [--folder Togocom]
lt validate [--scenario "Mon API"]
lt run -s "Mon API" -p load --vus 50 --duration 5m
lt run -s "Mon API" -p tps --tps 30 --duration 2m
lt run -s "Achat forfait" -p smoke --yes   # scénario « effets réels »
lt import /ancien/projet --force   # migre data/ + reports/
lt where
```

Codes de sortie : `0` succès, `1` seuils / validation en échec, `2` erreur d’usage.

## Rapports

Chaque exécution produit un dossier dédié sous `reports/<id>/` :

- `summary.html` — rapport lisible (métriques, seuils, checks, graphiques)
- `summary.pdf` — même rapport en PDF (sans graphiques temporels)
- `executive.pdf` — synthèse exécutive (verdict, KPIs clés, seuils)
- `summary.json` — données brutes
- `series.json` — série temporelle

Les chemins sont aussi listés dans l’historique de l’app (boutons HTML, PDF et synthèse).

## Architecture

```
crates/
  lt-core/     modèles, stockage JSON, profils, curl, SOAP/XML, rapports
  lt-engine/   moteur HTTP asynchrone (remplace k6)
  lt-cli/      binaire `lt`
  lt-app/      binaire `load-testing` (GPUI + gpui-component)
packager/      configs cargo-packager (macOS ARM/Intel, Windows)
assets/        icônes
```

Workspace typique :

```
MonWorkspace/
  data/
    tests.json
    folders.json
    auth.json      # secrets, non versionné
    runs.json
    runs/          # configs + logs par exécution
  reports/          # un dossier par exécution
```

## Tests et qualité

```bash
cargo test -p lt-core -p lt-engine -p lt-cli
cargo clippy -p lt-core -p lt-engine -p lt-cli -p lt-app --all-targets -- -D warnings
```

## Empaquetage

```bash
# Build release (cible locale)
cargo build --release -p lt-app -p lt-cli

# Archive CLI seule (même layout que les Releases)
./scripts/package-cli.sh
# → dist/cli/lt-v<version>-<target>.tar.gz (+ .sha256)

# Paquet macOS / Windows via cargo-packager
cargo install cargo-packager --locked --version 0.11.8   # une fois
cargo packager -c "$(python3 scripts/packager-config.py macos-arm64)"
# → dist/Load Testing.app et dist/*.dmg (ou .exe sous Windows)
```

La CI GitHub Actions (`.github/workflows/build.yml`) :

- teste `lt-core` / `lt-engine` / `lt-cli` ;
- packague l’app GUI (macOS ARM/Intel, Windows x64) ;
- packague la CLI seule (Linux musl x64/ARM64, macOS ARM/Intel, Windows x64) ;
- sur un tag `v*`, ouvre une release draft avec DMG/NSIS + archives `lt-v*` + `SHA256SUMS`.

## Migration depuis l’ancienne stack Node/k6

1. Ouvrir l’app → **Importer…** et choisir l’ancien dépôt (celui qui contient `data/`), **ou**
2. `lt import /chemin/ancien/projet --force` vers votre workspace.

Les scénarios, dossiers, auth et l’historique sont repris ; les résumés k6 sont reconstitués quand c’est possible. Docker, Grafana/InfluxDB et les scripts `scripts/*.js` ne sont plus nécessaires.
