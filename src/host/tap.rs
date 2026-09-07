//! Tap-side glue for the future daemon (Phase 3, slice 1).
//!
//! Tracks physical keyboard state coming off a global event tap and feeds
//! the pure dispatch `engine`: held-modifier set, key-down set for
//! autorepeat suppression (repeats arrive as extra key-downs; the engine
//! must see each physical press exactly once), and recorded layer-switch
//! hotkeys. Pure logic over CG keycodes: no rdev types here, so this is
//! fully unit-testable without Accessibility grants or hardware.

use super::engine::{Engine, EngineEvent};
use super::HostConfig;
use std::collections::{BTreeSet, HashSet};

/// (CG keycode, modifier name) for every tracked modifier key.
const MOD_CODES: [(u16, &str); 9] = [
    (59, "ctrl"),
    (62, "ctrl"),
    (58, "alt"),
    (61, "alt"),
    (56, "shift"),
    (60, "shift"),
    (55, "cmd"),
    (54, "cmd"),
    (63, "fn"),
];

/// Modifier name held for a CG keycode, if the code is a modifier.
pub fn mod_for_code(code: u16) -> Option<&'static str> {
    MOD_CODES.iter().find(|(c, _)| *c == code).map(|(_, m)| *m)
}

/// Canonicalize a modifier name the way vk01-anticater does:
/// opt/option/alt, cmd/command/win, ctrl/control, fn/function.
pub fn canonical_mod(name: &str) -> &str {
    match name.to_ascii_lowercase().as_str() {
        "opt" | "option" | "alt" => "alt",
        "cmd" | "command" | "win" => "cmd",
        "ctrl" | "control" => "ctrl",
        "shift" => "shift",
        "fn" | "function" => "fn",
        _ => "unknown",
    }
}

pub struct TapEngine {
    engine: Engine,
    mods: BTreeSet<String>,
    down: HashSet<u16>,
    /// Codes whose press was consumed (non-empty dispatch output). Their
    /// releases are swallowed too; everything else passes through.
    consumed: HashSet<u16>,
}

impl TapEngine {
    pub fn new(cfg: HostConfig) -> Self {
        Self {
            engine: Engine::new(cfg),
            mods: BTreeSet::new(),
            down: HashSet::new(),
            consumed: HashSet::new(),
        }
    }

    /// Hot-reload: swap dispatch config, keep physical key state.
    pub fn apply_config(&mut self, cfg: HostConfig) {
        self.engine.apply_config(cfg);
    }

    pub fn layer_idx(&self) -> usize {
        self.engine.layer_idx()
    }

    /// Currently held modifiers, sorted. Powers verbose tap logging.
    pub fn held_mods(&self) -> Vec<String> {
        self.mods.iter().cloned().collect()
    }

    pub fn layer_names(&self) -> Vec<String> {
        self.engine.layer_names()
    }

    /// Direct layer switch (menu-bar picks, hotkey paths use the engine).
    pub fn set_layer(&mut self, idx: usize) -> Vec<EngineEvent> {
        self.engine.set_layer(idx)
    }

    /// One tap key event. Returns dispatch events for physical presses
    /// only; releases and repeats produce no output.
    pub fn key(&mut self, code: u16, pressed: bool, now_ms: u64) -> Vec<EngineEvent> {
        if let Some(name) = mod_for_code(code) {
            if pressed {
                self.mods.insert(name.to_string());
            } else {
                self.mods.remove(name);
            }
            return vec![];
        }
        if !pressed {
            self.down.remove(&code);
            return vec![];
        }
        // Autorepeat suppression: extra key-downs while held are ignored.
        if !self.down.insert(code) {
            return vec![];
        }
        let held: Vec<&str> = self.mods.iter().map(String::as_str).collect();
        let out = if self.matches_layer_hotkey(code, &held, true) {
            self.engine.cycle_layer(1)
        } else if self.matches_layer_hotkey(code, &held, false) {
            self.engine.cycle_layer(-1)
        } else {
            self.engine.handle_chord(code, &held, now_ms)
        };
        if !out.is_empty() {
            self.consumed.insert(code);
        }
        out
    }

    /// Release bookkeeping for grab mode: returns true when the release
    /// must be swallowed (its press was consumed). Modifier releases are
    /// never swallowed; only swallowed F-key/hotkey releases are.
    pub fn release_swallow(&mut self, code: u16) -> bool {
        if let Some(name) = mod_for_code(code) {
            self.mods.remove(name);
            return false;
        }
        self.down.remove(&code);
        self.consumed.remove(&code)
    }

    /// Timer tick for an armed double-tap window.
    pub fn poll_expiry(&mut self, now_ms: u64) -> Vec<EngineEvent> {
        self.engine.fire_expired_press(now_ms)
    }

    /// Exact-set match against a recorded layer hotkey: the held modifiers
    /// must equal the recorded set (superset would hijack prefixed chords)
    /// and the key must match. Recorded chords always carry >= 1 modifier.
    fn matches_layer_hotkey(&self, code: u16, held: &[&str], forward: bool) -> bool {
        let cfg = self.engine.config();
        let spec = if forward {
            cfg.layer_hotkey.as_ref()
        } else {
            cfg.layer_hotkey_back.as_ref()
        };
        match spec {
            Some(s) if !s.mods.is_empty() && s.key == code => {
                let mut want: Vec<&str> = s.mods.iter().map(|m| canonical_mod(m)).collect();
                want.sort_unstable();
                let mut have: Vec<&str> = held.iter().map(|m| canonical_mod(m)).collect();
                have.sort_unstable();
                want == have
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ChordSpec;
    use super::*;

    fn tap() -> TapEngine {
        TapEngine::new(HostConfig::default_config())
    }

    fn ctrl_alt(t: &mut TapEngine) {
        t.key(59, true, 0);
        t.key(58, true, 0);
    }

    #[test]
    fn modifiers_tracked_and_released() {
        let mut t = tap();
        assert_eq!(t.key(59, true, 0), vec![]);
        assert_eq!(t.key(55, true, 0), vec![]);
        t.key(59, false, 0);
        // Twist-R with only cmd held: not a slot chord, passes through.
        assert_eq!(t.key(79, true, 10), vec![]);
    }

    #[test]
    fn autorepeat_presses_suppressed_until_release() {
        let mut t = tap();
        ctrl_alt(&mut t);
        let first = t.key(79, true, 0);
        assert!(!first.is_empty());
        // Held-key repeats: no output.
        assert_eq!(t.key(79, true, 50), vec![]);
        assert_eq!(t.key(79, true, 100), vec![]);
        // Release re-arms: next press dispatches again.
        t.key(79, false, 150);
        assert_eq!(t.key(79, true, 200), first);
    }

    #[test]
    fn slot_dispatch_end_to_end_through_tap() {
        let mut t = tap();
        ctrl_alt(&mut t);
        let out = t.key(106, true, 0);
        assert!(matches!(
            out[0],
            EngineEvent::Fire(super::super::engine::FiredAction::Scroll { lines: -3 })
        ));
    }

    #[test]
    fn recorded_layer_hotkey_cycles_exact_match_only() {
        let mut cfg = HostConfig::default_config();
        cfg.layer_hotkey = Some(ChordSpec {
            key: 97,
            mods: vec!["cmd".to_string(), "opt".to_string()],
            label: "F6".to_string(),
        });
        let mut t = TapEngine::new(cfg);
        // Partial modifier set: no cycle (would hijack prefixes).
        t.key(55, true, 0);
        assert_eq!(t.key(97, true, 10), vec![]);
        t.key(97, false, 15);
        // Exact set (opt is alt): cycles forward.
        t.key(58, true, 20);
        assert_eq!(t.key(97, true, 30), vec![EngineEvent::LayerChanged(1)]);
    }

    #[test]
    fn swallow_policy_consumed_press_and_release_only() {
        let mut t = tap();
        ctrl_alt(&mut t);
        // Consumed press -> release swallowed.
        assert!(!t.key(79, true, 0).is_empty());
        assert!(t.release_swallow(79));
        // Second release: nothing to swallow.
        assert!(!t.release_swallow(79));
        // Pass-through press -> release passes through.
        assert!(t.key(8, true, 10).is_empty());
        assert!(!t.release_swallow(8));
        // Modifier releases never swallow (but do update mod state).
        assert!(!t.release_swallow(59));
        assert!(t.key(79, true, 20).is_empty());
    }

    #[test]
    fn press_expiry_flows_through_poll() {
        let mut t = tap();
        ctrl_alt(&mut t);
        assert_eq!(t.key(64, true, 1000), vec![EngineEvent::NoOp]);
        assert_eq!(t.poll_expiry(1100), vec![]);
        let fired = t.poll_expiry(1300);
        assert!(matches!(fired[0], EngineEvent::Fire(_)));
    }
}
