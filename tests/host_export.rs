//! GUI preset-export tests: the exporter writes a valid host config once
//! and refuses to overwrite. Uses a unique temp path; never touches the
//! real ~/Library location or hardware.

use antiknob::gui::state::GuiState;
use antiknob::host::HostConfig;

fn temp_export_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "antiknob-export-test-{}-{}.json",
        std::process::id(),
        tag
    ))
}

#[test]
fn export_writes_valid_config_then_refuses_overwrite() {
    let path = temp_export_path("once");
    let _ = std::fs::remove_file(&path);

    let mut state = GuiState::new();
    state.export_presets_to_host(&path);
    assert!(state.status_is_ok, "first export: {}", state.status_message);

    let text = std::fs::read_to_string(&path).expect("exported file readable");
    let cfg = HostConfig::load_json(&text);
    assert_eq!(cfg.layers.len(), 6);

    let mut state2 = GuiState::new();
    state2.export_presets_to_host(&path);
    assert!(
        !state2.status_is_ok,
        "second export must refuse, got: {}",
        state2.status_message
    );
    assert!(
        state2.status_message.contains("CLI --force")
            || state2.status_message.contains("import-presets"),
        "refusal must point at the overwrite path, got: {}",
        state2.status_message
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn default_config_path_is_shared_and_suffixed() {
    let path = antiknob::host::default_config_path().expect("HOME is set in tests");
    assert!(path.ends_with("Library/Application Support/antiknob/host.json"));
}
