use antiknob::gui::presets::{apply_preset, ALL_PRESETS};
use antiknob::gui::state::{GuiState, KnobTarget, Selection};

#[test]
fn test_gui_state_initialization() {
    let state = GuiState::new();
    assert_eq!(state.active_layer, 0);
    assert!(!state.is_recording);
    assert!(state.status_is_ok);
}

#[test]
fn test_gui_state_target_selection_and_recording() {
    let mut state = GuiState::new();

    // 1. Select Knob CCW
    state.selected = Selection::Knob {
        index: 0,
        target: KnobTarget::Ccw,
    };
    state.record_key("cmd+opt+left");
    assert_eq!(
        state.get_binding(state.selected),
        Some("cmd+opt+left".to_string())
    );
    assert_eq!(state.recorded_shortcut, Some("cmd+opt+left".to_string()));

    // 2. Select Button
    state.selected = Selection::Button { row: 0, col: 0 };
    state.record_key("shift+cmd+4");
    assert_eq!(
        state.get_binding(state.selected),
        Some("shift+cmd+4".to_string())
    );
}

#[test]
fn test_gui_state_presets_application() {
    let mut state = GuiState::new();

    // Test Video Scrubbing Preset
    let video_preset = ALL_PRESETS
        .iter()
        .find(|p| p.id == "video_scrubbing")
        .unwrap();
    apply_preset(video_preset, &mut state);

    let knob_ccw = state.get_binding(Selection::Knob {
        index: 0,
        target: KnobTarget::Ccw,
    });
    let knob_cw = state.get_binding(Selection::Knob {
        index: 0,
        target: KnobTarget::Cw,
    });
    let knob_press = state.get_binding(Selection::Knob {
        index: 0,
        target: KnobTarget::Press,
    });
    let button = state.get_binding(Selection::Button { row: 0, col: 0 });

    assert_eq!(knob_ccw, Some("left".to_string()));
    assert_eq!(knob_cw, Some("right".to_string()));
    assert_eq!(knob_press, Some("space".to_string()));
    assert_eq!(button, Some("cmd+b".to_string()));
    assert_eq!(state.led_color, "red");

    // Test Digital Art Preset
    let art_preset = ALL_PRESETS.iter().find(|p| p.id == "digital_art").unwrap();
    apply_preset(art_preset, &mut state);

    assert_eq!(
        state.get_binding(Selection::Knob {
            index: 0,
            target: KnobTarget::Ccw,
        }),
        Some("leftbracket".to_string())
    );
    assert_eq!(
        state.get_binding(Selection::Knob {
            index: 0,
            target: KnobTarget::Press,
        }),
        Some("cmd+z".to_string())
    );
    assert_eq!(
        state.get_binding(Selection::Button { row: 0, col: 0 }),
        Some("b".to_string())
    );
    assert_eq!(state.led_color, "cyan");
}

#[test]
fn test_gui_state_clear_operations() {
    let mut state = GuiState::new();
    state.set_binding("cmd+c");
    assert_eq!(state.get_binding(state.selected), Some("cmd+c".to_string()));

    state.clear_current();
    assert_eq!(state.get_binding(state.selected), None);

    state.clear_all_on_layer();
    assert_eq!(
        state.get_binding(Selection::Knob {
            index: 0,
            target: KnobTarget::Press,
        }),
        None
    );
}

#[test]
fn test_gui_state_profile_save_and_reload() {
    let temp_dir = std::env::temp_dir().join("antiknob_gui_e2e_profile");
    let profile_path = temp_dir.join("gui_profile.yaml");

    let mut state = GuiState::new();
    state.selected = Selection::Button { row: 0, col: 0 };
    state.set_binding("ctrl+shift+esc");
    state.save_profile(profile_path.to_str().unwrap()).unwrap();

    let mut new_state = GuiState::new();
    new_state
        .load_profile(profile_path.to_str().unwrap())
        .unwrap();
    assert_eq!(
        new_state.get_binding(Selection::Button { row: 0, col: 0 }),
        Some("ctrl+shift+esc".to_string())
    );

    let _ = std::fs::remove_file(profile_path);
    let _ = std::fs::remove_dir(temp_dir);
}
