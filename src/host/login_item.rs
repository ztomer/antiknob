//! Login-item support for the daemon (Phase 5).
//!
//! The daemon is a plain binary, not an app bundle, so the correct macOS
//! mechanism is a per-user LaunchAgent plist, not SMAppService (which
//! requires a bundle identity). All paths derive from an explicit base
//! directory so tests run against temp dirs, never the real home.
//! Launching itself (`launchctl bootstrap/bootout`) stays in the daemon
//! binary; this module only renders and manages the file.

use std::path::{Path, PathBuf};

/// LaunchAgent label for the daemon.
pub const AGENT_LABEL: &str = "com.antiknob.daemon";

/// `~/Library/LaunchAgents/<label>.plist` under the given home dir.
pub fn agent_plist_path(home: &Path) -> PathBuf {
    home.join(format!("Library/LaunchAgents/{}.plist", AGENT_LABEL))
}

/// Default log file the agent writes stdout/stderr to.
pub fn default_log_path(home: &Path) -> PathBuf {
    home.join("Library/Logs/antiknob-daemon.log")
}

/// Render the agent plist: run the daemon `--active` with the given
/// config at login, keep it alive, log to file.
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
            "\t\t<string>--config</string>\n",
            "\t\t<string>{}</string>\n",
            "\t</array>\n",
            "\t<key>RunAtLoad</key>\n",
            "\t<true/>\n",
            "\t<key>KeepAlive</key>\n",
            "\t<true/>\n",
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
    }

    fn io_cursor(text: &str) -> std::io::Cursor<&[u8]> {
        std::io::Cursor::new(text.as_bytes())
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
