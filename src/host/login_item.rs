//! Login-item support for the daemon (Phase 5).
//!
//! The daemon is a plain binary, not an app bundle, so the correct macOS
//! mechanism is a per-user LaunchAgent plist, not SMAppService (which
//! requires a bundle identity). All paths derive from an explicit base
//! directory so tests run against temp dirs, never the real home.
//! Launching itself (`launchctl bootstrap/bootout`) stays in the daemon
//! binary; this module only renders and manages the file.

use std::path::{Path, PathBuf};

/// LaunchAgent label for the daemon. Defined once in the policy map: the
/// installer, the uninstaller, and the target strings must all name the
/// same label, and three spellings of it is how a label stops matching
/// itself.
pub use crate::policy::LOGIN_AGENT_LABEL as AGENT_LABEL;

/// `~/Library/LaunchAgents/<label>.plist` under the given home dir.
pub fn agent_plist_path(home: &Path) -> PathBuf {
    home.join(format!("Library/LaunchAgents/{}.plist", AGENT_LABEL))
}

/// Default log file the agent writes stdout/stderr to.
pub fn default_log_path(home: &Path) -> PathBuf {
    home.join("Library/Logs/antiknob-daemon.log")
}

/// Render the agent plist: run the daemon `--active` with the given
/// config at login, restart it if it crashes, log to file.
///
/// `KeepAlive` is `SuccessfulExit: false`, not a bare `true`. A bare `true`
/// means launchd relaunches the daemon within seconds of ANY exit, including
/// the deliberate one behind the menu bar's Quit -- so Quit did not quit,
/// `kill` did not kill, and the process looked wedged when it was being
/// resurrected. Restarting a crash is the job; overriding the user is not.
pub fn render_agent_plist(executable: &Path, config: &Path, log_path: &Path) -> String {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
            "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
            "<plist version=\"1.0\">\n",
            "<dict>\n",
            "\t<key>Label</key>\n",
            "\t<string>{}</string>\n",
            "\t<key>ProgramArguments</key>\n",
            "\t<array>\n",
            "\t\t<string>{}</string>\n",
            "\t\t<string>--active</string>\n",
            "\t\t<string>--no-tray</string>\n",
            "\t\t<string>--config</string>\n",
            "\t\t<string>{}</string>\n",
            "\t</array>\n",
            "\t<key>RunAtLoad</key>\n",
            "\t<true/>\n",
            "\t<key>KeepAlive</key>\n",
            "\t<dict>\n",
            "\t\t<key>SuccessfulExit</key>\n",
            "\t\t<false/>\n",
            "\t</dict>\n",
            "\t<key>StandardOutPath</key>\n",
            "\t<string>{}</string>\n",
            "\t<key>StandardErrorPath</key>\n",
            "\t<string>{}</string>\n",
            "</dict>\n",
            "</plist>\n",
        ),
        AGENT_LABEL,
        executable.display(),
        config.display(),
        log_path.display(),
        log_path.display(),
    )
}

/// Write the agent plist (creating parent dirs). Overwrites any previous
/// install of the same label: install is idempotent.
pub fn install(home: &Path, executable: &Path, config: &Path) -> anyhow::Result<PathBuf> {
    let plist_path = agent_plist_path(home);
    if let Some(dir) = plist_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let log_path = default_log_path(home);
    if let Some(dir) = log_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        &plist_path,
        render_agent_plist(executable, config, &log_path),
    )?;
    Ok(plist_path)
}

/// The bundled twin of a loose `bin/antiknob-daemon`, by path alone.
///
/// The daemon is installed twice from the same bits: as
/// `<root>/bin/antiknob-daemon` and as
/// `<root>/AntiknobDaemon.app/Contents/MacOS/AntiknobDaemon`. Pure, so the
/// mapping is testable without either file existing.
pub fn bundled_twin(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    if dir.file_name()? != "bin" || exe.file_name()? != "antiknob-daemon" {
        return None;
    }
    Some(
        dir.parent()?
            .join("AntiknobDaemon.app/Contents/MacOS/AntiknobDaemon"),
    )
}

/// Which executable the login item should actually start.
///
/// TCC identifies a process by its code signature, so the loose binary and
/// the one inside `AntiknobDaemon.app` are two different programs to System
/// Settings -- and only the bundle is listed there, by name, with an icon.
/// The Accessibility pane's `+` button will not even accept an executable
/// buried under `Contents/MacOS`. So a login item that starts the loose
/// binary can never be granted by adding the app, which is the only thing
/// the user can add: they grant Antiknob, the daemon stays deaf, and nothing
/// says why. Start the bundle whenever it is installed.
pub fn preferred_executable(exe: &Path) -> PathBuf {
    match bundled_twin(exe) {
        Some(bundled) if bundled.is_file() => bundled,
        _ => exe.to_path_buf(),
    }
}

/// Remove the agent plist. Returns true when a previous install existed.
pub fn uninstall(home: &Path) -> anyhow::Result<bool> {
    let plist_path = agent_plist_path(home);
    if !plist_path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&plist_path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("antiknob-home-{}-{}", std::process::id(), tag))
    }

    #[test]
    fn plist_renders_label_args_and_logging() {
        let text = render_agent_plist(
            Path::new("/Applications/Antiknob/bin/antiknob-daemon"),
            Path::new("/Users/me/Library/Application Support/antiknob/host.json"),
            Path::new("/Users/me/Library/Logs/antiknob-daemon.log"),
        );
        assert!(text.contains("<string>com.antiknob.daemon</string>"));
        assert!(text.contains("<string>--active</string>"));
        assert!(text.contains("antiknob-daemon</string>"));
        assert!(text.contains("<key>KeepAlive</key>"));
        assert!(text.contains("antiknob-daemon.log</string>"));
        // Well-formed XML plist the loader will accept.
        let value = plist::Value::from_reader_xml(io_cursor(&text)).expect("valid plist xml");
        let dict = value.as_dictionary().expect("plist dict");
        assert_eq!(
            dict.get("Label").and_then(|v| v.as_string()),
            Some("com.antiknob.daemon")
        );
        // A crash is restarted; a clean exit (menu bar Quit) is honoured.
        assert_eq!(
            dict.get("KeepAlive")
                .and_then(|v| v.as_dictionary())
                .and_then(|d| d.get("SuccessfulExit"))
                .and_then(|v| v.as_boolean()),
            Some(false)
        );
    }

    fn io_cursor(text: &str) -> std::io::Cursor<&[u8]> {
        std::io::Cursor::new(text.as_bytes())
    }

    #[test]
    fn a_loose_daemon_maps_to_the_bundled_one() {
        assert_eq!(
            bundled_twin(Path::new("/Applications/Antiknob/bin/antiknob-daemon")),
            Some(PathBuf::from(
                "/Applications/Antiknob/AntiknobDaemon.app/Contents/MacOS/AntiknobDaemon"
            ))
        );
        // Already the bundled executable, or somewhere else entirely.
        assert_eq!(
            bundled_twin(Path::new(
                "/Applications/Antiknob/AntiknobDaemon.app/Contents/MacOS/AntiknobDaemon"
            )),
            None
        );
        assert_eq!(bundled_twin(Path::new("/tmp/antiknob-daemon")), None);
    }

    #[test]
    fn the_login_item_prefers_the_bundle_only_when_it_is_there() {
        let root = temp_home("preferred");
        let _ = std::fs::remove_dir_all(&root);
        let loose = root.join("bin/antiknob-daemon");
        std::fs::create_dir_all(loose.parent().unwrap()).unwrap();
        std::fs::write(
            &loose,
            b"#!/bin/sh
",
        )
        .unwrap();

        // No bundle installed yet: the loose binary is all there is.
        assert_eq!(preferred_executable(&loose), loose);

        let bundled = root.join("AntiknobDaemon.app/Contents/MacOS/AntiknobDaemon");
        std::fs::create_dir_all(bundled.parent().unwrap()).unwrap();
        std::fs::write(
            &bundled,
            b"#!/bin/sh
",
        )
        .unwrap();
        assert_eq!(preferred_executable(&loose), bundled);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_then_uninstall_roundtrip() {
        let home = temp_home("login");
        let _ = std::fs::remove_dir_all(&home);
        let exe = Path::new("/tmp/antiknob-daemon");
        let cfg = home.join("host.json");

        let plist_path = install(&home, exe, &cfg).expect("install");
        assert!(plist_path.exists());
        assert_eq!(plist_path, agent_plist_path(&home));
        let body = std::fs::read_to_string(&plist_path).unwrap();
        assert!(body.contains("/tmp/antiknob-daemon"));

        assert!(uninstall(&home).expect("uninstall reports previous install"));
        assert!(!plist_path.exists());
        assert!(!uninstall(&home).expect("second uninstall reports nothing to do"));

        let _ = std::fs::remove_dir_all(&home);
    }
}
