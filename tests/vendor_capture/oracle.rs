//! The byte goldens, checked against a capture instead of against a comment.
//!
//! ## What this replaces
//!
//! This repo's hardcoded byte assertions are real, and most of them carry real
//! provenance — a VK01 dump, a vendor-app capture, an upstream issue, the USB
//! HID Usage Tables. That provenance lived in COMMENTS and in
//! `VENDOR_UI_MAP.md`, and **no test read either**. So the suite was
//! indistinguishable from a snapshot while looking oracle-backed: a reviewer
//! reading `assert_eq!(p[4], 5)` could not tell a measured value from a
//! remembered one, and neither could the test.
//!
//! `tests/fixtures/vendor_capture.json` is that provenance in a form code can
//! read. Every frame in it is a REAL capture, each carrying where it was taken
//! and what it establishes, and this file makes three demands of it:
//!
//!   1. **it is in the document** — every frame still appears in
//!      `VENDOR_UI_MAP.md`, so the prose and the fixture cannot drift apart
//!      unremarked;
//!   2. **it says what it is for** — a frame with no claim recorded beside it
//!      is a value somebody remembered; and
//!   3. **this repo's code re-derives it** — every reproducible frame is rebuilt
//!      from `fd::build_packet`, `led::build_led_packet`, `led_init_packet` or
//!      `slot_table_query` and must match byte for byte.
//!
//! (3) is the half that makes the fixture an ORACLE rather than a transcript. A
//! golden the code also produces is a snapshot; a golden the code must MATCH is
//! a specification, and it fails the moment the encoder drifts from what the
//! hardware was observed to accept.
//!
//! The companion `vendor_capture_decode.rs` does the other half — it DECODES
//! each frame per the documented layout and asserts the claim.
//!
//! ## What could not be done, stated plainly
//!
//! **No new capture was taken, and none was invented.** The knob
//! (`514c:8850`) was not attached when this was written: `ioreg` and
//! `system_profiler` list only the 2.4G receiver (`25a7:fa11`), and
//! `antiknob raw` refuses outright in that state —
//!
//! ```text
//! Error: Device is connected wirelessly. Hardware configuration (flashing
//! slot bindings and LED modes) requires a direct USB-C wired connection.
//! ```
//!
//! `tools/hidsnoop/` needs the same absent device (it wraps the vendor app
//! while that app drives the knob), so it could not produce a fixture either.
//! Every frame in the fixture is transcribed from the capture log already
//! committed in `VENDOR_UI_MAP.md`. That is a weaker oracle than a fresh
//! capture, and the fixture's own header says so.
//!
//! **What still rests on a comment**, after this change:
//!
//! * `VENDOR_UI_MAP.md:137`'s elided frame (`03 fd 05 01 01 00 01 00 00 04
//!   00 32 00 00 32 ...`) — elided with `...` in the source, so there is no
//!   complete frame to decode.
//! * Gesture-to-key-id for the **0x1189 CH57x** devices. The EXPERIMENT 1
//!   capture is a VK01. The mapping is applied across every supported VID/PID
//!   in `SUPPORTED_DEVICES`, and no capture of a CH57x exists in this repo.
//! * **Byte 5 of a chained record.** The factory frame
//!   (`chained-record-from-the-users-own-knob`) shows `01` where this repo writes
//!   `00`, and `VENDOR_UI_MAP.md` still lists "is byte 5 an index" as open.
//!   Nothing here settles it; the frame is pinned so it cannot be lost.
//! * **Mode 3's behaviour** (`ripple`), taken from
//!   kriomant/ch57x-keyboard-tool#173 rather than watched on this hardware — the
//!   code says so in three places and this file does not pretend otherwise.
//! * **A frame ADDED to the document but not to the fixture.** The
//!   fixture-to-document direction is checked; the reverse cannot be, because
//!   the document is prose and a maximal-hex-run regex over it reads `ed20` out
//!   of "Measur**ed 20**26". Adding a capture means adding it to the fixture.

use super::decoder::common_prefix_len;
use super::decoder::decode_record;
use super::decoder::report_sized;
use super::fixture::{by_id, fixture, CAPTURE_LOG};
use antiknob::device::slot_table_query;
use antiknob::fd::{self, Step};
use antiknob::firmware::{
    FD_ENTRY_LEN, FD_HEADER_LEN, LED_CMD, LED_INIT_CMD, LED_MODE_GREEN, LED_MODE_OFF, LED_MODE_RGB,
    LED_MODE_RIPPLE, LED_WRITE_SUB, REPORT_ID, SLOT_BURST_WIDTH, SLOT_COMMIT_PREFIX,
};
use antiknob::led;
use antiknob::protocol::Action;

/// Every maximal run of hex-byte tokens in a line, as digits.
///
/// A capture log writes a frame as `03 fd 02 01 02 00 02 00 00 b7`, sometimes
/// with `|` between groups. This walks each line and cuts at the first
/// character that cannot continue a run, so prose cannot be concatenated into a
/// frame that is not there — which is exactly the failure a whole-document
/// squash has, and the reason this is a scanner and not a `replace`.
///
/// Hand-rolled rather than a regex because the repo has no regex dependency and
/// adding one for a test is not a trade worth making.
fn hex_runs(doc: &str) -> Vec<String> {
    /// Two hex digits, then any run of separators, then two more.
    fn is_byte(b: u8) -> bool {
        b.is_ascii_hexdigit()
    }
    fn is_sep(b: u8) -> bool {
        b.is_ascii_whitespace() || b == b'|'
    }

    let mut runs = Vec::new();
    for line in doc.lines() {
        let b = line.as_bytes();
        let mut i = 0;
        while i + 1 < b.len() {
            if !is_byte(b[i]) || !is_byte(b[i + 1]) {
                i += 1;
                continue;
            }
            let start = i;
            let mut end = i + 2;
            // Extend while the next thing is a separator followed by a byte.
            loop {
                let mut j = end;
                while j < b.len() && is_sep(b[j]) {
                    j += 1;
                }
                if j + 1 < b.len() && is_byte(b[j]) && is_byte(b[j + 1]) {
                    end = j + 2;
                } else {
                    break;
                }
            }
            let digits: String = b[start..end]
                .iter()
                .filter(|c| c.is_ascii_hexdigit())
                .map(|c| (*c as char).to_ascii_lowercase())
                .collect();
            if digits.len() >= 4 && digits.len().is_multiple_of(2) {
                runs.push(digits);
            }
            i = end;
        }
    }
    runs
}

// ── the fixture is the document's, not a private list ───────────────────────

/// Every captured frame must still appear, byte for byte, in the capture log.
///
/// Checked by extracting the document's maximal runs of hex-byte tokens and
/// requiring each fixture frame to equal one, or be a prefix of one.
///
/// **Why not simply strip the non-hex characters and search.** That was the
/// first implementation, and calibrating it was a lesson worth keeping: it was
/// declared unsound on the theory that prose digits concatenate into frames,
/// and that theory was WRONG. Editing all three occurrences of the CCW frame
/// and squashing gives a document that genuinely no longer contains those
/// bytes, so the check does fire. The reasoning was asserted without being run,
/// which is the mistake the whole capture-oracle exercise is about.
///
/// What the calibration did show is that **one frame is written in the document
/// three times, in two formats** —
///
/// ```text
/// 03 fd 02 01 02 00 02 00 00 b7      (twice, space separated)
/// 03 fd 02 01 | 02 00 02 | 00 00 b7  (once, bar separated)
/// ```
///
/// — so "edit the frame in the document" means editing all of them, and a
/// calibration that edits one is measuring nothing. That is the argument for
/// the scanner: it locates frames where they are written rather than
/// reconstructing them from the text around them, its failure names the line,
/// and it cannot report a match assembled out of prose.
///
/// **The direction this cannot cover.** Fixture to document only. A frame ADDED
/// to the document and not to the fixture is not caught, and cannot be without
/// a parser for prose. Adding a capture means adding it to the fixture;
/// `VENDOR_UI_MAP.md` says so.
#[test]
fn every_captured_frame_is_still_in_the_capture_log() {
    let doc = std::fs::read_to_string(CAPTURE_LOG)
        .unwrap_or_else(|e| panic!("VENDOR_UI_MAP.md is the capture log: {e}"));

    let runs = hex_runs(&doc);
    assert!(
        runs.len() > 20,
        "only {} hex runs were found in the capture log; the extractor is broken and a \
         check that matched nothing would report agreement",
        runs.len()
    );

    for c in fixture() {
        if c.kind != "frame" {
            // An entry group is written with `|` between groups and no header;
            // its digits still appear inside a run, so the prefix rule covers it.
        }
        assert!(
            runs.iter().any(|r| r == &c.hex || r.starts_with(&c.hex)),
            "capture {} claims to come from VENDOR_UI_MAP.md:{} but those exact bytes are \
             no longer in the document as a hex run. Either the document was edited or the \
             fixture was invented; both are the failure this test exists for.\n\
             (an entry group is expected to appear as a prefix inside a longer run)",
            c.id,
            c.doc_line
        );
    }
}

/// A frame with no recorded claim is a value somebody remembered.
#[test]
fn every_frame_says_where_it_came_from_and_what_it_establishes() {
    for c in fixture() {
        assert!(
            c.establishes.len() > 40,
            "capture {} states no claim worth testing: {:?}",
            c.id,
            c.establishes
        );
        assert!(c.doc_line > 0, "capture {} names no line in the log", c.id);
    }
}

// ── the producers must re-derive the capture ────────────────────────────────

/// Every reproducible frame, rebuilt from this repo's own code.
///
/// This is the half that makes the fixture an oracle rather than a transcript.
/// A golden that the code also produces is a snapshot; a golden the code must
/// MATCH is a specification.
#[test]
fn every_reproducible_frame_is_rebuilt_byte_for_byte() {
    let all = fixture();

    // The five gesture records: the reason the knob's stride is five.
    for (id, key_id, usage) in [
        ("gesture-ccw-stop", 2u8, 0x00B7u16),
        ("gesture-press-brightness-up", 3, 0x006F),
        ("gesture-cw-playpause", 4, 0x00CD),
        ("gesture-hold-twist-l-brightness-down", 5, 0x0070),
        ("gesture-hold-twist-r-calculator", 6, 0x0092),
    ] {
        let c = by_id(&all, id);
        let built = fd::build_packet(key_id, 0, &[Step::now(Action::Media(usage))])
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(
            &built[..c.bytes.len()],
            &c.bytes[..],
            "{id}: the encoder no longer produces the frame the vendor app sent"
        );
    }

    // The commit tail: one 0xFD FE FF after all five records, never five.
    // The capture is a whole report, so it carries the report id that
    // `send_commit` writes into byte 0 and `SLOT_COMMIT_PREFIX` starts after.
    let commit = by_id(&all, "slot-commit");
    assert_eq!(
        commit.bytes[0], REPORT_ID,
        "the commit capture should be a whole report"
    );
    assert_eq!(
        &commit.bytes[1..],
        SLOT_COMMIT_PREFIX.as_slice(),
        "the commit tail is not the one sent once after all five records"
    );

    // Three slot-table queries, not one per slot.
    for (id, counter) in [
        ("slot-table-read-1", 1u8),
        ("slot-table-read-2", 2),
        ("slot-table-read-3", 3),
    ] {
        let c = by_id(&all, id);
        // `slot_table_query` builds the payload; `send_report` puts the report
        // id in front of it. The capture is a whole report, so compare after it.
        let q = slot_table_query(SLOT_BURST_WIDTH, counter);
        assert_eq!(
            &q[..c.bytes.len() - 1],
            &c.bytes[1..],
            "{id}: the slot-table query no longer matches the vendor's"
        );
    }

    // The LED frames, as PREFIXES: the capture recorded the header only, and
    // the palette that follows it is not in the capture.
    for (id, layer, mode, want) in [
        ("led-mode-3-layer-0", 0u8, "ripple", LED_MODE_RIPPLE),
        ("led-mode-0-layer-1", 1, "off", LED_MODE_OFF),
        ("led-mode-0-layer-2", 2, "off", LED_MODE_OFF),
    ] {
        let c = by_id(&all, id);
        let built = led::build_led_packet(layer, mode).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(
            common_prefix_len(&built, &c.bytes),
            c.bytes.len(),
            "{id}: the LED header no longer matches; built {:?} vs captured {:?}",
            &built[..c.bytes.len()],
            c.bytes
        );
        assert_eq!(
            &built[..4],
            &[REPORT_ID, LED_CMD, LED_WRITE_SUB, layer],
            "{id}"
        );
        assert_eq!(built[4], want, "{id}: the mode byte");
    }

    // `a`, 500 ms, `b`.
    let c = by_id(&all, "keyboard-a-then-500ms-b");
    let built = fd::build_packet(
        3,
        0,
        &[
            Step {
                action: Action::Key {
                    modifiers: 0,
                    code: 0x04,
                },
                delay_ms: 0,
            },
            Step {
                action: Action::Key {
                    modifiers: 0,
                    code: 0x05,
                },
                delay_ms: 500,
            },
        ],
    )
    .expect("a two-key record");
    assert_eq!(
        &built[..c.bytes.len()],
        &c.bytes[..],
        "the per-step delay is not written where the vendor wrote it"
    );
}

/// The LED init the firmware requires, sent before any LED write.
///
/// Without it the firmware accepts the write, stores the mode, reads it back
/// correctly, and changes nothing — which cost this repo a session of wrong
/// conclusions and was sitting in the very first capture, logged as an
/// unremarkable "handshake".
#[test]
fn the_led_init_we_send_is_the_one_the_vendor_sends() {
    let init = led::led_init_packet();
    let captured = by_id(&fixture(), "led-init").bytes.clone();
    assert_eq!(
        &init[..captured.len()],
        &captured[..],
        "the init packet is not the one ANTICATER.app sends"
    );
    assert_eq!(init[1], LED_INIT_CMD);
    assert_eq!(init.len(), 64, "a report is 64 bytes");
}

/// Mode 5 is a first-class button in the vendor's own UI.
///
/// It was refused here for weeks as "crashes the firmware", which made the
/// refusal part of the defect rather than a guard against one. The capture of
/// the vendor walking its own mode buttons is the third independent reason.
#[test]
fn mode_five_is_a_mode_the_device_has() {
    let built = led::build_led_packet(0, "rgb").expect("mode 5 builds");
    assert_eq!(built[4], LED_MODE_RGB);
    assert_eq!(
        &built[..5],
        &[REPORT_ID, LED_CMD, LED_WRITE_SUB, 0, LED_MODE_RGB]
    );
    assert_eq!(
        led::led_mode_number("rgb"),
        Some(LED_MODE_RGB),
        "and the mode-number table agrees, or sync_led would skip the write"
    );
}

// ── the fixture is not vacuous ──────────────────────────────────────────────

/// Nothing in the fixture is longer than a report, and nothing claims a length
/// its own frame cannot carry.
#[test]
fn every_frame_is_shaped_like_a_report() {
    for c in fixture() {
        assert!(!c.bytes.is_empty(), "{} has no bytes", c.id);
        assert!(report_sized(&c.bytes), "{} is longer than a report", c.id);
        if c.kind != "frame" {
            // A captured run of ENTRIES carries no header, so there is no
            // report id to check and decoding it as a report would score a
            // fragment against a frame's layout.
            assert_eq!(
                c.bytes.len() % FD_ENTRY_LEN,
                0,
                "{} is an entry group whose length is not a multiple of the \
                 three-byte entry size, so it is truncated mid-entry",
                c.id
            );
            continue;
        }
        assert_eq!(
            c.bytes[0], REPORT_ID,
            "{} does not start with the report id",
            c.id
        );
        if c.bytes.len() > FD_HEADER_LEN && c.bytes[1] == 0xFD {
            let r = decode_record(&c.bytes).expect("decodes");
            assert!(
                r.entries_match_declared_length() || r.declared_bytes as usize > r.entry_count(),
                "{}: byte 6 declares {} but the capture holds {} entries with nothing after \
                 them, so the capture is truncated mid-record",
                c.id,
                r.declared_bytes,
                r.entry_count()
            );
        }
    }
}

/// The fixture must cover the commands it claims to, not a convenient subset.
///
/// Named rather than counted: "at least N frames" passes just as happily with N
/// frames that are all LED headers, which is the way an oracle decays into
/// agreeing with whatever the code happens to do.
#[test]
fn the_fixture_covers_every_command_the_code_writes() {
    let all = fixture();
    let mut tags: Vec<u8> = all.iter().map(|c| c.bytes[1]).collect();
    tags.sort_unstable();
    tags.dedup();
    for (tag, what) in [
        (0xFDu8, "slot records"),
        (0xFA, "device read-backs"),
        (0xFB, "the LED init"),
        (0xFE, "backlight writes"),
    ] {
        assert!(
            tags.contains(&tag),
            "no captured frame uses 0x{tag:02X} ({what}); the fixture no longer covers every \
             command this crate writes, so it has stopped being an oracle for them"
        );
    }
    let greens = all
        .iter()
        .filter(|c| c.bytes.len() >= 3 && c.bytes[1] == 0xFA && c.bytes[2] == LED_MODE_GREEN)
        .count();
    assert!(
        greens > 0,
        "no captured LED read reply, so the mode byte is unchecked"
    );
}
