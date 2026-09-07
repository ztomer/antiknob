//! Installed-app discovery for launch/quit actions (Phase 4/5).
//!
//! Scans application directories for `.app` bundles and reads their
//! `Info.plist` bundle identifiers. Pure std + plist I/O over any
//! directories, so it is fully unit-testable with fixture bundles and the
//! GUI pickers can reuse it later. Unreadable bundles are skipped, never
//! fatal: a partial list beats no list.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: Option<String>,
    pub path: PathBuf,
}

/// Directories scanned by default (system + user applications).
pub fn default_app_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Ok(home) = std::env::var("HOME") {
        let user = PathBuf::from(home).join("Applications");
        if user != dirs[0] {
            dirs.push(user);
        }
    }
    dirs
}

/// Display name for a bundle directory (`Foo.app` to `Foo`).
fn bundle_display_name(bundle_dir: &Path) -> String {
    bundle_dir
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| bundle_dir.to_string_lossy().into_owned())
}

fn read_bundle_id(bundle_dir: &Path) -> Option<String> {
    let plist_path = bundle_dir.join("Contents/Info.plist");
    let value = plist::Value::from_file(&plist_path).ok()?;
    value
        .as_dictionary()?
        .get("CFBundleIdentifier")?
        .as_string()
        .map(str::to_string)
}

/// Scan directories (non-recursive, top level only) for `.app` bundles,
/// sorted by display name. Missing/unreadable directories are skipped.
pub fn scan_app_dirs(dirs: &[PathBuf]) -> Vec<AppInfo> {
    let mut apps = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "app") {
                continue;
            }
            if !path.join("Contents/Info.plist").exists() {
                continue;
            }
            apps.push(AppInfo {
                name: bundle_display_name(&path),
                bundle_id: read_bundle_id(&path),
                path,
            });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_app(root: &Path, name: &str, bundle_id: Option<&str>) -> PathBuf {
        use std::fmt::Write as _;
        let dir = root.join(format!("{}.app/Contents", name));
        std::fs::create_dir_all(&dir).unwrap();
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\n",
        );
        if let Some(id) = bundle_id {
            writeln!(xml, "<key>CFBundleIdentifier</key><string>{}</string>", id).unwrap();
        }
        xml.push_str("</dict></plist>\n");
        std::fs::write(dir.join("Info.plist"), xml).unwrap();
        root.join(format!("{}.app", name))
    }

    #[test]
    fn scan_finds_bundles_skips_junk_and_sorts() {
        let root = std::env::temp_dir().join(format!("antiknob-apps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        fixture_app(&root, "Zebra", Some("com.example.zebra"));
        fixture_app(&root, "alpha", Some("com.example.alpha"));
        fixture_app(&root, "NoId", None);
        std::fs::create_dir_all(root.join("NotAnApp")).unwrap();
        std::fs::write(root.join("notes.txt"), "hi").unwrap();

        let apps = scan_app_dirs(std::slice::from_ref(&root));
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[0].name, "alpha");
        assert_eq!(apps[0].bundle_id.as_deref(), Some("com.example.alpha"));
        assert_eq!(apps[2].name, "Zebra");
        assert!(apps
            .iter()
            .any(|a| a.name == "NoId" && a.bundle_id.is_none()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_tolerates_missing_dirs() {
        let apps = scan_app_dirs(&[PathBuf::from("/nonexistent-antiknob-dir")]);
        assert!(apps.is_empty());
    }
}
