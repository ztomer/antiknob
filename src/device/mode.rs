//! Which mode the knob's firmware is actually in.
//!
//! The settings app edits host layers and draws them as the knob's
//! behaviour, but a host layer only ever fires if the firmware sends the
//! bound slot chords. Flash the knob standalone and those layers become
//! decoration: correct, saved, and unreachable. Nothing on screen said so,
//! which is the same defect as a daemon reporting a tap it never had --
//! an intention presented as a fact.
//!
//! The device can answer. Its slot table says whether the knob's gestures
//! carry slot chords or ordinary actions, so the app reads rather than
//! assumes. Pure: a table in, a mode out.

use super::verify::parse_record;
use crate::protocol::{key_id_for_knob, KnobEvent};

/// What the firmware will do when the knob is turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobMode {
    /// Knob gestures send the bound slot chords, so host layers run.
    HostTranslate,
    /// Knob gestures send ordinary actions the OS handles directly. Host
    /// layers are configured but cannot fire.
    Standalone,
    /// The table holds nothing for the knob's slots -- no device attached,
    /// a read that returned nothing, or genuinely unbound gestures. Not a
    /// mode; an absence of evidence, and it must not be reported as either
    /// of the other two.
    Unknown,
}

impl KnobMode {
    pub fn as_str(self) -> &'static str {
        match self {
            KnobMode::HostTranslate => "host-translate",
            KnobMode::Standalone => "standalone",
            KnobMode::Unknown => "unknown",
        }
    }
}

/// HID keycodes for F16/F17/F18, the three slot chords `bind-slots` writes.
const SLOT_KEYCODES: [u8; 3] = [0x6B, 0x6C, 0x6D];
/// Control + Alt, the modifier pair those chords carry.
const SLOT_MODS: u8 = 0x01 | 0x04;
/// Slot records are keyboard actions.
const KIND_KEYBOARD: u8 = 1;

/// Read the mode off a slot-table dump.
///
/// `button_count` places the knob's slots -- the same layout question that
/// decides where bindings are written, so a wrong count here misreads the
/// table exactly as it would misflash it.
pub fn classify(records: &[Vec<u8>], button_count: usize) -> KnobMode {
    let knob_ids: Vec<u8> = [KnobEvent::RotateCCW, KnobEvent::Press, KnobEvent::RotateCW]
        .iter()
        .map(|e| key_id_for_knob(button_count, 0, *e))
        .collect();

    let mut saw_knob_slot = false;
    let mut saw_chord = false;
    for (addr, action) in records.iter().filter_map(|r| parse_record(r)) {
        if !knob_ids.contains(&addr.key_id) {
            continue;
        }
        // An empty slot is not evidence of a mode either way.
        let empty = action.media == 0 && action.code == 0;
        if empty {
            continue;
        }
        saw_knob_slot = true;
        if action.kind == KIND_KEYBOARD
            && action.mods == SLOT_MODS
            && SLOT_KEYCODES.contains(&action.code)
        {
            saw_chord = true;
        }
    }

    match (saw_knob_slot, saw_chord) {
        (false, _) => KnobMode::Unknown,
        (true, true) => KnobMode::HostTranslate,
        (true, false) => KnobMode::Standalone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `03 FA <key> <layer> <kind> .. .. .. .. <media> .. <mods> <code>`
    fn rec(key_id: u8, kind: u8, media: u8, mods: u8, code: u8) -> Vec<u8> {
        let mut r = vec![0u8; 64];
        r[0] = 0x03;
        r[1] = 0xFA;
        r[2] = key_id;
        r[3] = 1;
        r[4] = kind;
        r[9] = media;
        r[11] = mods;
        r[12] = code;
        r
    }

    /// The table read off this VK01 after a standalone flash: knob slots
    /// 4/5/6 carrying media usages.
    fn standalone_table() -> Vec<Vec<u8>> {
        vec![
            rec(1, 1, 0, 0x08, 0x06), // button: cmd-c
            rec(4, 2, 0xEA, 0, 0),    // knob CCW: volume down
            rec(5, 2, 0xE2, 0, 0),    // knob press: mute
            rec(6, 2, 0xE9, 0, 0),    // knob CW: volume up
        ]
    }

    /// What `bind-slots` leaves behind: ctrl+alt+F16/F17/F18.
    fn host_translate_table() -> Vec<Vec<u8>> {
        vec![
            rec(1, 1, 0, 0x08, 0x06),
            rec(4, 1, 0, SLOT_MODS, 0x6B),
            rec(5, 1, 0, SLOT_MODS, 0x6C),
            rec(6, 1, 0, SLOT_MODS, 0x6D),
        ]
    }

    #[test]
    fn media_bindings_on_the_knob_read_as_standalone() {
        assert_eq!(classify(&standalone_table(), 3), KnobMode::Standalone);
    }

    #[test]
    fn slot_chords_on_the_knob_read_as_host_translate() {
        assert_eq!(
            classify(&host_translate_table(), 3),
            KnobMode::HostTranslate
        );
    }

    /// The check that stops the app claiming a mode it cannot see: no knob
    /// records, or only empty ones, is Unknown -- never Standalone.
    #[test]
    fn an_absent_or_empty_table_is_unknown_rather_than_a_mode() {
        assert_eq!(classify(&[], 3), KnobMode::Unknown);
        // Buttons present, knob slots absent.
        assert_eq!(classify(&[rec(1, 1, 0, 0x08, 0x06)], 3), KnobMode::Unknown);
        // Knob slots present but empty.
        let empty = vec![rec(4, 2, 0, 0, 0), rec(5, 2, 0, 0, 0)];
        assert_eq!(classify(&empty, 3), KnobMode::Unknown);
    }

    /// A wrong button count looks at the wrong slots and must not
    /// confidently report the mode of rows it never examined.
    #[test]
    fn the_button_count_decides_which_slots_are_read() {
        // The same table read as a 15-button device looks at 16/17/18.
        assert_eq!(classify(&standalone_table(), 15), KnobMode::Unknown);
        assert_eq!(classify(&host_translate_table(), 15), KnobMode::Unknown);
    }

    /// One chord among ordinary actions still means the daemon can act --
    /// a half-bound device is host-translate, not standalone.
    #[test]
    fn a_partially_bound_knob_is_host_translate() {
        let mixed = vec![
            rec(4, 1, 0, SLOT_MODS, 0x6B),
            rec(5, 2, 0xE2, 0, 0),
            rec(6, 2, 0xE9, 0, 0),
        ];
        assert_eq!(classify(&mixed, 3), KnobMode::HostTranslate);
    }

    /// F16 without ctrl+alt is not a slot chord; the daemon ignores it.
    #[test]
    fn an_unmodified_function_key_is_not_a_slot_chord() {
        let bare = vec![rec(4, 1, 0, 0, 0x6B)];
        assert_eq!(classify(&bare, 3), KnobMode::Standalone);
    }
}
