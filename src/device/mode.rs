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
use crate::firmware::FD_KIND_KEYBOARD as KIND_KEYBOARD;
use crate::protocol::{key_id_for_knob, KnobEvent};

/// What one knob slot holds, as far as the mode is concerned.
enum Slot {
    /// Nothing runs from it -- no entries, or all zero.
    Empty,
    /// One of the chords `bind-slots` flashes.
    Chord,
    /// Some other action the OS handles directly.
    Other,
}

/// Read a slot out of a record the way the firmware stores it.
///
/// Keyboard and media records are `0xFD` chains, and that is also what
/// `bind-slots` writes and an `FA` read returns, so they are read as
/// chains. The previous reader took mods and keycode from bytes 11/12 of
/// the dead `0xFE` layout, found no chord in a chain, and reported every
/// flashed knob as Standalone. A `0xFE` record the firmware stored with no
/// entries (`len = 0`) is Empty: it holds bytes and runs nothing. Other
/// kinds (mouse) keep their own `0xFE` layout and are read as before.
fn read_slot(record: &[u8]) -> Option<(u8, Slot)> {
    if let Some(chain) = crate::fd::decode_chain(record) {
        if chain.kind == KIND_KEYBOARD || chain.kind == crate::fd::Kind::Media as u8 {
            let slot = if chain.values.iter().all(|v| *v == 0) {
                Slot::Empty
            } else if chain.kind == KIND_KEYBOARD
                && crate::fd::chord_of(&chain.values)
                    .is_some_and(|c| crate::host::bind::slot_chords().contains(&c))
            {
                Slot::Chord
            } else {
                Slot::Other
            };
            return Some((chain.key_id, slot));
        }
    }
    let (addr, action) = parse_record(record)?;
    let slot = if action.media == 0 && action.code == 0 {
        Slot::Empty
    } else {
        Slot::Other
    };
    Some((addr.key_id, slot))
}

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

/// Read the mode off a slot-table dump.
///
/// `button_count` places the knob's slots -- the same layout question that
/// decides where bindings are written, so a wrong count here misreads the
/// table exactly as it would misflash it. A count no slot table can hold
/// yields no ids and reads as Unknown; the flash paths refuse it outright.
pub fn classify(records: &[Vec<u8>], button_count: usize) -> KnobMode {
    // All five gestures, not the first three: hold+twist occupies real
    // slots on this firmware, and a classifier that never looks at them
    // reports on two thirds of the knob while sounding certain about it.
    let knob_ids: Vec<u8> = KnobEvent::ALL
        .iter()
        .filter_map(|e| key_id_for_knob(button_count, 0, *e).ok())
        .collect();

    let mut saw_knob_slot = false;
    let mut saw_chord = false;
    for (key_id, slot) in records.iter().filter_map(|r| read_slot(r)) {
        if !knob_ids.contains(&key_id) {
            continue;
        }
        match slot {
            // An empty slot is not evidence of a mode either way.
            Slot::Empty => {}
            Slot::Chord => {
                saw_knob_slot = true;
                saw_chord = true;
            }
            Slot::Other => saw_knob_slot = true,
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
    use crate::firmware::{GESTURES_PER_KNOB, SLOT_TABLE_QUERY_CMD};
    use crate::protocol::Action;

    /// A write re-framed as the reply the device gives for it: `0xFA` out,
    /// the same bytes (VENDOR_UI_MAP: a record reads back as it was stored).
    /// Every fixture below is built from the flasher's own packets, never
    /// from constants the classifier also reads -- the old fixtures took
    /// mods/code from `SLOT_CHORD_MODS`, so they could only agree with the
    /// reader, and the reader was wrong about the layout.
    fn as_reply(packet: Vec<u8>) -> Vec<u8> {
        let mut r = packet;
        r[1] = SLOT_TABLE_QUERY_CMD;
        r
    }

    /// The record `Action::to_packet` writes for `name` at `key_id`, as the
    /// device reports it.
    fn rec(key_id: u8, name: &str) -> Vec<u8> {
        as_reply(Action::parse(name).unwrap().to_packet(key_id, 0))
    }

    /// An unbound slot as the wipe leaves it: all entries zero.
    fn empty(key_id: u8) -> Vec<u8> {
        as_reply(crate::fd::wipe_packet(key_id, 0))
    }

    /// The table read off this VK01 after a standalone flash: knob slots
    /// 4/5/6 carrying media usages.
    fn standalone_table() -> Vec<Vec<u8>> {
        vec![
            rec(1, "cmd-c"),      // button
            rec(4, "volumedown"), // knob CCW
            rec(5, "mute"),       // knob press
            rec(6, "volumeup"),   // knob CW
        ]
    }

    /// What `bind-slots` leaves behind for a three-button layout.
    fn host_translate_table() -> Vec<Vec<u8>> {
        let mut t = vec![rec(1, "cmd-c")];
        t.extend(
            crate::host::bind::binding_packets(3, &[0])
                .unwrap()
                .into_iter()
                .map(as_reply),
        );
        t
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

    /// The table `bind-slots` really leaves behind, on every device layer.
    /// Proven red against the `0xFE`-layout reader first: it read bytes
    /// 9/11/12 of a `0xFD` chain, found no chord and called every flashed
    /// knob Standalone.
    #[test]
    fn what_bind_slots_writes_reads_as_host_translate() {
        let table: Vec<Vec<u8>> = crate::host::bind::binding_packets(1, &[0, 1, 2])
            .unwrap()
            .into_iter()
            .map(as_reply)
            .collect();
        assert_eq!(table.len(), 3 * GESTURES_PER_KNOB);
        assert_eq!(classify(&table, 1), KnobMode::HostTranslate);
    }

    /// The check that stops the app claiming a mode it cannot see: no knob
    /// records, or only empty ones, is Unknown -- never Standalone.
    #[test]
    fn an_absent_or_empty_table_is_unknown_rather_than_a_mode() {
        assert_eq!(classify(&[], 3), KnobMode::Unknown);
        // Buttons present, knob slots absent.
        assert_eq!(classify(&[rec(1, "cmd-c")], 3), KnobMode::Unknown);
        // Knob slots present but empty.
        assert_eq!(classify(&[empty(4), empty(5)], 3), KnobMode::Unknown);
    }

    /// A wrong button count looks at the wrong slots and must not
    /// confidently report the mode of rows it never examined.
    #[test]
    fn the_button_count_decides_which_slots_are_read() {
        // The same table read as a 15-button device looks at 16..20.
        assert_eq!(classify(&standalone_table(), 15), KnobMode::Unknown);
        assert_eq!(classify(&host_translate_table(), 15), KnobMode::Unknown);
    }

    /// Hold+twist slots count. A knob bound by the current `bind-slots` --
    /// which writes all five chords -- must read as host-translate even
    /// when only the hold+twist pair survives a partial reflash, because
    /// the daemon really can hear those two.
    #[test]
    fn the_hold_twist_slots_are_read_too() {
        // One button, so gestures sit at keys 2..6 and hold+twist at 5/6.
        let chords = crate::host::bind::binding_packets(1, &[0]).unwrap();
        let only_hold_twist = vec![
            rec(2, "volumedown"),
            rec(3, "mute"),
            rec(4, "volumeup"),
            as_reply(chords[3].clone()), // hold+twist L
            as_reply(chords[4].clone()), // hold+twist R
        ];
        assert_eq!(classify(&only_hold_twist, 1), KnobMode::HostTranslate);
    }

    /// One chord among ordinary actions still means the daemon can act --
    /// a half-bound device is host-translate, not standalone.
    #[test]
    fn a_partially_bound_knob_is_host_translate() {
        let chords = crate::host::bind::binding_packets(3, &[0]).unwrap();
        let mixed = vec![
            as_reply(chords[0].clone()),
            rec(5, "mute"),
            rec(6, "volumeup"),
        ];
        assert_eq!(classify(&mixed, 3), KnobMode::HostTranslate);
    }

    /// F16 without the slot modifiers is not a slot chord; the daemon
    /// ignores it. Nor is the pre-shift `ctrl-alt-F16` the classifier used
    /// to be pinned to: it is ordinary typing the OS handles.
    #[test]
    fn only_the_current_slot_chords_count() {
        assert_eq!(classify(&[rec(4, "f16")], 3), KnobMode::Standalone);
        assert_eq!(classify(&[rec(4, "ctrl-alt-f16")], 3), KnobMode::Standalone);
        // Right keycode, one modifier short.
        assert_eq!(
            classify(&[rec(4, "ctrl-shift-f16")], 3),
            KnobMode::Standalone
        );
    }

    /// The dead `0xFE` form -- payload at bytes 10-12, `len = 0` -- is what
    /// older versions stored for a chord. The firmware keeps the bytes and
    /// runs nothing, so it is evidence of no mode at all.
    #[test]
    fn a_stored_but_never_executed_fe_record_is_not_a_mode() {
        let mut dead = vec![0u8; 64];
        dead[..5].copy_from_slice(&[0x03, SLOT_TABLE_QUERY_CMD, 4, 1, 1]);
        dead[10] = 0x01;
        dead[11] = 0x07; // ctrl+alt+shift
        dead[12] = 0x6B;
        assert_eq!(classify(&[dead], 3), KnobMode::Unknown);
    }
}
