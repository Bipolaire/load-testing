//! Langue de l'application (anglais par défaut, français en second).

/// Langues supportées.
pub const SUPPORTED: &[&str] = &["en", "fr"];

/// Normalise un code langue (`en`, `fr`, `en-US` → `en`).
pub fn normalize(lang: &str) -> &'static str {
    let lower = lang.trim().to_ascii_lowercase();
    if lower.starts_with("fr") {
        "fr"
    } else {
        "en"
    }
}

/// Applique la langue active pour les chaînes de `lt-core` (rapports, labels).
pub fn set_language(lang: &str) {
    rust_i18n::set_locale(normalize(lang));
}

/// Langue actuellement active.
pub fn language() -> String {
    rust_i18n::locale().to_string()
}

/// `true` si la langue active est le français.
pub fn is_french() -> bool {
    language().starts_with("fr")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Vérifie que chaque clé des fichiers YAML a bien `en:` et `fr:`.
    fn assert_locale_file_complete(path: &str) {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut current: Option<String> = None;
        let mut langs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for line in text.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("_version") {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("  ") {
                if rest.starts_with("en:") || rest.starts_with("fr:") {
                    let lang = &rest[..2];
                    if let Some(key) = &current {
                        langs.entry(key.clone()).or_default().push(lang.to_string());
                    }
                }
            } else if let Some(key) = trimmed.strip_suffix(':') {
                current = Some(key.to_string());
            }
        }
        assert!(!langs.is_empty(), "{path} has no keys");
        for (key, found) in &langs {
            assert!(
                found.iter().any(|l| l == "en"),
                "{path}: missing en for {key}"
            );
            assert!(
                found.iter().any(|l| l == "fr"),
                "{path}: missing fr for {key}"
            );
        }
    }

    #[test]
    fn locale_files_have_en_and_fr() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for rel in [
            "crates/lt-core/locales/core.yml",
            "crates/lt-app/locales/app.yml",
            "crates/lt-cli/locales/cli.yml",
            "crates/lt-engine/locales/engine.yml",
        ] {
            assert_locale_file_complete(root.join(rel).to_str().unwrap());
        }
    }

    #[test]
    fn normalize_defaults_to_english() {
        assert_eq!(normalize("fr-FR"), "fr");
        assert_eq!(normalize("en-US"), "en");
        assert_eq!(normalize("de"), "en");
    }

    #[test]
    fn set_language_switches_labels() {
        set_language("fr");
        assert!(crate::models::RunStatus::Passed.label().contains("réussi")
            || crate::models::RunStatus::Passed.label().to_lowercase().contains("réuss"));
        set_language("en");
        assert!(crate::models::RunStatus::Passed.label().to_lowercase().contains("pass"));
    }
}
