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
pub fn slot_chord(gesture: Gesture) -> (u16, [&'static str; 2]) {
    match gesture {
        Gesture::TwistL => (106, ["ctrl", "alt"]),
        Gesture::Press => (64, ["ctrl", "alt"]),
        Gesture::TwistR => (79, ["ctrl", "alt"]),
        Gesture::HoldTwistL => (80, ["ctrl", "alt"]),
        Gesture::HoldTwistR => (90, ["ctrl", "alt"]),
    }
}

/// True when an incoming chord is exactly one of the five bound slot
/// chords (keycode plus ctrl+alt, no more, no less).
pub fn is_slot_chord(key: u16, mods: &[&str]) -> bool {
    let mut sorted: Vec<&str> = mods.to_vec();
    sorted.sort_unstable();
    matches_slot(key, &sorted)
}

fn matches_slot(key: u16, sorted_mods: &[&str]) -> bool {
    let wants_alt = |m: &[&str]| m == ["alt", "ctrl"];
    match key {
        106 | 64 | 79 | 80 | 90 => wants_alt(sorted_mods),
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
