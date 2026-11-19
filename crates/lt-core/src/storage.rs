//! Workspace local : `data/` (JSON) + `reports/`, compatible avec l'ancienne
//! plateforme Node.

use crate::folders;
use crate::models::{Auth, Run, RunStatus, Test};
use crate::summary;
use chrono::Local;
use rust_i18n::t;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DATA_DIR: &str = "data";
pub const REPORTS_DIR: &str = "reports";

#[derive(Clone, Debug)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn open(root: impl AsRef<Path>) -> io::Result<Self> {
        let workspace = Self {
            root: root.as_ref().to_path_buf(),
        };
        fs::create_dir_all(workspace.data_dir())?;
        fs::create_dir_all(workspace.runs_dir())?;
        fs::create_dir_all(workspace.reports_dir())?;
        workspace.seed()?;
        Ok(workspace)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn data_dir(&self) -> PathBuf {
        self.root.join(DATA_DIR)
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.data_dir().join("runs")
    }

    pub fn reports_dir(&self) -> PathBuf {
        self.root.join(REPORTS_DIR)
    }

    /// Dossier de rapports d'une exécution : `reports/<run.name>/`.
    pub fn run_reports_dir(&self, run_name: &str) -> PathBuf {
        self.reports_dir().join(run_name)
    }

    pub fn tests_path(&self) -> PathBuf {
        self.data_dir().join("tests.json")
    }

    pub fn folders_path(&self) -> PathBuf {
        self.data_dir().join("folders.json")
    }

    pub fn auth_path(&self) -> PathBuf {
        self.data_dir().join("auth.json")
    }

    pub fn runs_path(&self) -> PathBuf {
        self.data_dir().join("runs.json")
    }

    fn seed(&self) -> io::Result<()> {
        if !self.tests_path().exists() {
            self.save_tests(&[Test::demo()])?;
        }
        if !self.folders_path().exists() {
            self.save_folders(&[])?;
        }
        if !self.runs_path().exists() {
            self.save_runs(&[])?;
        }
        Ok(())
    }

    fn read_json<T: DeserializeOwned + Default>(&self, path: &Path) -> T {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
                eprintln!("JSON illisible {} : {error}", path.display());
                T::default()
            }),
            Err(_) => T::default(),
        }
    }

    fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> io::Result<()> {
        let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
        atomic_write(path, text.as_bytes())
    }

    pub fn load_tests(&self) -> Vec<Test> {
        self.read_json(&self.tests_path())
    }

    pub fn save_tests(&self, tests: &[Test]) -> io::Result<()> {
        self.write_json(&self.tests_path(), &tests)
    }

    pub fn load_folders(&self) -> Vec<String> {
        self.read_json(&self.folders_path())
    }

    pub fn save_folders(&self, folders: &[String]) -> io::Result<()> {
        self.write_json(&self.folders_path(), &folders)
    }

    pub fn load_auth(&self) -> Vec<Auth> {
        self.read_json(&self.auth_path())
    }

    pub fn save_auth(&self, auth: &[Auth]) -> io::Result<()> {
        self.write_json(&self.auth_path(), &auth)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(self.auth_path(), fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// Charge l'historique et marque comme « interrompu » les tests qui étaient
    /// encore en cours lors d'un arrêt de l'application.
    pub fn load_runs(&self) -> Vec<Run> {
        let mut runs: Vec<Run> = self.read_json(&self.runs_path());
        let mut changed = false;
        for run in runs.iter_mut() {
            if run.status == RunStatus::Running {
                run.status = RunStatus::Interrupted;
                changed = true;
            }
            if run.reports.is_empty() {
                run.reports = self.report_files(&run.name);
            }
        }
        if changed {
            let _ = self.save_runs(&runs);
        }
        runs
    }

    pub fn save_runs(&self, runs: &[Run]) -> io::Result<()> {
        self.write_json(&self.runs_path(), &runs)
    }

    pub fn report_files(&self, run_name: &str) -> Vec<String> {
        // Nouveau layout : reports/<run.name>/{summary,executive,series}.*
        let dir = self.run_reports_dir(run_name);
        if dir.is_dir() {
            let mut files: Vec<String> = fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().to_string();
                    entry.path().is_file().then(|| format!("{run_name}/{name}"))
                })
                .collect();
            files.sort();
            if !files.is_empty() {
                return files;
            }
        }

        // Ancien layout à plat : reports/<run.name>-summary.html, …
        let prefix = format!("{run_name}-");
        let Ok(entries) = fs::read_dir(self.reports_dir()) else {
            return Vec::new();
        };
        let mut files: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                (entry.path().is_file() && name.starts_with(&prefix)).then_some(name)
            })
            .collect();
        files.sort();
        files
    }

    pub fn run_config_path(&self, run_name: &str) -> PathBuf {
        self.runs_dir().join(format!("{run_name}.json"))
    }

    pub fn run_log_path(&self, run_name: &str) -> PathBuf {
        self.runs_dir().join(format!("{run_name}.log"))
    }

    pub fn save_run_config(&self, run_name: &str, test: &Test, auth: Option<&Auth>) -> io::Result<()> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct RunConfig<'a> {
            #[serde(flatten)]
            test: &'a Test,
            auth: AuthRef<'a>,
        }
        #[derive(Serialize)]
        struct AuthRef<'a> {
            #[serde(rename = "type")]
            auth_type: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }
        let config = RunConfig {
            test,
            auth: AuthRef {
                auth_type: auth.map(|auth| auth.auth_type.as_str()).unwrap_or("none"),
                name: auth.map(|auth| auth.name.as_str()),
            },
        };
        self.write_json(&self.run_config_path(run_name), &config)
    }

    pub fn import_from(source: &Path, destination: &Path, force: bool) -> io::Result<ImportReport> {
        let source = source.to_path_buf();
        let data_source = if source.join(DATA_DIR).join("tests.json").exists() {
            source.join(DATA_DIR)
        } else if source.join("tests.json").exists() {
            source.clone()
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                t!("storage.import.not_found").to_string(),
            ));
        };
        let source_root = data_source
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| source.clone());

        fs::create_dir_all(destination.join(DATA_DIR))?;
        fs::create_dir_all(destination.join(REPORTS_DIR))?;
        if destination.join(DATA_DIR).join("tests.json").exists() && !force {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                t!("storage.import.already_exists").to_string(),
            ));
        }

        let mut files = 0usize;
        for name in ["tests.json", "folders.json", "auth.json", "runs.json"] {
            let from = data_source.join(name);
            if from.exists() {
                fs::copy(&from, destination.join(DATA_DIR).join(name))?;
                files += 1;
            }
        }
        {
            let dir = "runs";
            let from = data_source.join(dir);
            if from.is_dir() {
                files += copy_dir(&from, &destination.join(DATA_DIR).join(dir))?;
            }
        }
        let reports = source_root.join(REPORTS_DIR);
        if reports.is_dir() {
            files += copy_dir(&reports, &destination.join(REPORTS_DIR))?;
        }

        let workspace = Workspace::open(destination)?;
        let mut runs = workspace.load_runs();
        let mut backfilled = 0usize;
        for run in runs.iter_mut() {
            if run.summary.is_none() {
                let candidates = [
                    workspace.run_reports_dir(&run.name).join("summary.json"),
                    workspace
                        .reports_dir()
                        .join(format!("{}-summary.json", run.name)),
                ];
                for path in candidates {
                    if let Ok(text) = fs::read_to_string(path)
                        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                        && value.get("metrics").is_some()
                    {
                        run.summary = Some(summary::from_legacy_k6(&value));
                        backfilled += 1;
                        break;
                    }
                }
            }
            if run.reports.is_empty() {
                run.reports = workspace.report_files(&run.name);
            }
        }
        workspace.save_runs(&runs)?;
        Ok(ImportReport {
            files,
            runs: runs.len(),
            backfilled,
            workspace: destination.to_path_buf(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub files: usize,
    pub runs: usize,
    pub backfilled: usize,
    pub workspace: PathBuf,
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<usize> {
    fs::create_dir_all(to)?;
    let mut count = 0;
    for entry in fs::read_dir(from)?.flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            count += copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
            count += 1;
        }
    }
    Ok(count)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let temporary = path.with_file_name(format!(".{file_name}.tmp"));
    fs::write(&temporary, bytes)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::AlreadyExists | io::ErrorKind::PermissionDenied
            ) =>
        {
            let _ = fs::remove_file(path);
            fs::rename(&temporary, path)
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

// ---------------------------------------------------------------------------
// Identifiants et noms de fichiers
// ---------------------------------------------------------------------------

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_else(|_| "0".to_string())
}

pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed) ^ (std::process::id() as u64);
    let random = crate::template::now_ms() ^ (counter.wrapping_mul(2654435761));
    format!("{}{}", base36(crate::template::now_ms()), base36(random))
}

pub fn slug(input: &str) -> String {
    use unicode_normalization::char::is_combining_mark;
    use unicode_normalization::UnicodeNormalization;
    let plain: String = input
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .collect::<String>()
        .to_lowercase();
    let mut out = String::new();
    for character in plain.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    let out: String = out.chars().take(40).collect();
    if out.is_empty() {
        "test".to_string()
    } else {
        out
    }
}

pub fn stamp() -> String {
    Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn run_name(test_name: &str, profile: &str) -> String {
    format!("{}-{}-{}", slug(test_name), profile, stamp())
}

// ---------------------------------------------------------------------------
// Paramètres de l'application
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub workspace: Option<String>,
    pub theme: String,
    /// Code langue (`en` ou `fr`). Défaut : anglais.
    pub language: String,
    pub last_tab: String,
    pub last_test_id: String,
    pub profile: crate::models::Profile,
    pub collapsed_folders: Vec<String>,
    pub window_width: Option<f32>,
    pub window_height: Option<f32>,
    pub last_report_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            workspace: None,
            theme: "dark".to_string(),
            language: "en".to_string(),
            last_tab: "run".to_string(),
            last_test_id: String::new(),
            profile: crate::models::Profile::Smoke,
            collapsed_folders: Vec::new(),
            window_width: Some(1280.0),
            window_height: Some(860.0),
            last_report_dir: None,
        }
    }
}

impl Settings {
    pub fn config_path() -> PathBuf {
        directories::ProjectDirs::from("com", "loadtesting", "Load Testing")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("settings.json")
    }

    pub fn load() -> Self {
        fs::read_to_string(Self::config_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path();
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        atomic_write(&path, text.as_bytes())
    }
}

pub fn default_workspace() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
        .or_else(|| directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Load Testing")
}

/// If `dir` looks like a workspace (`data/tests.json`), return it.
pub fn workspace_if_present(dir: &Path) -> Option<PathBuf> {
    if dir.join("data").join("tests.json").is_file() {
        Some(dir.to_path_buf())
    } else {
        None
    }
}

/// If the current directory looks like a workspace (`data/tests.json`), return it.
/// Used by the CLI so `cd /opt/load-tests && lt list` works without `LT_WORKSPACE`.
pub fn workspace_from_cwd() -> Option<PathBuf> {
    workspace_if_present(&std::env::current_dir().ok()?)
}

pub fn all_folders(workspace: &Workspace) -> Vec<String> {
    folders::all_folders(&workspace.load_folders(), &workspace.load_tests())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace() -> Workspace {
        let dir = std::env::temp_dir().join(format!("lt-test-{}", new_id()));
        Workspace::open(&dir).unwrap()
    }

    #[test]
    fn seeds_and_roundtrips() {
        let workspace = temp_workspace();
        let tests = workspace.load_tests();
        assert_eq!(tests.len(), 1);
        let mut tests = tests;
        tests[0].name = "Modifié".into();
        workspace.save_tests(&tests).unwrap();
        assert_eq!(workspace.load_tests()[0].name, "Modifié");
        let _ = fs::remove_dir_all(workspace.root());
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Consultation des forfaits"), "consultation-des-forfaits");
        assert_eq!(slug("Togocom / Préprod"), "togocom-preprod");
        assert_eq!(slug(""), "test");
        assert_eq!(slug("éàü"), "eau");
    }

    #[test]
    fn ids_are_unique() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
    }

    #[test]
    fn workspace_if_present_detects_tests_json() {
        let dir = std::env::temp_dir().join(format!("lt-cwd-{}", new_id()));
        fs::create_dir_all(dir.join("data")).unwrap();
        fs::write(dir.join("data").join("tests.json"), "[]").unwrap();
        assert_eq!(workspace_if_present(&dir).as_ref(), Some(&dir));
        assert!(workspace_if_present(Path::new("/nonexistent-lt-workspace")).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn imports_legacy_workspace() {
        let source = std::env::temp_dir().join(format!("lt-legacy-{}", new_id()));
        fs::create_dir_all(source.join("data").join("runs")).unwrap();
        fs::create_dir_all(source.join("reports")).unwrap();
        fs::write(
            source.join("data").join("tests.json"),
            r#"[{"id":"a","name":"Ancien","steps":[]}]"#,
        )
        .unwrap();
        fs::write(
            source.join("data").join("runs.json"),
            r#"[{"id":"r","name":"r","testId":"a","testName":"Ancien","profile":"smoke","status":"passed","startedAt":1}]"#,
        )
        .unwrap();
        let destination = std::env::temp_dir().join(format!("lt-dest-{}", new_id()));
        let report = Workspace::import_from(&source, &destination, true).unwrap();
        assert_eq!(report.runs, 1);
        let workspace = Workspace::open(&destination).unwrap();
        assert_eq!(workspace.load_tests()[0].name, "Ancien");
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(destination);
    }
}
