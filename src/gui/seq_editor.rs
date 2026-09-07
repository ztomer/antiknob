//! Sequence step editor for host gestures (Phase 4, final action type).
//!
//! Edits `HostAction::Sequence` steps in memory: keystroke steps (recorded
//! through the shared recorder, appended by `record_host_keystroke`),
//! wait steps with millisecond delays, delete, and reorder. Pure step
//! operations are unit-tested here; rendering stays thin.

use crate::gui::host_view::set_host_gesture_action;
use crate::gui::state::GuiState;
use crate::host::{Gesture, HostAction, SeqStep};
use eframe::egui::{self, Ui};

/// Move step `idx` one slot toward `up`. Out-of-range moves are no-ops.
pub fn move_step(steps: &mut [SeqStep], idx: usize, up: bool) {
    if up {
        if idx > 0 && idx < steps.len() {
            steps.swap(idx - 1, idx);
        }
    } else if idx + 1 < steps.len() {
        steps.swap(idx, idx + 1);
    }
}

fn describe_step(step: &SeqStep) -> String {
    match (step.key, step.delay_ms) {
        (Some(key), delay) => {
            let mods = if step.mods.is_empty() {
                String::new()
            } else {
                format!("{}+", step.mods.join("+"))
            };
            let wait = delay
                .map(|ms| format!(" (wait {}ms first)", ms))
                .unwrap_or_default();
            format!("Key {}{} [{}]{}", mods, step.label, key, wait)
        }
        (None, Some(ms)) => format!("Wait {}ms", ms),
        (None, None) => "Empty step".to_string(),
    }
}

fn with_steps(
    state: &mut GuiState,
    layer_idx: usize,
    gesture: Gesture,
    f: impl FnOnce(&mut Vec<SeqStep>),
) {
    let steps = state
        .host_view
        .as_ref()
        .and_then(|cfg| cfg.layers.get(layer_idx))
        .and_then(|layer| match layer.action(gesture).clone() {
            HostAction::Sequence { steps } => Some(steps),
            _ => None,
        });
    if let Some(mut steps) = steps {
        f(&mut steps);
        set_host_gesture_action(state, layer_idx, gesture, HostAction::Sequence { steps });
    }
}

/// Step list for the gesture currently holding a Sequence. Lives in its
/// own module to respect the file-length gate.
pub fn render_sequence_editor(
    ui: &mut Ui,
    state: &mut GuiState,
    layer_idx: usize,
    gesture: Gesture,
) {
    let steps = state
        .host_view
        .as_ref()
        .and_then(|cfg| cfg.layers.get(layer_idx))
        .and_then(|layer| match layer.action(gesture).clone() {
            HostAction::Sequence { steps } => Some(steps),
            _ => None,
        });
    let Some(steps) = steps else {
        return;
    };

    ui.horizontal(|ui| {
        if ui.button("Add keystroke...").clicked() {
            state.host_recording = true;
        }
        if ui.button("Add wait").clicked() {
            let mut next = steps.clone();
            next.push(SeqStep {
                key: None,
                mods: Vec::new(),
                label: String::new(),
                delay_ms: Some(100),
            });
            set_host_gesture_action(
                state,
                layer_idx,
                gesture,
                HostAction::Sequence { steps: next },
            );
        }
    });

    if steps.is_empty() {
        ui.small("Empty sequence: add keystrokes and waits above.");
    }
    for (i, step) in steps.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}. {}", i + 1, describe_step(step)));
            if ui.small_button("Up").clicked() {
                with_steps(state, layer_idx, gesture, |s| move_step(s, i, true));
            }
            if ui.small_button("Down").clicked() {
                with_steps(state, layer_idx, gesture, |s| move_step(s, i, false));
            }
            if ui.small_button("Delete").clicked() {
                with_steps(state, layer_idx, gesture, |s| {
                    if i < s.len() {
                        s.remove(i);
                    }
                });
            }
        });
        if step.key.is_none() {
            let mut ms = step.delay_ms.unwrap_or(100) as i32;
            ui.horizontal(|ui| {
                ui.label("Wait ms:");
                if ui
                    .add(egui::DragValue::new(&mut ms).range(0..=10000))
                    .changed()
                {
                    with_steps(state, layer_idx, gesture, |s| {
                        if let Some(target) = s.get_mut(i) {
                            target.delay_ms = Some(ms.max(0) as u64);
                        }
                    });
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_step(label: &str) -> SeqStep {
        SeqStep {
            key: Some(8),
            mods: vec![],
            label: label.to_string(),
            delay_ms: None,
        }
    }

    #[test]
    fn move_step_reorders_and_clamps() {
        let mut steps = vec![key_step("a"), key_step("b"), key_step("c")];
        move_step(&mut steps, 1, true);
        assert_eq!(steps[0].label, "b");
        move_step(&mut steps, 0, true);
        assert_eq!(steps[0].label, "b");
        move_step(&mut steps, 2, false);
        assert_eq!(steps[2].label, "c");
        move_step(&mut steps, 9, false);
        assert_eq!(steps.len(), 3);
    }

    #[test]
    fn describe_covers_key_wait_and_combined_steps() {
        assert!(describe_step(&key_step("C")).contains('C'));
        let wait = SeqStep {
            key: None,
            mods: vec![],
            label: String::new(),
            delay_ms: Some(250),
        };
        assert_eq!(describe_step(&wait), "Wait 250ms");
        let both = SeqStep {
            delay_ms: Some(50),
            ..key_step("V")
        };
        assert!(describe_step(&both).contains("wait 50ms first"));
    }
}
