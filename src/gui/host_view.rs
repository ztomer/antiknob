//! Host-layer viewer for the GUI (Phase 4/5).
//!
//! Read-only surface over the daemon's host.json: shows layers, gestures,
//! and actions, and surfaces parse errors instead of failing silently.
//! Lives outside `state.rs` to respect the file-length gate; editing
//! (sequence editor, app pickers) builds on this view next.

use crate::gui::state::GuiState;
use crate::host::{describe_host_action, AuxKey, Gesture, HostAction, HostConfig};
use eframe::egui::{self, Ui};

/// Store a converted device-style keystroke ("cmd+c") into the selected
/// host gesture. When the gesture holds a Sequence the chord appends as a
/// new step (only plain keystrokes qualify); otherwise it replaces the
/// action. Ends the recording session either way.
pub fn record_host_keystroke(state: &mut GuiState, key_str: &str) {
    state.host_recording = false;
    let Some((layer_idx, gesture)) = state.host_edit else {
        return;
    };
    let current = state
        .host_view
        .as_ref()
        .and_then(|cfg| cfg.layers.get(layer_idx))
        .map(|layer| layer.action(gesture).clone());
    if let Some(HostAction::Sequence { mut steps }) = current {
        match crate::host::migrate::device_str_to_host_action(key_str) {
            HostAction::KeyChord { key, mods, label } => {
                steps.push(crate::host::SeqStep {
                    key: Some(key),
                    mods,
                    label,
                    delay_ms: None,
                });
                set_host_gesture_action(state, layer_idx, gesture, HostAction::Sequence { steps });
                state.status_message = format!("Appended '{}' to sequence", key_str);
                state.status_is_ok = true;
            }
            _ => {
                state.status_message =
                    format!("Only keystrokes go into sequences ('{}' ignored)", key_str);
                state.status_is_ok = false;
            }
        }
        return;
    }
    let action = crate::host::migrate::device_str_to_host_action(key_str);
    set_host_gesture_action(state, layer_idx, gesture, action);
    state.status_message = format!("Recorded '{}' to host gesture", key_str);
    state.status_is_ok = true;
}

/// Write one action into the in-memory host view (bounds-checked no-op
/// when the layer index is stale, e.g. after an external reload).
pub fn set_host_gesture_action(
    state: &mut GuiState,
    layer_idx: usize,
    gesture: Gesture,
    action: HostAction,
) {
    let Some(cfg) = state.host_view.as_mut() else {
        return;
    };
    let Some(layer) = cfg.layers.get_mut(layer_idx) else {
        return;
    };
    match gesture {
        Gesture::TwistL => layer.twist_l = action,
        Gesture::TwistR => layer.twist_r = action,
        Gesture::HoldTwistL => layer.hold_twist_l = action,
        Gesture::HoldTwistR => layer.hold_twist_r = action,
        Gesture::Press => layer.press = action,
    }
}

/// Persist the in-memory host view to the daemon config path. The running
/// daemon picks it up through instant-apply within a tick.
pub fn save_host_view(state: &mut GuiState) {
    let path = match crate::host::default_config_path() {
        Ok(p) => p,
        Err(e) => {
            state.status_message = format!("Save failed (HOME): {}", e);
            state.status_is_ok = false;
            return;
        }
    };
    save_host_view_to(state, &path);
}

/// Persist the in-memory host view to an explicit path (the testable
/// core; production passes the default config path).
pub fn save_host_view_to(state: &mut GuiState, path: &std::path::Path) {
    let Some(cfg) = state.host_view.as_ref() else {
        state.status_message = "Nothing to save: no host view loaded.".to_string();
        state.status_is_ok = false;
        return;
    };
    match std::fs::write(path, cfg.to_json_pretty()) {
        Ok(()) => {
            state.status_message = format!("Saved host config to {}.", path.display());
            state.status_is_ok = true;
        }
        Err(e) => {
            state.status_message = format!("Save failed: {}", e);
            state.status_is_ok = false;
        }
    }
}

/// Reload the viewed host config from `path`. Missing file clears the
/// view with guidance; unparseable file keeps the previous view and
/// reports the error (mirrors the daemon's last-good rule).
pub fn reload_host_view(state: &mut GuiState, path: &std::path::Path) {
    match std::fs::read_to_string(path) {
        Err(_) => {
            state.host_view = None;
            state.host_error = Some(format!(
                "No host config at {}. Export presets or run the daemon once.",
                path.display()
            ));
        }
        Ok(text) => match HostConfig::try_load_json(&text) {
            Some(cfg) => {
                state.host_view = Some(cfg);
                state.host_error = None;
            }
            None => {
                state.host_error = Some(format!(
                    "Host config at {} is invalid; showing previous view.",
                    path.display()
                ));
            }
        },
    }
}

const GESTURE_ROWS: [(Gesture, &str); 5] = [
    (Gesture::TwistL, "Twist left"),
    (Gesture::HoldTwistL, "Hold + twist left"),
    (Gesture::Press, "Press"),
    (Gesture::HoldTwistR, "Hold + twist right"),
    (Gesture::TwistR, "Twist right"),
];

pub fn render_host_view(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Host Layers (daemon config)");
    let path = crate::host::default_config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "<HOME unset>".to_string());
    ui.small(format!("Source: {}", path));
    ui.separator();

    if ui.button("Reload Host Config").clicked() {
        if let Ok(path) = crate::host::default_config_path() {
            reload_host_view(state, &path);
            state.status_message = "Reloaded host view.".to_string();
            state.status_is_ok = true;
        }
    }
    ui.add_space(6.0);

    if let Some(err) = state.host_error.clone() {
        ui.colored_label(egui::Color32::from_rgb(230, 126, 34), err);
        ui.add_space(4.0);
    }

    match state.host_view.clone() {
        None => {
            ui.label("No host layers to show. Export the presets to create a starter config:");
            ui.add_space(4.0);
            if ui.button("Export Presets to Host Config").clicked() {
                if let Ok(path) = crate::host::default_config_path() {
                    state.export_presets_to_host(&path);
                    reload_host_view(state, &path);
                }
            }
        }
        Some(cfg) => {
            if ui.button("Save Host Config").clicked() {
                save_host_view(state);
            }
            ui.small("Edits apply to the daemon within a tick through instant-apply.");
            ui.add_space(4.0);
            render_editor(ui, state);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, layer) in cfg.layers.iter().enumerate() {
                    ui.group(|ui| {
                        ui.strong(format!("Layer {} — {}", i, layer.name));
                        for (gesture, label) in GESTURE_ROWS {
                            let action = layer.action(gesture);
                            ui.horizontal(|ui| {
                                ui.label(format!("{:<20}", label));
                                ui.colored_label(
                                    egui::Color32::from_rgb(0, 180, 255),
                                    describe_host_action(action),
                                );
                                if ui.small_button("Edit").clicked() {
                                    state.host_edit = Some((i, gesture));
                                    state.host_recording = false;
                                }
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        }
    }
}

const AUX_BUTTONS: [(AuxKey, &str); 8] = [
    (AuxKey::VolumeUp, "Vol +"),
    (AuxKey::VolumeDown, "Vol -"),
    (AuxKey::Mute, "Mute"),
    (AuxKey::PlayPause, "Play"),
    (AuxKey::Next, "Next"),
    (AuxKey::Previous, "Prev"),
    (AuxKey::BrightnessUp, "Bright +"),
    (AuxKey::BrightnessDown, "Bright -"),
];

/// In-memory editor for the selected gesture. Covers None, Scroll, Media,
/// recorded Keystrokes, mouse clicks, app launch/quit (picker), and
/// URL/path open (text fields). Sequences stay hand-edited JSON for now.
fn render_editor(ui: &mut Ui, state: &mut GuiState) {
    let Some((layer_idx, gesture)) = state.host_edit else {
        return;
    };
    let current = state
        .host_view
        .as_ref()
        .and_then(|cfg| cfg.layers.get(layer_idx))
        .map(|layer| layer.action(gesture).clone());
    let Some(current) = current else {
        state.host_edit = None;
        return;
    };
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.strong("Editing gesture");
            if ui.small_button("Done").clicked() {
                state.host_edit = None;
                state.host_recording = false;
            }
        });
        ui.horizontal(|ui| {
            if ui.button("None").clicked() {
                set_host_gesture_action(state, layer_idx, gesture, HostAction::None);
            }
            if ui.button("Scroll").clicked() {
                let lines = match current {
                    HostAction::Scroll { lines } => lines.unwrap_or(3),
                    _ => 3,
                };
                set_host_gesture_action(
                    state,
                    layer_idx,
                    gesture,
                    HostAction::Scroll { lines: Some(lines) },
                );
            }
            if ui.button("Media").clicked() {
                set_host_gesture_action(
                    state,
                    layer_idx,
                    gesture,
                    HostAction::Aux {
                        key: AuxKey::VolumeUp,
                    },
                );
            }
            if ui.button("Keystroke").clicked() {
                state.host_recording = true;
            }
            if ui.button("Sequence").clicked() {
                let steps = match &current {
                    HostAction::Sequence { steps } => steps.clone(),
                    _ => Vec::new(),
                };
                set_host_gesture_action(state, layer_idx, gesture, HostAction::Sequence { steps });
            }
            if ui.button("More...").clicked() {
                ui.data_mut(|d| {
                    let flag = d.get_temp_mut_or(egui::Id::new("host_more_open"), false);
                    *flag = !*flag;
                });
            }
        });
        let more_open = ui
            .data(|d| d.get_temp::<bool>(egui::Id::new("host_more_open")))
            .unwrap_or(false);
        if more_open {
            crate::gui::host_more::render_more_actions(ui, state, layer_idx, gesture);
        }

        match current {
            HostAction::Scroll { lines } => {
                let mut value = lines.unwrap_or(3);
                ui.horizontal(|ui| {
                    ui.label("Lines per detent:");
                    if ui
                        .add(egui::DragValue::new(&mut value).range(-30..=30))
                        .changed()
                    {
                        set_host_gesture_action(
                            state,
                            layer_idx,
                            gesture,
                            HostAction::Scroll { lines: Some(value) },
                        );
                    }
                });
            }
            HostAction::Aux { .. } => {
                ui.horizontal_wrapped(|ui| {
                    for (key, label) in AUX_BUTTONS {
                        if ui.button(label).clicked() {
                            set_host_gesture_action(
                                state,
                                layer_idx,
                                gesture,
                                HostAction::Aux { key },
                            );
                        }
                    }
                });
            }
            HostAction::Sequence { .. } => {
                crate::gui::seq_editor::render_sequence_editor(ui, state, layer_idx, gesture);
            }
            _ => {}
        }

        if state.host_recording {
            ui.colored_label(
                egui::Color32::from_rgb(255, 200, 80),
                "Press keys now to record the keystroke...",
            );
        }
    });
    ui.add_space(4.0);
}
