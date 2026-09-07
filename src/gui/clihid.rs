//! GUI-to-hardware bridge via the `antiknob` CLI subprocess.
//!
//! The GUI process must never call hidapi in-process: hid_enumerate pumps
//! the calling thread's CFRunLoop, which traps on worker threads (SIGTRAP)
//! and aborts reentrantly inside the running event loop (SIGABRT) — both
//! observed on macOS 26. The CLI binary runs on a plain main thread with
//! no runloop, so every hardware operation here shells out to it and reads
//! back exit codes plus machine-readable output.

use crate::device::DeviceMatch;
use std::path::{Path, PathBuf};
use std::process::Command;

/// CLI binary file name in every layout (dev target dir, install bin dir).
const CLI_NAME: &str = "antiknob";

/// Locate the CLI binary for the given current-executable directory and
/// PATH lookup. Layouts, in order: beside the running executable (cargo
/// `target/debug|release`), `../../bin` above an `.app/Contents/MacOS`
/// bundle executable, then `PATH`.
///
/// Critical guard: a candidate that canonicalizes to the running
/// executable itself is skipped. On case-insensitive filesystems the
/// beside-exe probe `.../MacOS/antiknob` resolves to the GUI binary
/// `.../MacOS/Antiknob`; exec'ing it relaunches the GUI, whose startup
/// scan spawns another copy — an exponential fork bomb (observed live).
pub fn resolve_in(
    exe_dir: &Path,
    own_exe: Option<&Path>,
    path_lookup: impl Fn(&str) -> Option<PathBuf>,
) -> Result<PathBuf, String> {
    let own_canon = own_exe.and_then(|p| std::fs::canonicalize(p).ok());
    let accept = |candidate: PathBuf| -> Option<PathBuf> {
        if !is_executable(&candidate) {
            return None;
        }
        // Never exec ourselves (covers case-insensitive aliasing).
        if let (Some(own), Ok(canon)) = (own_canon.as_ref(), std::fs::canonicalize(&candidate)) {
            if &canon == own {
                return None;
            }
        }
        Some(candidate)
    };

    if let Some(hit) = accept(exe_dir.join(CLI_NAME)) {
        return Ok(hit);
    }
    // App-bundle layout: <root>/Antiknob.app/Contents/MacOS/<exe> looks in
    // <root>/bin. ancestors()[3] of the MacOS dir is <root>.
    if let Some(hit) = exe_dir
        .ancestors()
        .nth(3)
        .map(|root| root.join("bin").join(CLI_NAME))
        .and_then(accept)
    {
        return Ok(hit);
    }
    path_lookup(CLI_NAME)
        .filter(|p| accept(p.clone()).is_some())
        .ok_or_else(|| "antiknob CLI not found (reinstall the app)".to_string())
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

/// Resolve via the real process environment.
pub fn resolve() -> Result<PathBuf, String> {
    let current = std::env::current_exe().ok();
    let exe_dir = current
        .as_ref()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    resolve_in(&exe_dir, current.as_deref(), |name| {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .find(|p| is_executable(p))
        })
    })
}

fn run_cli(exe: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(exe)
        .args(args)
        .output()
        .map_err(|e| format!("failed to launch antiknob CLI: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        let tail = stdout
            .lines()
            .chain(stderr.lines())
            .last()
            .unwrap_or("unknown CLI error");
        Err(tail.to_string())
    }
}

/// Parse `status --json` output. Pure for testability.
pub fn parse_status_json(text: &str) -> Result<Vec<DeviceMatch>, String> {
    serde_json::from_str(text).map_err(|e| format!("bad status JSON: {}", e))
}

pub struct CliHid {
    exe: PathBuf,
}

impl CliHid {
    pub fn connect() -> Result<Self, String> {
        Ok(Self { exe: resolve()? })
    }

    pub fn status(&self) -> Result<Vec<DeviceMatch>, String> {
        let out = run_cli(&self.exe, &["status", "--json"])?;
        parse_status_json(&out)
    }

    pub fn upload_file(&self, yaml: &Path) -> Result<String, String> {
        self.upload_layer(yaml, None)
    }

    /// Flash a config file, optionally restricted to one device layer.
    pub fn upload_layer(&self, yaml: &Path, layer: Option<u8>) -> Result<String, String> {
        let mut args: Vec<String> = vec!["upload".to_string(), yaml.to_string_lossy().into_owned()];
        if let Some(l) = layer {
            args.push("--layer".to_string());
            args.push(l.to_string());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = run_cli(&self.exe, &arg_refs)?;
        Ok(out.lines().last().unwrap_or("upload complete").to_string())
    }

    pub fn led(&self, layer: u8, spec: &str) -> Result<(), String> {
        let layer_arg = layer.to_string();
        let mut args: Vec<&str> = vec!["led", &layer_arg];
        args.extend(spec.split_whitespace());
        run_cli(&self.exe, &args)?;
        Ok(())
    }

    pub fn bind_slots(&self) -> Result<String, String> {
        run_cli(&self.exe, &["bind-slots"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn make_exe(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    fn temp_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("antiknob-cli-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn resolve_prefers_beside_then_bundle_then_path() {
        let root = temp_root("resolve");
        let exe_dir = root.join("debug");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let beside = make_exe(&exe_dir, "antiknob");
        let elsewhere_dir = root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere_dir).unwrap();
        let elsewhere = make_exe(&elsewhere_dir, "antiknob");
        let found = resolve_in(&exe_dir, None, |_| Some(elsewhere.clone())).unwrap();
        assert_eq!(found, beside);

        std::fs::remove_file(&beside).unwrap();
        let bundle_mac = root.join("Antiknob.app/Contents/MacOS");
        std::fs::create_dir_all(&bundle_mac).unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let _ = make_exe(&root.join("bin"), "antiknob");
        let found = resolve_in(&bundle_mac, None, |_| None).unwrap();
        assert_eq!(found, root.join("bin/antiknob"));

        let found = resolve_in(&root.join("empty"), None, |_| Some(elsewhere.clone())).unwrap();
        assert_eq!(found, elsewhere);

        assert!(resolve_in(&root.join("empty"), None, |_| None).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_never_returns_the_running_executable() {
        // Fork-bomb guard: a candidate that IS our own binary (same file,
        // e.g. via case-insensitive aliasing) must be skipped in every
        // layout position.
        let root = temp_root("selfexec");
        let exe_dir = root.join("MacOS");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let gui_binary = make_exe(&exe_dir, "Antiknob");
        // Same file reachable as the beside-exe probe name: on this
        // machine's case-insensitive disk the alias exists by
        // construction; elsewhere a hardlink stands in for it.
        let _ = std::fs::hard_link(&gui_binary, exe_dir.join("antiknob"));
        let fallback_dir = root.join("fallback");
        std::fs::create_dir_all(&fallback_dir).unwrap();
        let fallback = make_exe(&fallback_dir, "antiknob");

        // Without the guard this would return the GUI itself; with it, the
        // lookup falls through to PATH.
        let found = resolve_in(&exe_dir, Some(&gui_binary), |_| Some(fallback.clone())).unwrap();
        assert_eq!(found, fallback);

        // And with no fallback it errors instead of returning self.
        assert!(resolve_in(&exe_dir, Some(&gui_binary), |_| None).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn status_json_parses_and_rejects_garbage() {
        let text = r#"[{"vendor_id": 20812, "product_id": 34960, "name": "n", "serial_number": null, "path": "p", "usage_page": 65280, "usage": 1}]"#;
        let devs = parse_status_json(text).unwrap();
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].vendor_id, 20812);
        assert!(parse_status_json("not json").is_err());
        assert!(parse_status_json("[ ==> ] human text").is_err());
    }
}
