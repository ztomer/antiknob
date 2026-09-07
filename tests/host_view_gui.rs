//! Host-view tests: reload semantics against temp files (valid, invalid,
//! missing). Never touches the real host.json, hardware, or the display.

use antiknob::gui::host_view::{
    record_host_keystroke, reload_host_view, save_host_view_to, set_host_gesture_action,
};
use antiknob::gui::state::GuiState;
use antiknob::host::{Gesture, HostAction, SeqStep};

fn temp_view_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "antiknob-hostview-{}-{}.json",
        std::process::id(),
        tag
    ))
}

const ONE_LAYER: &str = r#"{
    "layers": [
        {
            "name": "Test",
            "twistL": {"type": "scroll", "lines": 5},
            "twistR": {"type": "none"},
            "holdTwistL": {"type": "none"},
            "holdTwistR": {"type": "none"},
            "press": {"type": "aux", "key": "mute"}
        }
    ]
}"#;

#[test]
fn reload_loads_valid_config() {
    let path = temp_view_path("valid");
    std::fs::write(&path, ONE_LAYER).unwrap();
    let mut state = GuiState::new();
    reload_host_view(&mut state, &path);
    assert!(state.host_error.is_none());
    let view = state.host_view.expect("view loaded");
    assert_eq!(view.layers.len(), 1);
    assert_eq!(view.layers[0].name, "Test");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn reload_keeps_previous_view_on_invalid_file() {
    let path = temp_view_path("invalid");
    std::fs::write(&path, ONE_LAYER).unwrap();
    let mut state = GuiState::new();
    reload_host_view(&mut state, &path);
    assert!(state.host_view.is_some());

    std::fs::write(&path, "definitely not json").unwrap();
    reload_host_view(&mut state, &path);
    assert!(state.host_view.is_some(), "previous view survives");
    assert!(state.host_error.is_some(), "error is reported");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn reload_missing_file_clears_view_with_guidance() {
    let path = temp_view_path("missing");
    let _ = std::fs::remove_file(&path);
    let mut state = GuiState::new();
    reload_host_view(&mut state, &path);
    assert!(state.host_view.is_none());
    let err = state.host_error.expect("guidance reported");
    assert!(err.contains("Export") || err.contains("daemon"));
}

#[test]
fn edit_set_save_roundtrip() {
    let path = temp_view_path("edit");
    std::fs::write(&path, ONE_LAYER).unwrap();
    let mut state = GuiState::new();
    reload_host_view(&mut state, &path);

    // Direct set writes into the in-memory view.
    set_host_gesture_action(
        &mut state,
        0,
        Gesture::TwistR,
        HostAction::Scroll { lines: Some(7) },
    );
    assert_eq!(
        state.host_view.as_ref().unwrap().layers[0].twist_r,
        HostAction::Scroll { lines: Some(7) }
    );

    // Out-of-range layer is a safe no-op.
    set_host_gesture_action(&mut state, 9, Gesture::Press, HostAction::None);
    assert_eq!(state.host_view.as_ref().unwrap().layers.len(), 1);

    // Recorded keystroke lands on the selected gesture.
    state.host_edit = Some((0, Gesture::Press));
    state.host_recording = true;
    record_host_keystroke(&mut state, "cmd+s");
    assert!(!state.host_recording);
    assert_eq!(
        state.host_view.as_ref().unwrap().layers[0].press,
        HostAction::KeyChord {
            key: 1,
            mods: vec!["cmd".to_string()],
            label: "cmd+s".to_string(),
        }
    );

    // Save persists; reload reads back identically.
    let out = temp_view_path("edit-out");
    let _ = std::fs::remove_file(&out);
    save_host_view_to(&mut state, &out);
    assert!(state.status_is_ok, "save: {}", state.status_message);
    let mut reloaded = GuiState::new();
    reload_host_view(&mut reloaded, &out);
    assert_eq!(reloaded.host_view, state.host_view);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn recorded_keystroke_appends_inside_sequences() {
    let path = temp_view_path("seq");
    std::fs::write(&path, ONE_LAYER).unwrap();
    let mut state = GuiState::new();
    reload_host_view(&mut state, &path);

    // Seed a one-step sequence, then record into it.
    set_host_gesture_action(
        &mut state,
        0,
        Gesture::Press,
        HostAction::Sequence {
            steps: vec![SeqStep {
                key: Some(8),
                mods: vec![],
                label: "c".to_string(),
                delay_ms: None,
            }],
        },
    );
    state.host_edit = Some((0, Gesture::Press));
    state.host_recording = true;
    record_host_keystroke(&mut state, "cmd+v");
    let steps = match &state.host_view.as_ref().unwrap().layers[0].press {
        HostAction::Sequence { steps } => steps.clone(),
        other => panic!("expected sequence, got {:?}", other),
    };
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[1].label, "cmd+v");

    // Non-keystrokes are refused, sequence untouched.
    state.host_edit = Some((0, Gesture::Press));
    state.host_recording = true;
    record_host_keystroke(&mut state, "mute");
    assert!(!state.status_is_ok);
    let steps = match &state.host_view.as_ref().unwrap().layers[0].press {
        HostAction::Sequence { steps } => steps.clone(),
        other => panic!("expected sequence, got {:?}", other),
    };
    assert_eq!(steps.len(), 2);

    let _ = std::fs::remove_file(&path);
}
