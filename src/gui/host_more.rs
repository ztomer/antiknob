//! Launch/quit/open/mouse actions for the host gesture editor.
//!
//! Split from host_view.rs to respect the file-length gate. Picker state
//! (filter text, scanned list) lives in egui temp data so GuiState stays
//! untouched.

use crate::gui::host_view::set_host_gesture_action;
use crate::gui::state::GuiState;
use crate::host::{Gesture, HostAction};
use eframe::egui::{self, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerKind {
    Launch,
    Quit,
}

#[derive(Clone, Default)]
struct AppPicker {
    open_for: Option<PickerKind>,
    filter: String,
    apps: Vec<crate::apps::AppInfo>,
    quit_force: bool,
}

const PICKER_ID: &str = "host_app_picker";
const URL_ID: &str = "host_open_url";
const PATH_ID: &str = "host_open_path";

/// Launch/quit app picker, URL/path fields, and mouse buttons. Picker
/// state (filter text, scanned list) lives in egui temp data so GuiState
/// stays untouched; only id-bearing apps are listed since both actions
/// address apps by bundle identifier.
pub(crate) fn render_more_actions(
    ui: &mut Ui,
    state: &mut GuiState,
    layer_idx: usize,
    gesture: Gesture,
) {
    ui.group(|ui| {
        ui.strong("More actions");
        ui.horizontal(|ui| {
            if ui.button("Launch App...").clicked() {
                open_picker(ui, PickerKind::Launch);
            }
            if ui.button("Quit App...").clicked() {
                open_picker(ui, PickerKind::Quit);
            }
            for (button, label) in [
                (crate::host::MouseButton::Left, "Left Click"),
                (crate::host::MouseButton::Right, "Right Click"),
                (crate::host::MouseButton::Middle, "Middle Click"),
            ] {
                if ui.button(label).clicked() {
                    set_host_gesture_action(
                        state,
                        layer_idx,
                        gesture,
                        HostAction::MouseClick { button },
                    );
                }
            }
        });

        render_app_picker(ui, state, layer_idx, gesture);
        render_open_field(ui, state, layer_idx, gesture, URL_ID, "Website URL:", false);
        render_open_field(ui, state, layer_idx, gesture, PATH_ID, "File/folder:", true);
    });
}

fn open_picker(ui: &mut Ui, kind: PickerKind) {
    let apps: Vec<crate::apps::AppInfo> =
        crate::apps::scan_app_dirs(&crate::apps::default_app_dirs())
            .into_iter()
            .filter(|a| a.bundle_id.is_some())
            .collect();
    ui.data_mut(|d| {
        let picker = d.get_temp_mut_or(egui::Id::new(PICKER_ID), AppPicker::default());
        picker.open_for = Some(kind);
        picker.filter.clear();
        picker.apps = apps;
    });
}

fn render_app_picker(ui: &mut Ui, state: &mut GuiState, layer_idx: usize, gesture: Gesture) {
    let picker = ui
        .data(|d| d.get_temp::<AppPicker>(egui::Id::new(PICKER_ID)))
        .unwrap_or_default();
    let Some(kind) = picker.open_for else {
        return;
    };
    ui.separator();
    let mut closed = false;
    ui.horizontal(|ui| {
        ui.label(match kind {
            PickerKind::Launch => "Launch which app?",
            PickerKind::Quit => "Quit which app?",
        });
        if ui.small_button("Close").clicked() {
            ui.data_mut(|d| {
                d.get_temp_mut_or(egui::Id::new(PICKER_ID), AppPicker::default())
                    .open_for = None;
            });
            closed = true;
        }
    });
    if closed {
        return;
    }
    let mut filter = picker.filter.clone();
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.text_edit_singleline(&mut filter);
    });
    if filter != picker.filter {
        ui.data_mut(|d| {
            d.get_temp_mut_or(egui::Id::new(PICKER_ID), AppPicker::default())
                .filter = filter.clone();
        });
    }
    if kind == PickerKind::Quit {
        let mut force = picker.quit_force;
        ui.checkbox(&mut force, "Force quit");
        if force != picker.quit_force {
            ui.data_mut(|d| {
                d.get_temp_mut_or(egui::Id::new(PICKER_ID), AppPicker::default())
                    .quit_force = force;
            });
        }
    }
    let needle = filter.to_lowercase();
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            for app in picker
                .apps
                .iter()
                .filter(|a| {
                    needle.is_empty()
                        || a.name.to_lowercase().contains(&needle)
                        || a.bundle_id
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&needle)
                })
                .take(60)
            {
                let id = app.bundle_id.clone().unwrap_or_default();
                if ui.button(format!("{}  ({})", app.name, id)).clicked() {
                    let action = match kind {
                        PickerKind::Launch => HostAction::LaunchApp { bundle_id: id },
                        PickerKind::Quit => HostAction::QuitApp {
                            bundle_id: id,
                            force: picker.quit_force,
                        },
                    };
                    set_host_gesture_action(state, layer_idx, gesture, action);
                    ui.data_mut(|d| {
                        d.get_temp_mut_or(egui::Id::new(PICKER_ID), AppPicker::default())
                            .open_for = None;
                    });
                    break;
                }
            }
        });
}

fn render_open_field(
    ui: &mut Ui,
    state: &mut GuiState,
    layer_idx: usize,
    gesture: Gesture,
    field_id: &str,
    label: &str,
    is_path: bool,
) {
    let mut value = ui
        .data(|d| d.get_temp::<String>(egui::Id::new(field_id)))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(&mut value);
        if ui.button("Set").clicked() && !value.trim().is_empty() {
            let text = value.trim().to_string();
            let action = if is_path {
                HostAction::OpenPath { path: text }
            } else {
                HostAction::OpenUrl { url: text }
            };
            set_host_gesture_action(state, layer_idx, gesture, action);
            value.clear();
        }
    });
    ui.data_mut(|d| {
        d.insert_temp(egui::Id::new(field_id), value);
    });
}
