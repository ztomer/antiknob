//! The `0xFD` slot writer -- the vendor's own keymap command.
//!
//! `Action::to_packet` writes slots with `0xFE`, one action per slot. The
//! vendor app uses `0xFD` instead, and the difference is not cosmetic: the
//! `0xFD` record carries a COUNT and then that many fixed-width action
//! groups, so one gesture can run a SEQUENCE. `0xFE` has no field for it.
//!
//! Record layout, measured on a VK01 (`514c:8850`) by writing and reading
//! back -- every write below was echoed by the `FA` read byte for byte:
//!
//! ```text
//! 03 FD <key_id> <layer> <kind> <n_groups> <group_kind> <group> <group> ...
//!                                                        \_ 3 bytes each
//! group = 00 <mods | usage_high> <keycode | usage_low>
//! ```
//!
//! `layer` is 1-based on the wire, the same convention `0xFE` uses.
//! `kind` and `group_kind` are both `01` for keyboard and `02` for media.
//! The commit is `03 FD FE FF`, which `device::send_commit` already sends.
//!
//! How this was established, since the repo has been wrong here before. The
//! notes recorded a captured `0xFD` frame as
//! `03 fd <key> <layer> <kind> <n_mods> <n_groups> <groups...>` and a key
//! mapping of 2..6 for the knob's five gestures. The field order was one
//! byte out and the mapping was wrong: writing `03 FD 02 ...` changes the
//! slot the `FA` read reports as key 2, which on this 3-button layout is the
//! `prev` BUTTON, not a knob gesture. Keys 1-3 are the buttons and 4-6 the
//! knob -- the same numbering `0xFE` uses, because there is only one slot
//! table and `0xFD` addresses it too.
//!
//! That last point is what makes this writer testable at all: a `0xFD` write
//! is visible to the ordinary `FA` read, so `device::verify` covers it and a
//! write that lands nowhere reports as unconfirmed rather than as success.
//!
//! Mouse actions are deliberately NOT supported here. `0xFE` stores them in
//! a layout this module has not measured, and shipping a guessed encoding is
//! how the media-at-byte-11 defect survived for months. Mouse bindings keep
//! using the `0xFE` path, which is verified.

use crate::protocol::Action;
use anyhow::{anyhow, Result};

/// The write command byte. `0xFE` is the other one; see the module note.
pub const CMD: u8 = 0xFD;

/// Bytes before the first action group: report, command, key, layer, kind,
/// count, group kind.
pub const HEADER_LEN: usize = 7;

/// Every action group is this wide, whatever it holds.
pub const GROUP_LEN: usize = 3;

/// How many actions one slot can hold, from the 64-byte report size.
///
/// A sequence longer than this cannot be expressed, so it is refused. The
/// alternative -- writing the first 19 and reporting success -- is the same
/// silent-truncation failure `gesture_probe::plan` refuses for the same
/// reason.
pub const MAX_GROUPS: usize = (64 - HEADER_LEN) / GROUP_LEN;

/// What a record and its groups declare themselves to be.
///
/// Both the `kind` byte and the `group_kind` byte carry this value; every
/// record read off the device has them equal, including the factory
/// placeholders in the slots nothing is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Keyboard = 1,
    Media = 2,
}

impl GroupKind {
    /// Which record kind an action belongs in, or `None` for one this
    /// command has no measured encoding for.
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

/// The two payload bytes of one action group. Byte 0 of a group is always
/// zero in every record read back from the device.
fn group_bytes(action: &Action) -> (u8, u8) {
    match action {
        Action::Key { modifiers, code } => (*modifiers, *code),
        Action::Media(usage) => {
            let [low, high] = usage.to_le_bytes();
            (high, low)
        }
        // Unreachable: `plan_kind` refuses mouse actions before we get here.
        Action::MouseClick { .. } | Action::MouseWheel { .. } => (0, 0),
    }
}

/// The single record kind a sequence can be written as.
///
/// One record has one `kind` byte, so a sequence that mixes keyboard and
/// media actions is not expressible. Refusing is the only honest answer:
/// picking one kind would write the other action's bytes into fields the
/// firmware reads under the wrong interpretation, and the device would
/// accept it.
fn plan_kind(actions: &[Action]) -> Result<GroupKind> {
    let mut kind: Option<GroupKind> = None;
    for action in actions {
        let this = GroupKind::of(action).ok_or_else(|| {
            anyhow!(
                "the 0xFD writer has no measured encoding for mouse actions; \
                 bind them with the 0xFE path instead"
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
pub fn build_packet(key_id: u8, layer: u8, actions: &[Action]) -> Result<Vec<u8>> {
    let kind = plan_kind(actions)?;
    if actions.len() > MAX_GROUPS {
        return Err(anyhow!(
            "{} actions exceeds the {} one slot record can hold; a truncated \
             sequence would flash as a success and run the wrong thing",
            actions.len(),
            MAX_GROUPS
        ));
    }

    let mut packet = vec![0u8; 64];
    packet[0] = 0x03;
    packet[1] = CMD;
    packet[2] = key_id;
    packet[3] = layer + 1;
    packet[4] = kind as u8;
    packet[5] = actions.len() as u8;
    packet[6] = kind as u8;

    for (i, action) in actions.iter().enumerate() {
        let (mid, value) = group_bytes(action);
        let at = HEADER_LEN + i * GROUP_LEN;
        packet[at + 1] = mid;
        packet[at + 2] = value;
    }

    Ok(packet)
}

/// How many bytes of a record actually carry the binding.
///
/// Everything past the last group is padding the device zeroes, so
/// comparing the whole 64 bytes would be comparing noise.
fn significant_len(record: &[u8]) -> Option<usize> {
    let groups = *record.get(5)? as usize;
    let end = HEADER_LEN + groups * GROUP_LEN;
    (end <= record.len()).then_some(end)
}

/// Does the record read back off the device carry the binding we sent?
///
/// Address-only verification is not verification. A check that asks "did a
/// record for key 7 come back?" answers yes whatever the device stored,
/// which is the same class of false confidence as the `hid_write` return
/// value that let knob bindings flash into dead slots for months. This
/// compares the binding itself: kind, count, group kind and every group.
///
/// Byte 1 is skipped on purpose -- it is `0xFD` on the way out and `0xFA` on
/// the way back, and is the one byte that is *supposed* to differ.
pub fn record_matches(sent: &[u8], readback: &[u8]) -> bool {
    let Some(end) = significant_len(sent) else {
        return false;
    };
    if significant_len(readback) != Some(end) {
        return false;
    }
    sent.first() == readback.first() && sent[2..end] == readback[2..end]
}

/// Parse a sequence of action strings into the actions one slot will hold.
pub fn parse_sequence(specs: &[String]) -> Result<Vec<Action>> {
    specs.iter().map(|s| Action::parse(s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte against what the device echoed back. `03 FD 07 01 01
    /// 01 01 00 05 6B` was written to slot 7 and read back as
    /// `03 FA 07 01 01 01 01 00 05 6B`.
    #[test]
    fn a_single_chord_matches_the_record_the_device_echoed() {
        let packet = build_packet(7, 0, &[Action::parse("ctrl-alt-f16").unwrap()]).unwrap();
        assert_eq!(packet.len(), 64);
        assert_eq!(
            &packet[..10],
            &[0x03, 0xFD, 7, 1, 1, 1, 1, 0x00, 0x05, 0x6B]
        );
        assert!(
            packet[10..].iter().all(|b| *b == 0),
            "trailing bytes must be clear"
        );
    }

    /// The measured media record: `03 FD 02 01 02 01 02 00 00 B7` was
    /// written and read back unchanged.
    #[test]
    fn a_media_usage_splits_across_the_group_the_way_the_device_stores_it() {
        let packet = build_packet(2, 0, &[Action::parse("stop").unwrap()]).unwrap();
        assert_eq!(
            &packet[..10],
            &[0x03, 0xFD, 2, 1, 2, 1, 2, 0x00, 0x00, 0xB7]
        );
    }

    /// The reason this command exists. `03 FD 08 01 01 02 01 00 00 04 00 02
    /// 05` was written to slot 8 and echoed back with both groups intact --
    /// two actions in one slot, which `0xFE` cannot express at all.
    #[test]
    fn a_sequence_writes_one_group_per_action() {
        let actions = vec![
            Action::parse("a").unwrap(),
            Action::parse("shift-b").unwrap(),
        ];
        let packet = build_packet(8, 0, &actions).unwrap();
        assert_eq!(
            &packet[..13],
            &[0x03, 0xFD, 8, 1, 1, 2, 1, 0x00, 0x00, 0x04, 0x00, 0x02, 0x05]
        );
    }

    /// The layer byte is 1-based on the wire, like `0xFE`'s.
    #[test]
    fn the_layer_byte_is_one_based_on_the_wire() {
        for layer in 0..3u8 {
            let p = build_packet(4, layer, &[Action::parse("mute").unwrap()]).unwrap();
            assert_eq!(p[3], layer + 1);
        }
    }

    /// One record has one kind byte. Writing a mixed sequence would put one
    /// action's bytes into fields read under the other's interpretation, and
    /// the device would accept it without complaint.
    #[test]
    fn a_sequence_mixing_keyboard_and_media_is_refused() {
        let mixed = vec![Action::parse("a").unwrap(), Action::parse("mute").unwrap()];
        let err = build_packet(4, 0, &mixed).expect_err("mixed kinds must be refused");
        assert!(err.to_string().contains("mixes"), "{err}");
    }

    /// Mouse has no measured `0xFD` encoding. Guessing one is exactly how
    /// media spent months being written to bytes the firmware never read.
    #[test]
    fn mouse_actions_are_refused_rather_than_guessed_at() {
        for spec in ["click", "wheelup"] {
            let err = build_packet(4, 0, &[Action::parse(spec).unwrap()])
                .expect_err("mouse must be refused");
            assert!(err.to_string().contains("0xFE"), "{spec}: {err}");
        }
    }

    /// A truncated sequence would flash as a success and run the wrong
    /// thing, so length is a refusal, not a clamp.
    #[test]
    fn a_sequence_too_long_for_the_report_is_refused_not_truncated() {
        let one = Action::parse("a").unwrap();
        let at_limit = vec![one.clone(); MAX_GROUPS];
        let packet = build_packet(4, 0, &at_limit).expect("the limit itself must fit");
        assert_eq!(packet[5] as usize, MAX_GROUPS);
        // The last group has to land inside the report.
        assert_eq!(HEADER_LEN + MAX_GROUPS * GROUP_LEN, 64);

        let over = vec![one; MAX_GROUPS + 1];
        let err = build_packet(4, 0, &over).expect_err("over the limit must be refused");
        assert!(err.to_string().contains("truncated"), "{err}");
    }

    #[test]
    fn an_empty_sequence_binds_nothing_and_is_refused() {
        let err = build_packet(4, 0, &[]).expect_err("empty must be refused");
        assert!(err.to_string().contains("empty"), "{err}");
    }

    /// `device::verify` reads slots with a parser that accepts `0xFE` and
    /// `0xFA`. A `0xFD` write lands in the same table, so the addressing
    /// bytes have to sit where that parser looks for them.
    #[test]
    fn the_addressing_bytes_sit_where_the_slot_reader_looks_for_them() {
        let packet = build_packet(5, 2, &[Action::parse("mute").unwrap()]).unwrap();
        assert_eq!(packet[2], 5, "key id");
        assert_eq!(packet[3], 3, "layer, 1-based");
    }

    /// The record the device echoed for `bind-seq --key 7 cmd-x cmd-z`,
    /// captured off real hardware. Byte 1 differs by design.
    #[test]
    fn a_readback_of_what_was_written_matches() {
        let sent = build_packet(
            7,
            0,
            &[
                Action::parse("cmd-x").unwrap(),
                Action::parse("cmd-z").unwrap(),
            ],
        )
        .unwrap();
        let mut readback = sent.clone();
        readback[1] = 0xFA;
        assert!(record_matches(&sent, &readback));
    }

    /// The property that makes this verification rather than decoration:
    /// every byte that carries the binding must be able to break the match.
    #[test]
    fn a_readback_differing_in_any_binding_byte_is_rejected() {
        let sent = build_packet(
            7,
            0,
            &[
                Action::parse("cmd-x").unwrap(),
                Action::parse("cmd-z").unwrap(),
            ],
        )
        .unwrap();
        let end = HEADER_LEN + 2 * GROUP_LEN;
        for i in (2..end).chain(std::iter::once(0)) {
            let mut tampered = sent.clone();
            tampered[1] = 0xFA;
            tampered[i] = tampered[i].wrapping_add(1);
            assert!(
                !record_matches(&sent, &tampered),
                "a device storing a different byte {i} would be reported as confirmed"
            );
        }
        // A device that stored only the first of two actions is a partial
        // write, and partial is not success.
        let one = build_packet(7, 0, &[Action::parse("cmd-x").unwrap()]).unwrap();
        let mut short = one.clone();
        short[1] = 0xFA;
        assert!(!record_matches(&sent, &short));
        // And nothing at all is certainly not a match.
        assert!(!record_matches(&sent, &[]));
    }

    #[test]
    fn parse_sequence_reads_a_list_of_action_names() {
        let specs = vec!["cmd-c".to_string(), "cmd-v".to_string()];
        let actions = parse_sequence(&specs).unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0],
            Action::Key {
                modifiers: 0x08,
                code: 0x06
            }
        );
        assert!(parse_sequence(&["notakey".to_string()]).is_err());
    }
}
