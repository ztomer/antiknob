//! The `0xFD` slot writer -- the vendor's own keymap command.
//!
//! `Action::to_packet` writes slots with `0xFE`, one action per slot. The
//! vendor uses `0xFD`, whose record is a SEQUENCE with per-step timing:
//!
//! ```text
//! 03 FD <key_id> <layer> <kind> <b5> <len> <entry> <entry> ... 19 entries
//!
//! entry = <delay hi> <delay lo> <value>     delay is 16-bit BIG-endian ms
//! ```
//!
//! `len` counts value BYTES, not entries, because one action can span
//! several: a keycode is 1 byte, a media usage is 2 (low then high), a mouse
//! action is 4. The payload runs from byte 7 to byte 63, so a slot holds 19
//! entries -- 19 keystrokes, or 9 media actions.
//!
//! **Modifiers are entries, not a bitmask.** `0xF1..=0xF8` are Ctrl, Shift,
//! Alt, Win and their right-hand twins, in HID modifier bit order, and each
//! occupies an entry of its own before the key it applies to. `Ctrl+C` is
//! `F1 06`. That is why the vendor's UI lists `Ctrl+` as a clickable key
//! rather than a checkbox, and it differs from the `0xFE` path, which puts a
//! modifier BITMASK at byte 11.
//!
//! All of this was measured by driving ANTICATER.app under the
//! `tools/hidsnoop/` interposer, one change per save; the derivation is in
//! `VENDOR_UI_MAP.md`. An earlier version of this module GUESSED the layout
//! as `<kind> <n_groups> <group_kind>` with groups of `<00> <mods> <code>`.
//! It round-tripped for a single action by coincidence while writing the
//! modifier bitmask into the delay's high byte, so `ctrl-alt-f16` would have
//! typed F16 after a 5 ms pause with no modifiers held at all.
//!
//! Mouse actions are refused. Their four bytes do not sit where the `0xFE`
//! layout puts them -- the button mask lands at byte 12 and the wheel delta
//! at byte 21 -- and only the buttons (1 left, 2 right, 4 middle) and the
//! wheel (`01` / `ff`) have been measured, not the remaining two bytes.

use crate::protocol::Action;
use anyhow::{anyhow, Result};

/// The write command byte. `0xFE` is the other one; see the module note.
pub const CMD: u8 = 0xFD;

/// Bytes before the first entry: report, command, key, layer, kind, b5, len.
pub const HEADER_LEN: usize = 7;

/// Delay hi, delay lo, value.
pub const ENTRY_LEN: usize = 3;

/// How many entries fit in one 64-byte report.
pub const MAX_ENTRIES: usize = (64 - HEADER_LEN) / ENTRY_LEN;

/// The modifier entry for HID modifier bit `n`, `0xF1 + n`.
///
/// Measured one at a time: `Ctrl+` is `F1`, `Shift+` `F2`, `Alt+` `F3`,
/// `Win+` and `command` both `F4`, then `F5..F8` for the right-hand four.
pub fn modifier_entry(bit: u8) -> u8 {
    0xF1 + bit
}

/// What a record declares itself to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyboard = 1,
    Media = 2,
}

impl Kind {
    fn of(action: &Action) -> Option<Self> {
        match action {
            Action::Key { .. } => Some(Self::Keyboard),
            Action::Media(_) => Some(Self::Media),
            Action::MouseClick { .. } | Action::MouseWheel { .. } => None,
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Media => "media",
        }
    }
}

/// One action and how long to wait before running it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub action: Action,
    /// Milliseconds to wait before this step. The vendor defaults every step
    /// after the first to 50, which is what the `0x32` bytes filling its
    /// records are -- seventeen unused entries, not padding.
    pub delay_ms: u16,
}

impl Step {
    pub fn now(action: Action) -> Self {
        Self {
            action,
            delay_ms: 0,
        }
    }
}

/// The value bytes one action occupies, in wire order.
///
/// A chord becomes one entry per held modifier followed by the key, which is
/// how the vendor writes it and the only reason `Ctrl+` appears in its key
/// list at all.
fn value_bytes(action: &Action) -> Vec<u8> {
    match action {
        Action::Key { modifiers, code } => {
            let mut out: Vec<u8> = (0..8)
                .filter(|bit| modifiers & (1 << bit) != 0)
                .map(modifier_entry)
                .collect();
            out.push(*code);
            out
        }
        Action::Media(usage) => {
            let [low, high] = usage.to_le_bytes();
            vec![low, high]
        }
        // Unreachable: `plan_kind` refuses mouse before we get here.
        Action::MouseClick { .. } | Action::MouseWheel { .. } => Vec::new(),
    }
}

/// The single record kind a sequence can be written as.
///
/// One record has one `kind` byte, so a sequence mixing keyboard and media
/// is not expressible. Refusing is the only honest answer: picking one would
/// write the other action's bytes into fields read under the wrong
/// interpretation, and the device would accept it.
fn plan_kind(steps: &[Step]) -> Result<Kind> {
    let mut kind: Option<Kind> = None;
    for step in steps {
        let this = Kind::of(&step.action).ok_or_else(|| {
            anyhow!(
                "the 0xFD writer has no measured encoding for mouse actions; \
                 their bytes do not sit where the 0xFE layout puts them \
                 (button at byte 12, wheel at byte 21). Bind mouse actions \
                 with the 0xFE path instead"
            )
        })?;
        match kind {
            None => kind = Some(this),
            Some(first) if first == this => {}
            Some(first) => {
                return Err(anyhow!(
                    "one slot record holds one kind of action, but this sequence \
                     mixes {} and {}; split it across gestures",
                    first.describe(),
                    this.describe()
                ))
            }
        }
    }
    kind.ok_or_else(|| anyhow!("an empty sequence binds nothing; give at least one action"))
}

/// Build the 64-byte `0xFD` record binding one slot to a sequence.
///
/// `layer` is 0-based here and 1-based on the wire, matching
/// `Action::to_packet` so callers never have to remember which is which.
pub fn build_packet(key_id: u8, layer: u8, steps: &[Step]) -> Result<Vec<u8>> {
    let kind = plan_kind(steps)?;

    // Flatten to entries. A chord contributes one per modifier plus one for
    // the key, and the step's delay belongs to the FIRST of them: the wait
    // happens before the chord, not between its modifiers.
    let mut entries: Vec<(u16, u8)> = Vec::new();
    for step in steps {
        for (i, value) in value_bytes(&step.action).into_iter().enumerate() {
            entries.push((if i == 0 { step.delay_ms } else { 0 }, value));
        }
    }

    if entries.len() > MAX_ENTRIES {
        return Err(anyhow!(
            "this sequence needs {} entries but one slot holds {}; a truncated \
             sequence would flash as a success and run the wrong thing. Note a \
             chord costs one entry per modifier plus one for the key",
            entries.len(),
            MAX_ENTRIES
        ));
    }

    let mut packet = vec![0u8; 64];
    packet[0] = 0x03;
    packet[1] = CMD;
    packet[2] = key_id;
    packet[3] = layer + 1;
    packet[4] = kind as u8;
    // Byte 5 is device-owned: it reads back the same whatever is sent here.
    packet[5] = 0;
    packet[6] = entries.len() as u8;

    for (i, (delay, value)) in entries.iter().enumerate() {
        let at = HEADER_LEN + i * ENTRY_LEN;
        packet[at] = (delay >> 8) as u8;
        packet[at + 1] = (delay & 0xFF) as u8;
        packet[at + 2] = *value;
    }

    Ok(packet)
}

/// A record that zeroes every entry, to be sent BEFORE a shorter one.
///
/// The device updates only the first `len` bytes' worth of entries and
/// leaves the rest, so replacing a long sequence with a short one strands
/// the tail of the old one in the slot. `len` bounds what the firmware
/// reads, so the residue is inert -- but a slot dump then shows bytes that
/// mean nothing, which is exactly the sort of thing this repo has misread
/// before.
pub fn wipe_packet(key_id: u8, layer: u8) -> Vec<u8> {
    let mut packet = vec![0u8; 64];
    packet[0] = 0x03;
    packet[1] = CMD;
    packet[2] = key_id;
    packet[3] = layer + 1;
    packet[4] = Kind::Keyboard as u8;
    packet[6] = MAX_ENTRIES as u8 - 1;
    packet
}

/// How many bytes of a record carry the binding.
fn significant_len(record: &[u8]) -> Option<usize> {
    let entries = *record.get(6)? as usize;
    let end = HEADER_LEN + entries * ENTRY_LEN;
    (end <= record.len()).then_some(end)
}

/// Does the record read back off the device carry the binding we sent?
///
/// Address-only verification is not verification. A check that asks "did a
/// record for key 7 come back?" answers yes whatever the device stored,
/// which is the same false confidence as the `hid_write` return value that
/// let knob bindings flash into dead slots for months.
///
/// Byte 1 is skipped because it is `0xFD` out and `0xFA` back, and byte 5
/// because the device owns it -- it reads back the same regardless of what
/// was sent in that position.
pub fn record_matches(sent: &[u8], readback: &[u8]) -> bool {
    let Some(end) = significant_len(sent) else {
        return false;
    };
    if significant_len(readback) != Some(end) {
        return false;
    }
    sent.first() == readback.first()
        && sent[2..5] == readback[2..5]
        && sent[6..end] == readback[6..end]
}

/// Parse a list of action names into steps, all with no delay.
pub fn parse_sequence(specs: &[String]) -> Result<Vec<Step>> {
    specs
        .iter()
        .map(|s| Action::parse(s).map(Step::now))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(spec: &str) -> Step {
        Step::now(Action::parse(spec).unwrap())
    }

    /// Written with `antiknob raw` and echoed by the device byte for byte:
    /// `03 FD 02 01 01 00 03  00 00 04  00 00 05  00 00 06`.
    #[test]
    fn a_keyboard_chain_matches_what_the_device_echoed() {
        let packet = build_packet(2, 0, &[kb("a"), kb("b"), kb("c")]).unwrap();
        assert_eq!(
            &packet[..16],
            &[0x03, 0xFD, 2, 1, 1, 0, 3, 0, 0, 0x04, 0, 0, 0x05, 0, 0, 0x06]
        );
        assert!(packet[16..].iter().all(|b| *b == 0));
    }

    /// The other echoed record: `03 FD 03 01 01 00 02  00 00 04  01 f4 05`
    /// -- `a`, wait 500 ms, `b`. The delay is 16-bit BIG-endian.
    #[test]
    fn a_delay_is_sixteen_bits_big_endian() {
        let steps = vec![
            kb("a"),
            Step {
                action: Action::parse("b").unwrap(),
                delay_ms: 500,
            },
        ];
        let packet = build_packet(3, 0, &steps).unwrap();
        assert_eq!(
            &packet[..13],
            &[0x03, 0xFD, 3, 1, 1, 0, 2, 0, 0, 0x04, 0x01, 0xF4, 0x05]
        );
        // A one-byte field could not hold this. A mistyped 50200 in the
        // vendor's own delay box is what proved the width.
        let big = build_packet(
            3,
            0,
            &[Step {
                action: Action::parse("a").unwrap(),
                delay_ms: 50200,
            }],
        )
        .unwrap();
        assert_eq!(&big[7..10], &[0xC4, 0x18, 0x04]);
    }

    /// Modifiers are ENTRIES, `0xF1 + HID bit`, each before the key it
    /// applies to. The vendor writes `Ctrl+C` as `F1 06`, and its Procreate
    /// `Undo` as `F4 1D` -- Cmd+Z, the real shortcut.
    #[test]
    fn a_chord_becomes_one_entry_per_modifier_then_the_key() {
        let packet = build_packet(2, 0, &[kb("ctrl-c")]).unwrap();
        assert_eq!(packet[6], 2, "two entries: the modifier and the key");
        assert_eq!(&packet[7..13], &[0, 0, 0xF1, 0, 0, 0x06]);

        let undo = build_packet(2, 0, &[kb("cmd-z")]).unwrap();
        assert_eq!(&undo[7..13], &[0, 0, 0xF4, 0, 0, 0x1D]);
    }

    #[test]
    fn every_modifier_bit_has_its_measured_entry() {
        for (spec, want) in [
            ("ctrl-a", 0xF1u8),
            ("shift-a", 0xF2),
            ("alt-a", 0xF3),
            ("cmd-a", 0xF4),
            ("rctrl-a", 0xF5),
            ("rshift-a", 0xF6),
            ("ralt-a", 0xF7),
            ("rcmd-a", 0xF8),
        ] {
            let p = build_packet(2, 0, &[kb(spec)]).unwrap();
            assert_eq!(p[9], want, "{spec}");
            assert_eq!(p[12], 0x04, "{spec} still ends in the key");
        }
    }

    /// Modifiers come out in HID bit order, so a multi-modifier chord is
    /// reproducible rather than dependent on how it was spelled.
    #[test]
    fn multiple_modifiers_are_emitted_in_hid_bit_order() {
        let p = build_packet(2, 0, &[kb("cmd-shift-4")]).unwrap();
        assert_eq!(p[6], 3);
        assert_eq!(p[9], 0xF2, "shift (bit 1) before cmd (bit 3)");
        assert_eq!(p[12], 0xF4);
        assert_eq!(p[15], 0x21, "the digit 4");
    }

    /// A media usage is 16-bit and spans two entries, low byte first.
    /// Calculator (0x0192) is what proved it: `92` then `01`.
    #[test]
    fn a_media_usage_spans_two_entries_low_byte_first() {
        let p = build_packet(6, 0, &[Step::now(Action::Media(0x0192))]).unwrap();
        assert_eq!(p[4], 2, "media kind");
        assert_eq!(p[6], 2, "two value bytes");
        assert_eq!(&p[7..13], &[0, 0, 0x92, 0, 0, 0x01]);

        let stop = build_packet(2, 0, &[Step::now(Action::parse("stop").unwrap())]).unwrap();
        assert_eq!(&stop[7..13], &[0, 0, 0xB7, 0, 0, 0x00]);
    }

    /// 19 entries fill the report exactly. Nineteen letters is what the
    /// vendor app itself wrote, `len = 0x13`.
    #[test]
    fn nineteen_entries_fill_the_report_and_twenty_are_refused() {
        assert_eq!(MAX_ENTRIES, 19);
        assert_eq!(HEADER_LEN + MAX_ENTRIES * ENTRY_LEN, 64);

        let ok: Vec<Step> = (0..19).map(|_| kb("a")).collect();
        assert_eq!(build_packet(2, 0, &ok).expect("19 fits")[6], 19);

        let over: Vec<Step> = (0..20).map(|_| kb("a")).collect();
        let err = build_packet(2, 0, &over).expect_err("20 must be refused");
        assert!(err.to_string().contains("truncated"), "{err}");
    }

    /// A chord costs more than one entry, so the limit bites sooner than an
    /// action count suggests. Saying so saves the reader working it out.
    #[test]
    fn the_limit_counts_entries_not_actions() {
        let steps: Vec<Step> = (0..10).map(|_| kb("ctrl-a")).collect();
        let err = build_packet(2, 0, &steps).expect_err("20 entries must be refused");
        assert!(err.to_string().contains("20 entries"), "{err}");
        assert!(err.to_string().contains("chord costs"), "{err}");
    }

    #[test]
    fn a_sequence_mixing_keyboard_and_media_is_refused() {
        let mixed = vec![kb("a"), Step::now(Action::parse("mute").unwrap())];
        let err = build_packet(4, 0, &mixed).expect_err("mixed kinds must be refused");
        assert!(err.to_string().contains("mixes"), "{err}");
    }

    /// Mouse has no measured `0xFD` encoding: its bytes land at 12 and 21,
    /// not where the `0xFE` layout puts them. Guessing is how media spent
    /// months being written to bytes the firmware never read.
    #[test]
    fn mouse_actions_are_refused_rather_than_guessed_at() {
        for spec in ["click", "wheelup"] {
            let err = build_packet(4, 0, &[kb(spec)]).expect_err("mouse must be refused");
            assert!(err.to_string().contains("0xFE"), "{spec}: {err}");
        }
    }

    #[test]
    fn an_empty_sequence_binds_nothing_and_is_refused() {
        let err = build_packet(4, 0, &[]).expect_err("empty must be refused");
        assert!(err.to_string().contains("empty"), "{err}");
    }

    #[test]
    fn the_layer_byte_is_one_based_on_the_wire() {
        for layer in 0..3u8 {
            assert_eq!(build_packet(4, layer, &[kb("a")]).unwrap()[3], layer + 1);
        }
    }

    /// The wipe clears every entry so a shorter binding cannot leave the
    /// tail of a longer one behind.
    #[test]
    fn the_wipe_record_covers_every_entry() {
        let w = wipe_packet(7, 1);
        assert_eq!(&w[..7], &[0x03, 0xFD, 7, 2, 1, 0, 18]);
        assert!(w[7..].iter().all(|b| *b == 0), "entries must be zero");
    }

    /// Byte 5 is device-owned and must not break the comparison.
    #[test]
    fn a_readback_of_what_was_written_matches() {
        let sent = build_packet(7, 0, &[kb("cmd-x"), kb("cmd-z")]).unwrap();
        let mut back = sent.clone();
        back[1] = 0xFA;
        back[5] = 1;
        assert!(record_matches(&sent, &back));
    }

    /// Every byte that carries the binding must be able to break the match,
    /// or this is decoration rather than verification.
    #[test]
    fn a_readback_differing_in_any_binding_byte_is_rejected() {
        let sent = build_packet(7, 0, &[kb("a"), kb("b")]).unwrap();
        let end = HEADER_LEN + 2 * ENTRY_LEN;
        for i in (2..5).chain(6..end).chain(std::iter::once(0)) {
            let mut bad = sent.clone();
            bad[1] = 0xFA;
            bad[i] = bad[i].wrapping_add(1);
            assert!(
                !record_matches(&sent, &bad),
                "a device storing a different byte {i} would be reported as confirmed"
            );
        }
        let one = build_packet(7, 0, &[kb("a")]).unwrap();
        let mut short = one.clone();
        short[1] = 0xFA;
        assert!(
            !record_matches(&sent, &short),
            "a partial write is not success"
        );
        assert!(!record_matches(&sent, &[]));
    }

    #[test]
    fn parse_sequence_reads_a_list_of_action_names() {
        let specs = vec!["cmd-c".to_string(), "cmd-v".to_string()];
        let steps = parse_sequence(&specs).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].delay_ms, 0);
        assert!(parse_sequence(&["notakey".to_string()]).is_err());
    }
}
