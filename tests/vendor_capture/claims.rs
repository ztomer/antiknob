//! What the captured frames PROVE about the wire format.
//!
//! The companion to `oracle.rs`, which checks that every capture
//! carries provenance, still appears in `VENDOR_UI_MAP.md`, and can be
//! re-derived from this repo's own producers. This file is the other half: it
//! DECODES each frame per the documented layout and asserts the claim recorded
//! beside it.
//!
//! That is the part a golden cannot do for itself. A `assert_eq!(p[4], 5)` in a
//! test is indistinguishable from a remembered value; a decoded capture that
//! says "byte 6 is four payload bytes and only one action's worth of bytes
//! follow" is a statement about the hardware that fails if the format changes.
//!

use super::decoder::decode_entry_group;
use super::decoder::decode_record;
use super::fixture::{by_id, fixture};
use antiknob::fd::{self, Step};
use antiknob::firmware::{FD_KIND_KEYBOARD, LED_MODE_GREEN};
use antiknob::led;
use antiknob::protocol::Action;

// ── the decoder agrees with the documented format ───────────────────────────

/// The five gestures, as the vendor app's own save wrote them.
///
/// EXPERIMENT 1 assigned one distinct action per zone and read the key ids off
/// the wire. That mapping is the reason `key_id_for_knob` uses a stride of five
/// and a base of `button_count + 1`, and it is the one claim in this file that
/// a decay in the code would silently undo.
#[test]
fn the_captured_gesture_records_name_the_five_firmware_slots() {
    let all = fixture();
    let expected = [
        ("gesture-ccw-stop", 2u8, 0x00B7u16),
        ("gesture-press-brightness-up", 3, 0x006F),
        ("gesture-cw-playpause", 4, 0x00CD),
        ("gesture-hold-twist-l-brightness-down", 5, 0x0070),
        ("gesture-hold-twist-r-calculator", 6, 0x0092),
    ];
    for (id, key_id, usage) in expected {
        let c = by_id(&all, id);
        let r = decode_record(&c.bytes).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(
            r.tag, 0xFD,
            "{id}: the vendor writes slot records with 0xFD"
        );
        assert_eq!(r.key_id, key_id, "{id}: firmware key id");
        assert_eq!(r.layer, 1, "{id}: layer is 1-based on the wire");
        assert_eq!(r.kind, 2, "{id}: a MutiMedia action is kind 02");
        assert_eq!(
            r.b5, 0,
            "{id}: byte 5 is 00 in every frame this writer produced"
        );
        assert_eq!(
            r.declared_bytes, 2,
            "{id}: one media action declares TWO payload bytes"
        );
        assert_eq!(
            r.media_usage(),
            Some(usage),
            "{id}: a 16-bit consumer usage spans two entries, low byte first"
        );
    }
}

/// The gesture-to-slot mapping, decided by the CAPTURE rather than by us.
///
/// EXPERIMENT 1 assigned one distinct action per zone, pressed Save, and read
/// the key ids off the wire: keys 2..6 for CCW, press, CW, hold-left,
/// hold-right, with key 1 never driven. `key_id_for_knob` is what turns a
/// gesture into a slot, and it is a stride and a base, so a decay in it would
/// write every binding to the wrong gesture — which is exactly what happened
/// before this was measured, and why nothing complained: the knob did SOMETHING
/// for every gesture.
///
/// The capture is checked against the FUNCTION, not against a literal key id,
/// because a literal would agree with the capture forever while the function
/// that uses it rotted. This assertion is the one that failed when a gesture
/// offset was shifted by one.
#[test]
fn the_captured_gesture_slots_are_the_ones_the_function_computes() {
    use antiknob::protocol::{key_id_for_knob, KnobEvent};
    let all = fixture();

    // The layout in config.yaml after EXPERIMENT 1: keys 2-6 are all gestures,
    // so this device has AT MOST ONE button and declares one.
    let buttons = 1usize;
    for (id, event, want) in [
        ("gesture-ccw-stop", KnobEvent::RotateCCW, 2u8),
        ("gesture-press-brightness-up", KnobEvent::Press, 3),
        ("gesture-cw-playpause", KnobEvent::RotateCW, 4),
        (
            "gesture-hold-twist-l-brightness-down",
            KnobEvent::HoldTwistL,
            5,
        ),
        ("gesture-hold-twist-r-calculator", KnobEvent::HoldTwistR, 6),
    ] {
        let c = by_id(&all, id);
        let r = decode_record(&c.bytes).unwrap_or_else(|e| panic!("{id}: {e}"));
        let got = key_id_for_knob(buttons, 0, event).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(
            got, want,
            "{id}: key_id_for_knob puts {:?} on key {got}, and the vendor app's capture \
             puts it on key {want}",
            event
        );
        assert_eq!(
            got, r.key_id,
            "{id}: the function and the captured frame disagree"
        );
    }

    // And the stride is five, not three: a knob spans five slots.
    let second_knob = key_id_for_knob(buttons, 1, KnobEvent::RotateCCW)
        .expect("a second knob's CCW slot is addressable");
    assert_eq!(
        second_knob, 7,
        "the second knob's first slot must be five past the first's"
    );
}

/// Byte 6 is a payload LENGTH IN BYTES. Three media actions declared six and
/// wrote one; that is the fact `entries_match_declared_length` exists to catch,
/// and it is why the decoder cannot walk byte 6 as an entry count.
#[test]
fn byte_six_is_a_byte_count_not_an_entry_count() {
    let all = fixture();
    for (id, actions) in [
        ("vendor-two-action-save-len-4", 2u8),
        ("vendor-three-action-save-len-6", 3),
    ] {
        let c = by_id(&all, id);
        let r = decode_record(&c.bytes).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(
            r.declared_bytes,
            actions * 2,
            "{id}: byte 6 is 2 x the action count — a byte count, not an entry count, \
             since each media action is two bytes"
        );
        // The chaining failure, stated as what the capture actually shows: the
        // length was declared honestly and then only the LAST action was
        // written, so every earlier entry is zero. Counting the non-zero value
        // bytes is the check; asserting "fewer entries than declared" would
        // not survive the report being zero-padded, which it always is.
        let non_zero: Vec<u8> = r
            .entries
            .iter()
            .filter(|e| e.value != 0)
            .map(|e| e.value)
            .collect();
        assert_eq!(
            non_zero,
            vec![0xE2],
            "{id}: exactly one action reached the device (Mute, 0x00E2) out of {} declared — \
             this is the chaining failure, and if a second value byte appears it is gone",
            actions
        );
    }
}

/// The delay field is 16-bit BIG-ENDIAN, proved by an accident.
///
/// A triple-click failed to select a spin box, so the typed digits appended:
/// `50200` and `509`. `0xC418` and `0x01FD` do not fit in one byte, which is the
/// only reason this is a measurement rather than an inference from `0x0032`.
#[test]
fn the_delay_field_is_sixteen_bit_big_endian() {
    let all = fixture();
    // An entry GROUP: bytes 7 onward, with no report header in front of them.
    // Decoding these as records would be scoring a fragment against a frame's
    // layout, and they have no report id to check.
    let group = by_id(&all, "delay-boxes-123-50200-509");
    assert_eq!(group.kind, "entries");
    assert_eq!(
        decode_entry_group(&group.bytes)
            .iter()
            .map(|e| e.delay_ms)
            .collect::<Vec<_>>(),
        vec![123, 50200, 509],
        "the three typed delays, read big-endian"
    );

    let c = by_id(&all, "keyboard-a-then-500ms-b");
    let r = decode_record(&c.bytes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r.kind, FD_KIND_KEYBOARD);
    assert_eq!(r.declared_bytes, 2, "a then b is two payload bytes");
    assert_eq!(
        r.entries.iter().map(|e| e.delay_ms).collect::<Vec<_>>(),
        vec![0, 500],
        "0x01F4 = 500 ms sits in the entry that follows the first key"
    );
    // Two entries, so two keys — `keycode()` deliberately answers None for a
    // record that is not a single key rather than reporting the first one.
    assert_eq!(
        r.entries.iter().map(|e| e.value).collect::<Vec<_>>(),
        vec![0x04, 0x05],
        "`a` is HID usage 0x04 and `b` is 0x05"
    );
    assert_eq!(r.keycode(), None, "and this is not a one-key record");

    // And the shallower capture of the same experiment, which is the one that
    // reads as three 50s — seventeen unused entries each carrying a default.
    let shallow = by_id(&all, "delay-boxes-123-50-50");
    assert_eq!(
        decode_entry_group(&shallow.bytes)
            .iter()
            .map(|e| e.delay_ms)
            .collect::<Vec<_>>(),
        vec![123, 50, 50],
        "0x0032 = 50, the vendor's default, repeated in every unused entry"
    );
}

/// Nineteen letters in ONE record: the capability the whole exercise was for.
#[test]
fn a_keyboard_sequence_chains_into_one_record() {
    let all = fixture();
    let c = by_id(&all, "keyboard-chain-19");
    let r = decode_record(&c.bytes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        r.declared_bytes, 0x13,
        "0x13 = 19, one payload byte per key"
    );
    assert!(r.entries_match_declared_length());
    let codes: Vec<u8> = r.entries.iter().map(|e| e.value).collect();
    assert_eq!(
        &codes[..5],
        &[0x04, 0x05, 0x07, 0x08, 0x09],
        "the capture shows a, b, d, e, f — the letters actually clicked, not a guess"
    );
    assert_eq!(
        r.entry_count(),
        19,
        "and byte 6 declares nineteen, so the record really is nineteen entries"
    );

    // The capture wrote down five of the nineteen, so the fourteen it does not
    // show cannot be reconstructed — and inventing fourteen letters to make the
    // two sides line up would be exactly the fabrication this fixture exists to
    // avoid. What IS reproducible is the length field: a nineteen-key record
    // declares nineteen payload bytes, one per key.
    let nineteen: Vec<Step> = (0..19)
        .map(|i| {
            Step::now(Action::Key {
                modifiers: 0,
                code: 0x04 + i as u8,
            })
        })
        .collect();
    let built = fd::build_packet(2, 0, &nineteen).expect("a nineteen-key record fits");
    assert_eq!(
        built[6], 0x13,
        "a nineteen-key record must declare 0x13 payload bytes"
    );
}

/// The media-chain limit is the DEVICE's, and the code now refuses it.
///
/// Two media actions written, read back with the second dropped and `len` cut
/// to 2. The second usage was chosen for its non-zero high byte precisely so
/// trailing-zero trimming could not explain it.
#[test]
fn the_device_owns_the_one_media_action_per_slot_limit() {
    let all = fixture();

    let written = decode_record(&by_id(&all, "media-chain-written").bytes)
        .expect("the written frame decodes");
    assert_eq!(written.declared_bytes, 4, "two media actions = four bytes");
    assert!(
        written.entries_match_declared_length(),
        "what this repo WROTE did fill the record — that is what makes the read-back a finding"
    );
    assert_eq!(written.media_usage(), Some(0x00E9));

    let read =
        decode_record(&by_id(&all, "media-chain-read-back").bytes).expect("the read-back decodes");
    assert_eq!(
        read.declared_bytes, 2,
        "the firmware truncated len from 4 to 2 — the second action is gone"
    );
    assert_eq!(read.media_usage(), Some(0x00E9), "the first survived");
    assert!(
        !read.action_bytes().contains(&0x92),
        "the Calculator byte must be absent; if it is present the limit is not real"
    );

    // And the code refuses to write what the firmware would truncate.
    let err = fd::build_packet(
        7,
        0,
        &[
            Step {
                action: Action::Media(0x00E9),
                delay_ms: 0,
            },
            Step {
                action: Action::Media(0x0192),
                delay_ms: 0,
            },
        ],
    )
    .expect_err("a two-action media record must be refused");
    let said = err.to_string();
    assert!(
        said.contains("one media action per slot"),
        "the refusal has to say why, or it reads as a bug: {said}"
    );
}

/// An `FA` read-back proves the device STORED bytes. It has never proved the
/// device ACTS on them.
///
/// The `0xFE` frame below is byte-for-byte what this repo used to flash, and
/// the firmware returned it unchanged while executing zero entries: `len = 0`
/// and the payload at bytes 10-12, where nothing looks. So every "confirmed by
/// read-back" in this repo's history confirmed storage.
#[test]
fn a_read_back_proves_storage_and_never_effect() {
    let all = fixture();
    let c = by_id(&all, "fe-single-action-that-never-ran");
    let r = decode_record(&c.bytes).expect("the old 0xFE record decodes");
    assert_eq!(r.tag, 0xFA, "it came back on the read channel");
    assert_eq!(
        r.declared_bytes, 0,
        "len = 0: the firmware was told there were no entries, so it ran none"
    );
    assert_eq!(r.entry_count(), 0);

    // The same action must go out as 0xFD now, not as this.
    let out = Action::Key {
        modifiers: 0,
        code: 91,
    }
    .to_packet(2, 0);
    assert_eq!(
        out[1], 0xFD,
        "a keyboard single action is written as 0xFD; got 0x{:02X}",
        out[1]
    );
    assert_ne!(
        &out[..c.bytes.len()],
        &c.bytes[..],
        "to_packet still emits the record that never ran"
    );
    assert!(
        !fd::record_matches(&out, &c.bytes),
        "the device would have accepted that"
    );
}

/// The LED read-back needs its tag matched, because the init's echo answers
/// first and its byte 2 is `00`.
///
/// This is the failure `device::led_state::read_led_mode` documents: a
/// read-back on the writing handle returned mode 0 — "off" — for a write that
/// had worked, every time, while `get_led` on a fresh handle looked fine.
#[test]
fn an_led_reply_is_told_apart_from_the_init_echo() {
    let all = fixture();

    let echo = by_id(&all, "led-init-echo").bytes.clone();
    let err = super::decoder::decode_led_reply(&echo)
        .expect_err("the init echo must NOT decode as an LED mode reply");
    assert!(
        err.contains("wrong report"),
        "the refusal has to name the trap: {err}"
    );

    let reply = by_id(&all, "led-mode-read-reply-green").bytes.clone();
    let (mode, palette) = super::decoder::decode_led_reply(&reply).expect("a real reply decodes");
    assert_eq!(mode, LED_MODE_GREEN, "byte 2 is the mode: 02 = green");
    assert_eq!(
        led::led_mode_number("green"),
        Some(mode),
        "and the name the daemon would print is green's"
    );
    assert_eq!(
        palette.len(),
        13,
        "the capture is 16 bytes: 3 header + 13 palette"
    );
}
