//! Checking that a keymap write actually landed.
//!
//! `upload` used to print "Configuration successfully written" on the
//! strength of `hid_write` returning without an error, which it does for a
//! perfectly-formed packet addressed to a slot the firmware stores and never
//! reads. That is exactly what happened here: knob bindings went to key IDs
//! 16/17/18 on a device whose knob reads 4/5/6, the device accepted every
//! packet, and the tool reported success for months of flashes that changed
//! nothing. A write is not evidence; a read-back is.
//!
//! Pure matching lives here so the comparison is testable without hardware;
//! the read loop that feeds it lives in the CLI.

/// The largest key id any supported layout reaches: a 4x4 grid plus four
/// knobs. Anything above it in byte 2 is a subcommand, not a slot.
const MAX_KEY_ID: u8 = 40;

/// One slot as the device reports it: `03 FA <key_id> <layer> <kind> ...`.
///
/// `layer` is the wire value, which is 1-based -- the same convention
/// `Action::to_packet` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotAddr {
    pub key_id: u8,
    pub layer: u8,
}

/// The action bytes that decide whether two records mean the same thing.
///
/// Deliberately not the whole 64-byte record: the reply carries framing the
/// request does not, so a byte-for-byte comparison would report every slot
/// as mismatched. `kind` plus the code fields is what an action IS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotAction {
    pub kind: u8,
    pub media: u8,
    pub mods: u8,
    pub code: u8,
}

/// Read the addressed action out of a 64-byte record, request or reply.
///
/// Byte 1 is `0xFE` on a write and `0xFA` on a read; everything the two
/// share sits at the same offsets, which is why one parser serves both.
pub fn parse_record(record: &[u8]) -> Option<(SlotAddr, SlotAction)> {
    if record.len() < 13 || record[0] != 0x03 {
        return None;
    }
    if record[1] != 0xFE && record[1] != 0xFA {
        return None;
    }
    let (key_id, layer, kind) = (record[2], record[3], record[4]);
    // Byte 2 is a key id only for keymap packets. `0xB0` there is the LED
    // subcommand (`03 FE B0 <layer> <mode>`), and reading it as key 176
    // made a flash with per-layer LEDs report "20 slots, 2 unconfirmed" --
    // counting writes the slot table was never going to contain.
    if key_id == 0 || key_id > MAX_KEY_ID || layer == 0 || layer > 3 {
        return None;
    }
    Some((
        SlotAddr { key_id, layer },
        SlotAction {
            kind,
            media: record[9],
            mods: record[11],
            code: record[12],
        },
    ))
}

/// What a verification pass concluded about one written slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotVerdict {
    /// Read back with the action we wrote.
    Confirmed,
    /// Read back holding something else -- the write was overridden or
    /// went to a slot the firmware rewrites.
    Mismatched,
    /// Never seen in the dump. Not proof of failure: the read walks the
    /// table rather than addressing it, so a slot can simply not come up.
    NotSeen,
}

/// Compare what was written against what the device reports.
///
/// `written` and `observed` are both records in the shared layout above.
pub fn verify(written: &[Vec<u8>], observed: &[Vec<u8>]) -> Vec<(SlotAddr, SlotVerdict)> {
    let seen: Vec<(SlotAddr, SlotAction)> =
        observed.iter().filter_map(|r| parse_record(r)).collect();
    written
        .iter()
        .filter_map(|r| parse_record(r))
        .map(|(addr, want)| {
            let verdict = match seen.iter().find(|(a, _)| *a == addr) {
                None => SlotVerdict::NotSeen,
                Some((_, got)) if *got == want => SlotVerdict::Confirmed,
                Some(_) => SlotVerdict::Mismatched,
            };
            (addr, verdict)
        })
        .collect()
}

/// A one-line summary a user can act on, or `None` when every slot that
/// came up in the dump matched and none is outright wrong.
pub fn summarize(verdicts: &[(SlotAddr, SlotVerdict)]) -> Option<String> {
    let wrong: Vec<&SlotAddr> = verdicts
        .iter()
        .filter(|(_, v)| *v == SlotVerdict::Mismatched)
        .map(|(a, _)| a)
        .collect();
    let unseen = verdicts
        .iter()
        .filter(|(_, v)| *v == SlotVerdict::NotSeen)
        .count();
    if !wrong.is_empty() {
        let list: Vec<String> = wrong
            .iter()
            .map(|a| format!("key {} layer {}", a.key_id, a.layer))
            .collect();
        return Some(format!(
            "{} slot(s) read back with a different action than was written: {}",
            wrong.len(),
            list.join(", ")
        ));
    }
    if unseen > 0 {
        return Some(format!(
            "{} of {} slot(s) did not appear in the read-back, so the write is unconfirmed",
            unseen,
            verdicts.len()
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `03 FE/FA <key> <layer> <kind> .. .. .. .. <media> .. <mods> <code>`
    fn record(tag: u8, key_id: u8, layer: u8, kind: u8, media: u8, mods: u8, code: u8) -> Vec<u8> {
        let mut r = vec![0u8; 64];
        r[0] = 0x03;
        r[1] = tag;
        r[2] = key_id;
        r[3] = layer;
        r[4] = kind;
        r[9] = media;
        r[11] = mods;
        r[12] = code;
        r
    }

    fn written(key_id: u8, media: u8) -> Vec<u8> {
        record(0xFE, key_id, 1, 2, media, 0, 0)
    }
    fn observed(key_id: u8, media: u8) -> Vec<u8> {
        record(0xFA, key_id, 1, 2, media, 0, 0)
    }

    #[test]
    fn a_write_that_reads_back_unchanged_is_confirmed() {
        let v = verify(&[written(4, 0xEA)], &[observed(4, 0xEA)]);
        assert_eq!(
            v,
            vec![(
                SlotAddr {
                    key_id: 4,
                    layer: 1
                },
                SlotVerdict::Confirmed
            )]
        );
        assert_eq!(summarize(&v), None);
    }

    /// The exact failure this module exists for: the packet was accepted,
    /// the slot it addressed was never the one the knob reads, and the slot
    /// still holds the factory action.
    #[test]
    fn a_slot_still_holding_its_old_action_is_reported_as_mismatched() {
        let v = verify(&[written(4, 0xEA)], &[observed(4, 0xE9)]);
        assert_eq!(v[0].1, SlotVerdict::Mismatched);
        let msg = summarize(&v).expect("a mismatch must be reported");
        assert!(msg.contains("key 4 layer 1"), "{msg}");
    }

    #[test]
    fn a_slot_the_dump_never_showed_is_unconfirmed_not_confirmed() {
        let v = verify(&[written(4, 0xEA)], &[observed(5, 0xEA)]);
        assert_eq!(v[0].1, SlotVerdict::NotSeen);
        let msg = summarize(&v).expect("an unconfirmed write must be reported");
        assert!(msg.contains("unconfirmed"), "{msg}");
    }

    #[test]
    fn requests_and_replies_parse_through_the_same_offsets() {
        let want = (
            SlotAddr {
                key_id: 4,
                layer: 1,
            },
            SlotAction {
                kind: 2,
                media: 0xEA,
                mods: 0,
                code: 0,
            },
        );
        assert_eq!(parse_record(&written(4, 0xEA)), Some(want));
        assert_eq!(parse_record(&observed(4, 0xEA)), Some(want));
    }

    #[test]
    fn framing_that_is_not_a_slot_record_is_rejected() {
        assert_eq!(parse_record(&[]), None);
        assert_eq!(parse_record(&[0u8; 64]), None);
        // LED packets share the 0x03/0xFE framing but address no slot.
        assert_eq!(parse_record(&record(0xFE, 0xB0, 0, 0, 0, 0, 0)), None);
        // Padding and end-of-table markers.
        assert_eq!(parse_record(&record(0xFA, 0xFF, 1, 2, 0, 0, 0)), None);
        assert_eq!(parse_record(&record(0xFA, 4, 9, 2, 0, 0, 0)), None);
    }

    /// An LED packet shares the `03 FE` framing and is not a slot. Counting
    /// it as one makes a flash report unconfirmed writes for rows the slot
    /// table never contained.
    #[test]
    fn led_packets_are_not_slot_records() {
        // 03 FE B0 <layer> <mode> -- the LED subcommand.
        let led = record(0xFE, 0xB0, 1, 5, 0, 0, 0);
        assert_eq!(parse_record(&led), None);
        // And they drop out of a verification rather than showing as unseen.
        let verdicts = verify(&[led, written(4, 0xEA)], &[observed(4, 0xEA)]);
        assert_eq!(verdicts.len(), 1, "only the real slot is verified");
        assert_eq!(summarize(&verdicts), None);
    }

    #[test]
    fn keyboard_slots_compare_on_their_modifier_and_keycode() {
        let cmd_c = record(0xFE, 1, 2, 1, 0, 0x08, 0x06);
        let cmd_v = record(0xFA, 1, 2, 1, 0, 0x08, 0x19);
        assert_eq!(
            verify(std::slice::from_ref(&cmd_c), &[cmd_v])[0].1,
            SlotVerdict::Mismatched
        );
        let echo = record(0xFA, 1, 2, 1, 0, 0x08, 0x06);
        assert_eq!(verify(&[cmd_c], &[echo])[0].1, SlotVerdict::Confirmed);
    }
}
