//! Tap-side glue for the future daemon (Phase 3, slice 1).
//!
//! Tracks physical keyboard state coming off a global event tap and feeds
//! the pure dispatch `engine`: held-modifier set, key-down set for
//! autorepeat suppression (repeats arrive as extra key-downs; the engine
//! must see each physical press exactly once), and recorded layer-switch
//! hotkeys. Pure logic over CG keycodes: no rdev types here, so this is
//! fully unit-testable without Accessibility grants or hardware.

use super::engine::{Engine, EngineEvent};
use super::hold_stats::HoldSample;
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

/// A modifier press the grab tap is holding back: everything needed to put it
/// back into the event stream if no slot chord follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub code: u16,
    /// The event's CGEventFlags, replayed verbatim.
    pub flags: u64,
    /// When it was pressed, microseconds on the daemon's one clock.
    pub at_us: u64,
}

/// What the grab tap does with one event (`TapEngine::decide`).
///
/// There is deliberately no "pass this past the held events" verdict: while
/// anything is held, an event that is not consumed comes back as `Replay`,
/// so the stream other applications see keeps the order it was typed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Hand it on; nothing is held.
    Pass,
    /// Consume it; nobody downstream sees it.
    Swallow,
    /// Consume it for now: it may be the first half of a slot chord.
    Hold,
    /// Consume it, then post these held presses and a copy of it, in order.
    Replay(Vec<Held>),
}

/// One `decide` answer: what to do with the event, and what it fired.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub verdict: Verdict,
    pub out: Vec<EngineEvent>,
}

impl Step {
    fn bare(verdict: Verdict) -> Self {
        Self {
            verdict,
            out: vec![],
        }
    }
}

pub struct TapEngine {
    engine: Engine,
    mods: BTreeSet<String>,
    down: HashSet<u16>,
    /// Codes whose press was consumed (non-empty dispatch output). Their
    /// releases are swallowed too; everything else passes through.
    consumed: HashSet<u16>,
    /// Slot-chord modifier presses held back, oldest first (`decide`).
    held: Vec<Held>,
    /// Modifier codes whose held press a consumed chord absorbed: their
    /// releases are swallowed, so no application sees half a keystroke.
    consumed_mods: HashSet<u16>,
    /// Hold measurements waiting for the recorder (`take_hold_samples`).
    samples: Vec<HoldSample>,
}

impl TapEngine {
    pub fn with_frontmost(
        cfg: HostConfig,
        frontmost: std::sync::Arc<dyn crate::host::frontmost::FrontmostApp>,
    ) -> Self {
        let mut me = Self::new(cfg.clone());
        me.engine = Engine::with_frontmost(cfg, frontmost);
        me
    }

    pub fn new(cfg: HostConfig) -> Self {
        Self {
            engine: Engine::new(cfg),
            mods: BTreeSet::new(),
            down: HashSet::new(),
            consumed: HashSet::new(),
            held: Vec::new(),
            consumed_mods: HashSet::new(),
            samples: Vec::new(),
        }
    }

    /// Hot-reload: swap dispatch config, keep physical key state.
    pub fn apply_config(&mut self, cfg: HostConfig) {
        self.engine.apply_config(cfg);
    }

    pub fn layer_idx(&self) -> usize {
        self.engine.layer_idx()
    }

    pub fn config(&self) -> &HostConfig {
        self.engine.config()
    }

    /// Currently held modifiers, sorted. Powers verbose tap logging.
    pub fn held_mods(&self) -> Vec<String> {
        self.mods.iter().cloned().collect()
    }

    /// Which variant is live on the current layer, and why.
    pub fn resolution(&self) -> crate::host::virtual_layer::Resolution {
        self.engine.resolution()
    }

    /// Pin a variant, or clear the pin and return to app-driven choice.
    pub fn set_variant_override(&mut self, name: Option<String>) {
        self.engine.set_variant_override(name);
    }

    pub fn variant_override(&self) -> Option<&str> {
        self.engine.variant_override()
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

    /// The grab tap's whole decision for one key event (AK.LEAK).
    ///
    /// A slot chord is modifiers THEN a key, and the modifiers are events in
    /// their own right: another application watching `FlagsChanged` sees
    /// them even when the key is swallowed (Claude's Quick Entry opens on a
    /// double-tapped Option, and two detents are two Option taps). So a
    /// press of a slot-chord modifier, while every held modifier is one, is
    /// HELD rather than passed. A consumed key then absorbs the held presses
    /// (and their releases are swallowed); anything else replays them first.
    /// `expire_held` replays them when no key follows in `MOD_HOLD_MS`.
    ///
    /// `now_us` is microseconds on the daemon's one clock: the hold
    /// instrument (`hold_stats`) needs finer than the engine's milliseconds.
    pub fn decide(&mut self, code: u16, pressed: bool, flags: u64, now_us: u64) -> Step {
        if let Some(name) = mod_for_code(code) {
            if pressed {
                // A press means any earlier release was seen or lost; never
                // let a stale entry swallow the release of THIS press.
                self.consumed_mods.remove(&code);
                self.mods.insert(name.to_string());
                let holdable = self.mods.iter().all(|m| super::gesture::is_slot_mod(m))
                    && !self.held.iter().any(|h| h.code == code);
                if holdable {
                    self.held.push(Held {
                        code,
                        flags,
                        at_us: now_us,
                    });
                    return Step::bare(Verdict::Hold);
                }
                return self.pass(vec![]);
            }
            self.mods.remove(name);
            if self.consumed_mods.remove(&code) {
                return Step::bare(Verdict::Swallow);
            }
            return self.pass(vec![]);
        }
        if pressed {
            let slot = self.slot_ordinal(code);
            let out = self.key(code, true, now_us / 1000);
            if out.is_empty() {
                return self.pass(out);
            }
            if let (Some(slot), Some(first)) = (slot, self.held.first()) {
                self.samples.push(HoldSample::Gap {
                    slot,
                    gap_us: now_us.saturating_sub(first.at_us),
                });
            }
            self.consumed_mods
                .extend(self.held.drain(..).map(|h| h.code));
            return Step {
                verdict: Verdict::Swallow,
                out,
            };
        }
        if self.release_swallow(code) {
            return Step::bare(Verdict::Swallow);
        }
        self.pass(vec![])
    }

    /// The held presses to replay now, because `MOD_HOLD_MS` passed with no
    /// key to complete a chord. Empty while nothing is due.
    pub fn expire_held(&mut self, now_us: u64) -> Vec<Held> {
        match self.held.first() {
            Some(h) if now_us.saturating_sub(h.at_us) >= crate::policy::MOD_HOLD_MS * 1000 => {
                self.samples.push(HoldSample::Expired);
                std::mem::take(&mut self.held)
            }
            _ => vec![],
        }
    }

    /// Hold measurements since the last call: one `Gap` per slot chord the
    /// hold absorbed, one `Expired` per hold that ran out. Drained by the
    /// daemon and handed to its recorder thread, never written from here.
    pub fn take_hold_samples(&mut self) -> Vec<HoldSample> {
        std::mem::take(&mut self.samples)
    }

    /// Which of the five slot chords `code` is, given the modifiers now
    /// held: its position in `Gesture::ALL`. `None` for any other key, so
    /// the instrument never samples something a person typed.
    fn slot_ordinal(&self, code: u16) -> Option<u8> {
        let mods: Vec<&str> = self.mods.iter().map(String::as_str).collect();
        let gesture = super::gesture::gesture_for_chord(code, &mods)?;
        let at = super::gesture::Gesture::ALL
            .iter()
            .position(|g| *g == gesture)?;
        u8::try_from(at).ok()
    }

    /// Pass, unless presses are held: then they go first, in order.
    fn pass(&mut self, out: Vec<EngineEvent>) -> Step {
        if self.held.is_empty() {
            Step {
                verdict: Verdict::Pass,
                out,
            }
        } else {
            Step {
                verdict: Verdict::Replay(std::mem::take(&mut self.held)),
                out,
            }
        }
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
#[path = "tap_tests.rs"]
mod tests;
