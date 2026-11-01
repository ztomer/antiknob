//! The daemon's socket path has one source of truth.
//!
//! The daemon serves `~/Library/Application Support/antiknob/antiknob.sock`
//! (`api::default_socket_path`) and used to add a `/tmp/antiknob.sock`
//! symlink "for easy discovery". `install.sh` asked the symlink whether the
//! tap was up, and the symlink was routinely absent -- every `SocketServer`
//! a test started deleted it on start and on drop, the real daemon's
//! included -- so the installer reported a running daemon as "not running".
//! The symlink is gone; the shell asks the CLI (`antiknob status --json`),
//! which resolves the path in Rust. The Swift app spells the Application
//! Support path once, in `AppConstants.swift`.

use std::path::{Path, PathBuf};

fn files_under(root: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if !path.ends_with(".build") {
                files_under(&path, exts, out);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| exts.contains(&e))
        {
            out.push(path);
        }
    }
}

#[test]
fn no_source_names_the_socket_path_but_its_one_owner_per_language() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("install.sh"), root.join("release.sh")];
    files_under(&root.join("src"), &["rs"], &mut files);
    files_under(&root.join("tools"), &["sh", "py"], &mut files);
    files_under(&root.join("ui/Sources"), &["swift"], &mut files);
    assert!(
        files.len() > 60,
        "the scan lost its subject: {}",
        files.len()
    );

    let owners = [
        "src/api/socket.rs",
        "ui/Sources/AntiknobUI/Core/AppConstants.swift",
    ];
    let mut offenders = Vec::new();
    for path in files {
        let rel = path.strip_prefix(&root).unwrap().display().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            if line.contains("/tmp/antiknob.sock") {
                offenders.push(format!("{rel}:{} names the retired /tmp socket", n + 1));
            } else if line.contains("\"antiknob.sock\"") && !owners.contains(&rel.as_str()) {
                offenders.push(format!("{rel}:{} spells the socket path itself", n + 1));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}
