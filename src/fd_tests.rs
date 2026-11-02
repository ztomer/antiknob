//! Tests for the `0xFD` sequence writer.
//!
//! Split from `fd.rs` for the file-length gate. Every byte asserted here was
//! read off a real VK01; the derivation is in `VENDOR_UI_MAP.md`.

use super::*;
use crate::protocol::Action;

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
    assert_eq!(FD_MAX_ENTRIES, 19);
    assert_eq!(FD_HEADER_LEN + FD_MAX_ENTRIES * FD_ENTRY_LEN, REPORT_LEN);

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

/// Measured: writing two media actions reads back as one. The device
/// truncates silently, so this refuses instead -- the vendor app is what
/// happens otherwise, showing three actions over a slot holding one.
#[test]
fn a_media_sequence_is_refused_because_the_device_stores_one() {
    let two = vec![
        Step::now(Action::parse("volumeup").unwrap()),
        Step::now(Action::parse("next").unwrap()),
    ];
    let err = build_packet(7, 0, &two).expect_err("a media chain must be refused");
    assert!(err.to_string().contains("one media action"), "{err}");
    // One is still fine, and keyboard sequences are unaffected.
    assert!(build_packet(7, 0, &two[..1]).is_ok());
    let keys: Vec<Step> = (0..5).map(|_| kb("a")).collect();
    assert!(build_packet(7, 0, &keys).is_ok());
}

#[test]
fn a_sequence_mixing_keyboard_and_media_is_refused() {
    let mixed = vec![kb("a"), Step::now(Action::parse("mute").unwrap())];
    let err = build_packet(4, 0, &mixed).expect_err("mixed kinds must be refused");
    assert!(err.to_string().contains("mixes"), "{err}");
}

/// A mouse record is the vendor's (`Widget::SetMousePage` @0x1000477f0,
/// `tests/fixtures/vendor_static.json`): kind 03, byte 5 = 01, len 04,
/// buttons at byte 12, wheel at byte 21. The cross-check against every
/// static record is `tests/vendor_capture/static_derived.rs`.
#[test]
fn a_mouse_action_is_written_as_the_vendor_writes_it() {
    let click = build_packet(4, 0, &[kb("rclick")]).unwrap();
    assert_eq!(&click[..13], &[3, 0xFD, 4, 1, 3, 1, 4, 0, 0, 0, 0, 0, 2]);
    assert!(click[13..].iter().all(|b| *b == 0));
    let wheel = build_packet(4, 0, &[kb("wheeldown")]).unwrap();
    assert_eq!(&wheel[4..7], &[3, 1, 4]);
    assert_eq!(wheel[21], 0xFF);
    assert!(wheel[7..21].iter().all(|b| *b == 0));
}

/// The vendor clears the record before every mouse action, so a mouse
/// record holds one; values it never writes have no known encoding.
#[test]
fn what_the_vendor_never_writes_for_mouse_is_refused() {
    let two = build_packet(4, 0, &[kb("click"), kb("click")]).expect_err("two mouse steps");
    assert!(two.to_string().contains("alone"), "{two}");
    let mixed = build_packet(4, 0, &[kb("a"), kb("click")]).expect_err("mixed");
    assert!(mixed.to_string().contains("mixes"), "{mixed}");
    for action in [
        Action::MouseClick { button: 3 },
        Action::MouseClick { button: 8 },
        Action::MouseWheel { delta: 2 },
        Action::MouseWheel { delta: 0 },
    ] {
        let err = build_packet(4, 0, &[Step::now(action.clone())]).expect_err("unpinned");
        assert!(
            err.to_string().contains("not a mouse record"),
            "{action:?}: {err}"
        );
    }
    let late = Step {
        action: Action::parse("click").unwrap(),
        delay_ms: 50,
    };
    assert!(
        build_packet(4, 0, &[late]).is_err(),
        "no measured mouse delay"
    );
}

/// A touch gesture is the vendor's `SetMousePage` record with byte 5 = 04
/// (the touch class) and the gesture at byte 9: 1 left, 2 right, 3 up,
/// 4 down, 5 like. `tests/fixtures/vendor_static.json` pins each one.
#[test]
fn a_touch_gesture_is_written_as_the_vendor_writes_it() {
    for (name, code) in [
        ("swipe-left", 1u8),
        ("swipe-right", 2),
        ("swipe-up", 3),
        ("swipe-down", 4),
        ("like", 5),
    ] {
        let p = build_packet(2, 0, &[kb(name)]).unwrap();
        assert_eq!(&p[..10], &[3, 0xFD, 2, 1, 3, 4, 4, 0, 0, code], "{name}");
        assert!(p[10..].iter().all(|b| *b == 0), "{name}: nothing else set");
    }
}

/// Only the five gesture codes the vendor writes exist; one record holds one
/// gesture, with no delay, and never beside another kind of action.
#[test]
fn what_the_vendor_never_writes_for_touch_is_refused() {
    for gesture in [0u8, 6, 7, 0x80, 0xFF] {
        let err = build_packet(2, 0, &[Step::now(Action::Touch { gesture })])
            .expect_err("an unpinned gesture code");
        assert!(
            err.to_string().contains("not a mouse record"),
            "{gesture}: {err}"
        );
    }
    let two = build_packet(2, 0, &[kb("swipe-up"), kb("swipe-down")]).expect_err("two gestures");
    assert!(two.to_string().contains("alone"), "{two}");
    let mixed = build_packet(2, 0, &[kb("a"), kb("like")]).expect_err("mixed");
    assert!(mixed.to_string().contains("mixes"), "{mixed}");
    let late = Step {
        action: Action::parse("like").unwrap(),
        delay_ms: 50,
    };
    assert!(build_packet(2, 0, &[late]).is_err(), "no measured delay");
}

#[test]
fn touch_gestures_parse_to_the_vendor_codes() {
    for (name, gesture) in [
        ("swipe-left", 1u8),
        ("swipe-right", 2),
        ("swipe-up", 3),
        ("swipe-down", 4),
        ("like", 5),
        ("Swipe-Left", 1),
    ] {
        assert_eq!(
            Action::parse(name).unwrap(),
            Action::Touch { gesture },
            "{name}"
        );
    }
    // Not names the vendor app's own table has: neither is a key.
    for name in ["swipe", "swipe-diagonal", "dislike", "swipe+left"] {
        assert!(Action::parse(name).is_err(), "{name}");
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

/// Only byte 1 differs between a write and its read-back (`0xFD` out,
/// `0xFA` back).
#[test]
fn a_readback_of_what_was_written_matches() {
    let sent = build_packet(7, 0, &[kb("cmd-x"), kb("cmd-z")]).unwrap();
    let mut back = sent.clone();
    back[1] = 0xFA;
    assert!(record_matches(&sent, &back));
}

/// Byte 5 is STORED, not device-owned: measured 2026-11-01 on the knob,
/// `03 fd 14 01 01 <b5> 01 00 00 17` sent with b5 = 02, 00, 04, 01 read back
/// with exactly that byte each time (VENDOR_UI_MAP, "Byte 5 is stored as
/// written"). A slot holding a different byte 5 holds a different record,
/// so the comparison must see it.
#[test]
fn a_readback_differing_only_in_byte_5_is_rejected() {
    let sent = build_packet(7, 0, &[kb("cmd-x"), kb("cmd-z")]).unwrap();
    let mut back = sent.clone();
    back[1] = 0xFA;
    back[5] = 1;
    assert!(!record_matches(&sent, &back));
}

/// Every byte that carries the binding must be able to break the match,
/// or this is decoration rather than verification.
#[test]
fn a_readback_differing_in_any_binding_byte_is_rejected() {
    let sent = build_packet(7, 0, &[kb("a"), kb("b")]).unwrap();
    let end = FD_HEADER_LEN + 2 * FD_ENTRY_LEN;
    for i in (2..end).chain(std::iter::once(0)) {
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

/// The tail past `len` is part of the record. Measured 2026-11-01: a media
/// write updates only the first `len` entries, so `mute` written over an
/// 11-entry chain with no wipe read back `03 fa 02 01 02 00 02 00 00 e2 00
/// 00 00 00 00 f3 00 00 6b ...` -- the old chain's entries still in the
/// slot. A comparison that stopped at `len` called that slot confirmed.
#[test]
fn a_leftover_tail_past_len_is_not_a_confirmed_write() {
    let sent = Action::parse("mute").unwrap().to_packet(2, 0);
    let mut back = sent.clone();
    back[1] = 0xFA;
    assert!(record_matches(&sent, &back), "a clean tail matches");
    for (at, stale) in [(15usize, 0xF3u8), (18, 0x6B), (63, 0x01)] {
        let mut tailed = back.clone();
        tailed[at] = stale;
        assert!(
            !record_matches(&sent, &tailed),
            "a stale byte {at} past len was read as confirmed"
        );
    }
    // A read-back cut short is not the record either.
    assert!(!record_matches(&sent, &back[..13]));
}

/// A restore replays the stored record with the command the firmware RUNS.
/// Every kind on this table is stored in the `0xFD` layout -- the reference
/// table of 2026-11-01 holds keyboard chains, media, and a vendor-written
/// mouse record (`03 fa 10 02 03 00 04 ...`) -- and `0xFE` single-action
/// records are stored and not executed. The read-back cannot catch a wrong
/// choice (both commands read back identically), so this pins it.
#[test]
fn a_restore_replays_the_stored_record_with_the_command_the_firmware_runs() {
    let chord = Action::parse("ctrl-alt-shift-f16").unwrap().to_packet(2, 0);
    let media = Action::parse("mute").unwrap().to_packet(3, 1);
    let mut mouse = vec![0u8; REPORT_LEN];
    mouse[..13].copy_from_slice(&[3, 0xFA, 0x10, 2, 3, 0, 4, 0, 0, 0, 1, 5, 0]);
    for written in [chord, media, mouse] {
        let mut stored = written.clone();
        stored[1] = crate::firmware::SLOT_TABLE_QUERY_CMD;
        let restore = restore_packet(&stored);
        assert_eq!(
            restore[1], FD_CMD,
            "restored with a command the firmware stores and does not run"
        );
        assert_eq!(restore.len(), REPORT_LEN);
        assert_eq!(restore[0], stored[0]);
        assert_eq!(
            restore[2..],
            stored[2..],
            "a restore must not change the record"
        );
        assert!(
            record_matches(&restore, &stored),
            "the restore must confirm against the record it replays"
        );
    }
}

#[test]
fn parse_sequence_reads_a_list_of_action_names() {
    let specs = vec!["cmd-c".to_string(), "cmd-v".to_string()];
    let steps = parse_sequence(&specs).unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].delay_ms, 0);
    assert!(parse_sequence(&["notakey".to_string()]).is_err());
}

/// Every write gets its wipe, whatever command wrote it. Proven red against
/// the `parse_record` derivation, which returned none for `0xFD` packets.
#[test]
fn every_written_slot_gets_a_wipe() {
    let fd_packet = build_packet(5, 1, &[kb("a"), kb("b")]).unwrap();
    let media = Action::parse("mute").unwrap().to_packet(6, 0);
    // A record some older writer left in the `0xFE` layout still gets one.
    let mut legacy = vec![0u8; REPORT_LEN];
    legacy[..5].copy_from_slice(&[REPORT_ID, crate::firmware::FE_CHORD_CMD, 1, 3, 3]);
    let wipes = wipes_for(&[fd_packet, media, legacy]);
    let slots: Vec<(u8, u8)> = wipes.iter().map(|w| (w[2], w[3])).collect();
    assert_eq!(slots, vec![(5, 2), (6, 1), (1, 3)]);
    assert!(wipes
        .iter()
        .all(|w| w[1] == FD_CMD && w[7..].iter().all(|b| *b == 0)));
}
