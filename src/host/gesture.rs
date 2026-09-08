//! The five gestures, and the chords the firmware sends for them.
//!
//! Split from `mod.rs` for the file-length cap, along the seam between what
//! the knob SENDS and what the host does about it. Nothing here reads a
//! config or fires an action: it is one half of an agreement with
//! `host::bind`, which flashes the other half onto the device. The two are
//! pinned together by `bind::slot_specs_match_the_decoder`, because they
//! speak different keycode spaces -- USB HID on the flashing side, Core
//! Graphics here -- and a change to either alone is silent.

use serde::{Deserialize, Serialize};

/// The five knob gestures, one per bound firmware slot.
///
/// All five are real. An earlier version of this file claimed hold+twist did
/// not exist, from the reference tool's `KnobAction` enum having three
/// variants and a probe of slots 7/8 firing nothing. Neither was evidence:
/// the enum is that tool's model of the device, and the probe only ruled out
/// the slots it tested. Captured from the vendor app driving this hardware,
/// the five gestures are key IDs 2..6 in the `0xFD` command space -- a
/// different table from the `0xFE` writes this build uses, which is why
/// hold+twist was invisible to a probe that never addressed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gesture {
    TwistL,
    Press,
    TwistR,
    HoldTwistL,
    HoldTwistR,
}

impl Gesture {
    /// Every gesture, in slot order.
    pub const ALL: [Gesture; 5] = [
        Gesture::TwistL,
        Gesture::Press,
        Gesture::TwistR,
        Gesture::HoldTwistL,
        Gesture::HoldTwistR,
    ];
}

/// A recorded keyboard chord: CG keycode plus lowercase modifier names
/// (`cmd`, `shift`, `opt`/`alt`, `ctrl`, `fn`). `label` is display-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordSpec {
    pub key: u16,
    pub mods: Vec<String>,
    #[serde(default)]
    pub label: String,
}

/// Ground truth shared with vk01-anticater: slot chord per gesture.
/// CG keycodes: F16=106, F17=64, F18=79, F19=80, F20=90. Required mods:
/// control + alternate. Must NOT be user-editable.
pub fn slot_chord(gesture: Gesture) -> (u16, [&'static str; 3]) {
    match gesture {
        Gesture::TwistL => (106, SLOT_MODS),
        Gesture::Press => (64, SLOT_MODS),
        Gesture::TwistR => (79, SLOT_MODS),
        Gesture::HoldTwistL => (80, SLOT_MODS),
        Gesture::HoldTwistR => (90, SLOT_MODS),
    }
}

/// The modifiers every slot chord carries.
///
/// SHIFT is here because `ctrl+alt` alone collided. Measured 2026-09-08 on
/// this machine: with the knob bound to `ctrl-alt-F18/F19/F20`, twisting or
/// hold-twisting ran the host action AND opened another application's quick
/// window -- something else on the system claims those three, while F16 and
/// F17 were clean. Rebinding one gesture to a bare `z` stopped it, which is
/// what identified the chord rather than the gesture as the trigger.
///
/// Three modifiers instead of two is the cheap dodge: it keeps all five
/// chords in one family (easier to reason about than a mix of F-key ranges)
/// and avoids the F13-F15 alternative, where F14/F15 are brightness on Apple
/// keyboards. If this collides too, move the KEYS, not the modifiers.
const SLOT_MODS: [&str; 3] = ["ctrl", "alt", "shift"];

/// True when an incoming chord is exactly one of the five bound slot
/// chords (keycode plus ctrl+alt, no more, no less).
pub fn is_slot_chord(key: u16, mods: &[&str]) -> bool {
    let mut sorted: Vec<&str> = mods.to_vec();
    sorted.sort_unstable();
    matches_slot(key, &sorted)
}

fn matches_slot(key: u16, sorted_mods: &[&str]) -> bool {
    // Sorted, so this is SLOT_MODS in alphabetical order. Kept as a literal
    // rather than sorting SLOT_MODS at runtime: the point of the assertion
    // is that the decoder and the flasher agree, and a shared helper that
    // derived both from one value could not catch them drifting.
    let wants = |m: &[&str]| m == ["alt", "ctrl", "shift"];
    match key {
        106 | 64 | 79 | 80 | 90 => wants(sorted_mods),
        _ => false,
    }
}

/// Which gesture a slot chord represents, if any.
pub fn gesture_for_chord(key: u16, mods: &[&str]) -> Option<Gesture> {
    if !is_slot_chord(key, mods) {
        return None;
    }
    match key {
        106 => Some(Gesture::TwistL),
        64 => Some(Gesture::Press),
        79 => Some(Gesture::TwistR),
        80 => Some(Gesture::HoldTwistL),
        90 => Some(Gesture::HoldTwistR),
        _ => None,
    }
}
