//! Dispatch engine for host-side translation (Phase 1).
//!
//! Pure state machine: no clock reads, no I/O. Time enters as `now_ms`
//! chosen by the caller so every timing behavior is unit-testable.
//! Mirrors vk01-anticater semantics: wraparound layer rolodex, press
//! double-tap window, per-slot alternating hotkey-switch state that resets
//! on config change.

use super::{gesture_for_chord, ChordSpec, Gesture, HostAction, HostConfig};
use std::collections::HashMap;

/// A fully resolved output event. Hotkey-switch and defaulted scroll lines
/// are resolved at dispatch; the output layer never sees ambiguity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    Fire(FiredAction),
    LayerChanged(usize),
    NoOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiredAction {
    Scroll { lines: i32 },
    KeyChord { key: u16, mods: Vec<String> },
    Sequence(Vec<SequenceFire>),
    Aux(super::AuxKey),
    MouseClick { button: super::MouseButton },
    LaunchApp { bundle_id: String },
    OpenUrl { url: String },
    OpenPath { path: String },
    QuitApp { bundle_id: String, force: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceFire {
    pub key: Option<u16>,
    pub mods: Vec<String>,
    pub delay_ms: Option<u64>,
}

pub struct Engine {
    cfg: HostConfig,
    layer_idx: usize,
    /// `Some(t)` while a first press at `t` arms the double-tap window.
    pending_press_ms: Option<u64>,
    /// Alternation state per `L{layer}.{gesture:?}` slot.
    toggle_state: HashMap<String, bool>,
}

impl Engine {
    pub fn new(cfg: HostConfig) -> Self {
        Self {
            layer_idx: 0,
            pending_press_ms: None,
            toggle_state: HashMap::new(),
            cfg,
        }
    }

    /// Hot-reload path: swap config, drop alternation state, clamp layer.
    pub fn apply_config(&mut self, cfg: HostConfig) {
        self.cfg = cfg;
        self.toggle_state.clear();
        self.pending_press_ms = None;
        self.layer_idx = self.layer_idx.min(self.cfg.layers.len().saturating_sub(1));
    }

    pub fn layer_idx(&self) -> usize {
        self.layer_idx
    }

    pub fn config(&self) -> &HostConfig {
        &self.cfg
    }

    pub fn layer_count(&self) -> usize {
        self.cfg.layers.len()
    }

    pub fn layer_names(&self) -> Vec<String> {
        self.cfg.layers.iter().map(|l| l.name.clone()).collect()
    }

    /// Wraparound rolodex, 0-based index in, events out.
    pub fn set_layer(&mut self, idx: usize) -> Vec<EngineEvent> {
        let n = self.cfg.layers.len().max(1);
        self.layer_idx = ((idx % n) + n) % n;
        self.pending_press_ms = None;
        vec![EngineEvent::LayerChanged(self.layer_idx)]
    }

    pub fn cycle_layer(&mut self, delta: isize) -> Vec<EngineEvent> {
        let n = self.cfg.layers.len().max(1) as isize;
        self.set_layer((self.layer_idx as isize + delta).rem_euclid(n) as usize)
    }

    fn tap_window_ms(&self) -> u64 {
        (self.cfg.double_tap_window * 1000.0).max(0.0) as u64
    }

    fn slot_key(&self, gesture: Gesture) -> String {
        format!("L{}.{:?}", self.layer_idx, gesture)
    }

    /// Handle one non-autorepeat key-down chord at `now_ms`. Unknown chords
    /// pass through (return empty: caller must NOT swallow them).
    pub fn handle_chord(&mut self, key: u16, mods: &[&str], now_ms: u64) -> Vec<EngineEvent> {
        let Some(gesture) = gesture_for_chord(key, mods) else {
            return vec![];
        };
        if gesture == Gesture::Press {
            return self.handle_press(now_ms);
        }
        // Any non-press gesture cancels an armed double-tap (it was a single).
        let mut out = self.flush_pending_press();
        let action = self.cfg.layers[self.layer_idx].action(gesture).clone();
        out.push(self.fire(action, gesture));
        out
    }

    /// Fire an armed single press whose window expired without a second tap.
    /// The daemon calls this from its timer when `pending` is armed.
    pub fn fire_expired_press(&mut self, now_ms: u64) -> Vec<EngineEvent> {
        match self.pending_press_ms {
            Some(t) if now_ms.saturating_sub(t) >= self.tap_window_ms() => {
                self.pending_press_ms = None;
                let action = self.cfg.layers[self.layer_idx]
                    .action(Gesture::Press)
                    .clone();
                vec![self.fire(action, Gesture::Press)]
            }
            _ => vec![],
        }
    }

    fn handle_press(&mut self, now_ms: u64) -> Vec<EngineEvent> {
        if !self.cfg.double_tap_switch {
            let action = self.cfg.layers[self.layer_idx]
                .action(Gesture::Press)
                .clone();
            return vec![self.fire(action, Gesture::Press)];
        }
        match self.pending_press_ms {
            Some(t) if now_ms.saturating_sub(t) <= self.tap_window_ms() => {
                // Second tap inside the window: switch layer, swallow both.
                self.pending_press_ms = None;
                self.cycle_layer(1)
            }
            _ => {
                // First tap: arm the window, decide on expiry or second tap.
                self.pending_press_ms = Some(now_ms);
                vec![EngineEvent::NoOp]
            }
        }
    }

    fn flush_pending_press(&mut self) -> Vec<EngineEvent> {
        // A non-press gesture means the armed press was really a single
        // press: run it first so no input is lost.
        match self.pending_press_ms.take() {
            Some(_) => {
                let action = self.cfg.layers[self.layer_idx]
                    .action(Gesture::Press)
                    .clone();
                vec![self.fire(action, Gesture::Press)]
            }
            None => vec![],
        }
    }

    fn fire(&mut self, action: HostAction, gesture: Gesture) -> EngineEvent {
        match action {
            HostAction::None => EngineEvent::NoOp,
            HostAction::Scroll { lines } => EngineEvent::Fire(FiredAction::Scroll {
                lines: lines.unwrap_or(self.cfg.scroll_lines_per_detent),
            }),
            HostAction::KeyChord { key, mods, .. } => {
                EngineEvent::Fire(FiredAction::KeyChord { key, mods })
            }
            HostAction::Sequence { steps } => EngineEvent::Fire(FiredAction::Sequence(
                steps
                    .into_iter()
                    .map(|s| SequenceFire {
                        key: s.key,
                        mods: s.mods,
                        delay_ms: s.delay_ms,
                    })
                    .collect(),
            )),
            HostAction::Aux { key } => EngineEvent::Fire(FiredAction::Aux(key)),
            HostAction::MouseClick { button } => {
                EngineEvent::Fire(FiredAction::MouseClick { button })
            }
            HostAction::LaunchApp { bundle_id } => {
                EngineEvent::Fire(FiredAction::LaunchApp { bundle_id })
            }
            HostAction::OpenUrl { url } => EngineEvent::Fire(FiredAction::OpenUrl { url }),
            HostAction::OpenPath { path } => EngineEvent::Fire(FiredAction::OpenPath { path }),
            HostAction::QuitApp { bundle_id, force } => {
                EngineEvent::Fire(FiredAction::QuitApp { bundle_id, force })
            }
            HostAction::HotkeySwitch { first, second } => {
                let slot = self.slot_key(gesture);
                let use_first = !self.toggle_state.get(&slot).copied().unwrap_or(false);
                self.toggle_state.insert(slot, use_first);
                let pick: Option<ChordSpec> = if use_first {
                    first.or(second)
                } else {
                    second.or(first)
                };
                match pick {
                    Some(c) => EngineEvent::Fire(FiredAction::KeyChord {
                        key: c.key,
                        mods: c.mods,
                    }),
                    None => EngineEvent::NoOp,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    fn engine() -> Engine {
        Engine::new(HostConfig::default_config())
    }

    #[test]
    fn unknown_chords_pass_through_untouched() {
        let mut e = engine();
        assert_eq!(e.handle_chord(8, &["cmd"], 0), vec![]);
        // Right keycode, wrong mods: not swallowed.
        assert_eq!(e.handle_chord(106, &["ctrl"], 0), vec![]);
    }

    #[test]
    fn twist_dispatches_current_layer_action() {
        let mut e = engine();
        // Layer 0 "Navigate": twist-R = scroll +3.
        assert_eq!(
            e.handle_chord(79, &["ctrl", "alt"], 0),
            vec![EngineEvent::Fire(FiredAction::Scroll { lines: 3 })]
        );
        // Twist-L = scroll -3.
        assert_eq!(
            e.handle_chord(106, &["ctrl", "alt"], 0),
            vec![EngineEvent::Fire(FiredAction::Scroll { lines: -3 })]
        );
    }

    #[test]
    fn layer_rolodex_wraps_both_directions() {
        let mut e = engine();
        assert_eq!(e.layer_count(), 2);
        assert_eq!(e.cycle_layer(1), vec![EngineEvent::LayerChanged(1)]);
        assert_eq!(e.cycle_layer(1), vec![EngineEvent::LayerChanged(0)]);
        assert_eq!(e.cycle_layer(-1), vec![EngineEvent::LayerChanged(1)]);
        // Media layer twist-R = volume up aux.
        assert!(matches!(
            e.handle_chord(79, &["ctrl", "alt"], 0)[0],
            EngineEvent::Fire(FiredAction::Aux(AuxKey::VolumeUp))
        ));
    }

    #[test]
    fn double_tap_switches_layer_single_press_fires_on_expiry() {
        let mut e = engine();
        // First press arms the window.
        assert_eq!(
            e.handle_chord(64, &["ctrl", "alt"], 1000),
            vec![EngineEvent::NoOp]
        );
        // Second press inside 250 ms window switches layer.
        assert_eq!(
            e.handle_chord(64, &["ctrl", "alt"], 1100),
            vec![EngineEvent::LayerChanged(1)]
        );
        // Single press: fires when the window expires.
        assert_eq!(
            e.handle_chord(64, &["ctrl", "alt"], 2000),
            vec![EngineEvent::NoOp]
        );
        let fired = e.fire_expired_press(2250);
        assert!(matches!(
            fired[0],
            EngineEvent::Fire(FiredAction::Aux(AuxKey::Mute))
        ));
        // Too early: nothing yet.
        e.handle_chord(64, &["ctrl", "alt"], 3000);
        assert_eq!(e.fire_expired_press(3100), vec![]);
    }

    #[test]
    fn press_is_instant_when_double_tap_disabled() {
        let mut cfg = HostConfig::default_config();
        cfg.double_tap_switch = false;
        let mut e = Engine::new(cfg);
        let out = e.handle_chord(64, &["ctrl", "alt"], 0);
        assert!(matches!(
            out[0],
            EngineEvent::Fire(FiredAction::KeyChord { .. })
        ));
    }

    #[test]
    fn armed_press_flushes_before_other_gesture() {
        let mut e = engine();
        e.handle_chord(64, &["ctrl", "alt"], 0);
        // Twist arrives while press is armed: press was a single, run both.
        let out = e.handle_chord(79, &["ctrl", "alt"], 50);
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0], EngineEvent::Fire(_)));
        assert!(matches!(out[1], EngineEvent::Fire(_)));
    }

    #[test]
    fn hotkey_switch_alternates_and_resets_on_config_change() {
        let mut cfg = HostConfig::default_config();
        cfg.layers[0].twist_l = HostAction::HotkeySwitch {
            first: Some(ChordSpec {
                key: 6,
                mods: vec!["cmd".to_string()],
                label: "Z".to_string(),
            }),
            second: Some(ChordSpec {
                key: 6,
                mods: vec!["cmd".to_string(), "shift".to_string()],
                label: "Z".to_string(),
            }),
        };
        let mut e = Engine::new(cfg.clone());
        let first = e.handle_chord(106, &["ctrl", "alt"], 0);
        let second = e.handle_chord(106, &["ctrl", "alt"], 10);
        assert_ne!(first, second);
        // Config reload resets alternation: next fires `first` again.
        e.apply_config(cfg);
        assert_eq!(e.handle_chord(106, &["ctrl", "alt"], 20), first);
    }
}
