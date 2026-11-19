//! Gestion des dossiers de scénarios. Un dossier est un chemin `Parent/Enfant`.

use crate::models::Test;
use rust_i18n::t;

pub fn norm_folder(folder: &str) -> String {
    folder
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn parent_folder(folder: &str) -> String {
    let normalized = norm_folder(folder);
    let mut parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() <= 1 {
        return String::new();
    }
    parts.pop();
    parts.join("/")
}

pub fn move_folder(folder: &str, from: &str, to: &str) -> String {
    let folder = norm_folder(folder);
    let from = norm_folder(from);
    let to = norm_folder(to);
    if folder == from {
        return to;
    }
    if let Some(rest) = folder.strip_prefix(&format!("{from}/")) {
        return norm_folder(&format!("{to}/{rest}"));
    }
    folder
}

pub fn in_folder(test: &Test, folder: &str) -> bool {
    let test_folder = norm_folder(&test.folder);
    let folder = norm_folder(folder);
    test_folder == folder || test_folder.starts_with(&format!("{folder}/"))
}

pub fn rename_folder(
    tests: &mut [Test],
    folders: &mut Vec<String>,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let from = norm_folder(from);
    let to = norm_folder(to);
    if from.is_empty() || to.is_empty() {
        return Err(t!("folder.error.empty_name").to_string());
    }
    if to == from {
        return Ok(());
    }
    if to.starts_with(&format!("{from}/")) {
        return Err(t!("folder.error.move_into_self").to_string());
    }
    for test in tests.iter_mut() {
        test.folder = move_folder(&test.folder, &from, &to);
    }
    let mut moved: Vec<String> = folders
        .iter()
        .map(|folder| move_folder(folder, &from, &to))
        .filter(|folder| !folder.is_empty())
        .collect();
    moved.push(to);
    moved.sort();
    moved.dedup();
    *folders = moved;
    Ok(())
}

/// Tous les chemins de dossiers connus (dossiers vides + dossiers des scénarios),
/// triés alphabétiquement.
pub fn all_folders(folders: &[String], tests: &[Test]) -> Vec<String> {
    let mut out: Vec<String> = folders.iter().map(|f| norm_folder(f)).collect();
    for test in tests {
        let folder = norm_folder(&test.folder);
        if !folder.is_empty() {
            out.push(folder);
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn folder_label(folder: &str) -> String {
    norm_folder(folder).split('/').collect::<Vec<_>>().join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(norm_folder("  A / B / "), "A/B");
        assert_eq!(norm_folder(""), "");
        assert_eq!(parent_folder("A/B/C"), "A/B");
        assert_eq!(parent_folder("A"), "");
        assert_eq!(move_folder("A/B", "A", "X"), "X/B");
        assert_eq!(move_folder("AB", "A", "X"), "AB");
    }

    #[test]
    fn rename_updates_tests_and_folders() {
        let mut tests = vec![
            Test {
                folder: "A/B".into(),
                ..Test::default()
            },
            Test {
                folder: "C".into(),
                ..Test::default()
            },
        ];
        let mut folders = vec!["A".into(), "A/B".into(), "C".into()];
        rename_folder(&mut tests, &mut folders, "A", "X/Y").unwrap();
        assert_eq!(tests[0].folder, "X/Y/B");
        assert_eq!(tests[1].folder, "C");
        assert!(folders.contains(&"X/Y".to_string()));
        assert!(folders.contains(&"X/Y/B".to_string()));
    }

    #[test]
    fn refuses_move_into_self() {
        let mut tests = vec![];
        let mut folders = vec![];
        assert!(rename_folder(&mut tests, &mut folders, "A", "A/B").is_err());
    }
}
