//! Records read out of ANTICATER.app's own packet builders, NOT off the wire.
//!
//! `tests/fixtures/vendor_static.json` holds what the vendor's `x86_64` binary
//! writes into its record table (`PHY2_KEY_Value`), which `Widget::HID_write`
//! sends unchanged after the report id. Each entry names the function and
//! address it was read from. It is a second, weaker source than a capture and
//! is kept in its own file so the two are never mixed: a static record that
//! also appeared in the capture fixture would let a reading of the code pass
//! for a measurement.
//!
//! What it pins, and what this crate checks:
//!
//! * the mouse and touch-gesture records antiknob can express (click
//!   left/right/middle, wheel up/down, the four swipes, Like) are rebuilt byte
//!   for byte by `Action::to_packet` and by `fd::build_packet`, the two
//!   producers that write them;
//! * the whole LED write (`led_records`: mode, then the 48 per-key colour
//!   bytes straight after it) is rebuilt byte for byte by `build_led_packet`;
//! * every record decodes, with byte 5 and `len` as the vendor sets them;
//! * no static record is passed off as a capture.

use super::decoder::{decode_record, hex};
use super::fixture::fixture;
use antiknob::fd::{self, Step};
use antiknob::firmware::REPORT_LEN;
use antiknob::protocol::Action;
use serde_json::Value;

const STATIC_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/vendor_static.json"
);

struct StaticRecord {
    id: String,
    source: String,
    reproduce: Option<String>,
    bytes: Vec<u8>,
}

fn records() -> Vec<StaticRecord> {
    let text = std::fs::read_to_string(STATIC_FIXTURE)
        .unwrap_or_else(|e| panic!("tests/fixtures/vendor_static.json is missing: {e}"));
    let json: Value = serde_json::from_str(&text).expect("the static fixture parses");
    let list = json["records"].as_array().expect("a `records` array");
    assert!(
        list.len() >= 16,
        "the static fixture holds {} records; the vendor's mouse page alone has 16",
        list.len()
    );
    list.iter()
        .map(|r| StaticRecord {
            id: r["id"].as_str().expect("id").to_string(),
            source: r["source"].as_str().expect("source").to_string(),
            reproduce: r["reproduce"].as_str().map(str::to_string),
            bytes: hex(r["hex"].as_str().expect("hex")),
        })
        .collect()
}

fn padded(bytes: &[u8]) -> Vec<u8> {
    let mut p = bytes.to_vec();
    p.resize(REPORT_LEN, 0);
    p
}

#[test]
fn every_static_record_names_the_code_it_was_read_from() {
    for r in records() {
        assert!(
            r.source.starts_with("static: ") && r.source.contains("@0x"),
            "{}: a static record must say `static: <function> @<address>`, got {:?}",
            r.id,
            r.source
        );
    }
}

#[test]
fn no_static_record_is_mixed_into_the_captures() {
    let captured: Vec<Vec<u8>> = fixture().into_iter().map(|c| c.bytes).collect();
    for r in records() {
        assert!(
            !captured.contains(&r.bytes),
            "{} is a static reading and also sits in vendor_capture.json; one source each",
            r.id
        );
    }
}

/// The mouse and touch records antiknob can express are what the vendor
/// writes, from both producers. Before this, `fd::build_packet` refused every mouse step
/// and `Action::to_packet` wrote mouse as a `0xFE` record, the command whose
/// single-action records the firmware stores and does not run.
#[test]
fn antiknob_writes_the_mouse_records_the_vendor_writes() {
    let mut checked = 0;
    for r in records() {
        let Some(spec) = r.reproduce.as_deref() else {
            continue;
        };
        let want = padded(&r.bytes);
        let action = Action::parse(spec).expect("a reproducible record names a real action");
        assert_eq!(action.to_packet(2, 0), want, "{}: Action::to_packet", r.id);
        let built = fd::build_packet(2, 0, &[Step::now(action)])
            .unwrap_or_else(|e| panic!("{}: fd::build_packet refused: {e}", r.id));
        assert_eq!(built, want, "{}: fd::build_packet", r.id);
        checked += 1;
    }
    assert_eq!(
        checked, 10,
        "click, rclick, mclick, wheelup, wheeldown, the four swipes and like"
    );
}

/// Byte 5 is written in exactly one place in the vendor binary,
/// `SetMousePage`: `01` for a mouse-report action, `04` for a touch gesture.
/// Keyboard and media setters never touch it, so on those records it is
/// whatever the slot held (zero after the vendor's clear, the device's own
/// value when appending to a record read back off the knob).
#[test]
fn byte_five_is_the_mouse_action_class() {
    for r in records() {
        let rec = decode_record(&r.bytes).unwrap_or_else(|e| panic!("{}: {e}", r.id));
        let b5 = r.bytes[5];
        match (rec.kind, r.id.as_str()) {
            (3, "like" | "swipe-left" | "swipe-right" | "swipe-up" | "swipe-down") => {
                assert_eq!(b5, 4, "{}", r.id);
                assert!((1..=5).contains(&r.bytes[9]), "{}", r.id);
            }
            (3, _) => assert_eq!(b5, 1, "{}", r.id),
            (1, _) => assert_eq!(b5, 0, "{}", r.id),
            (kind, id) => panic!("{id}: unexpected kind {kind}"),
        }
        if rec.kind == 3 {
            assert_eq!(
                r.bytes[6], 4,
                "{}: a vendor mouse record declares len 4",
                r.id
            );
        }
    }
}

/// The whole LED write the vendor builds: `fe b0 <layer> <mode>` then 48
/// colour bytes copied from `_KeyBoard_KeyLed` to byte 5. antiknob sent zeros
/// there; `keys=vendor` is the palette the vendor's own table holds.
#[test]
fn antiknob_writes_the_led_records_the_vendor_writes() {
    let text = std::fs::read_to_string(STATIC_FIXTURE).expect("the static fixture");
    let json: Value = serde_json::from_str(&text).expect("the static fixture parses");
    let list = json["led_records"]
        .as_array()
        .expect("a `led_records` array");
    assert_eq!(list.len(), 3, "layers 0, 1 and 2 at distinct modes");
    for r in list {
        let id = r["id"].as_str().expect("id");
        let source = r["source"].as_str().expect("source");
        assert!(
            source.starts_with("static: ") && source.contains("@0x"),
            "{id}: {source:?}"
        );
        let want = padded(&hex(r["hex"].as_str().expect("hex")));
        let layer = u8::try_from(r["layer"].as_u64().expect("layer")).unwrap();
        let spec = r["reproduce"].as_str().expect("reproduce");
        let built = antiknob::protocol::build_led_packet(layer, spec)
            .unwrap_or_else(|e| panic!("{id}: build_led_packet({spec:?}) refused: {e}"));
        assert_eq!(built, want, "{id}: {spec}");
        assert_eq!(&want[..4], &[3, 0xFE, 0xB0, layer], "{id}");
        assert_eq!(
            want[4],
            u8::try_from(r["mode"].as_u64().unwrap()).unwrap(),
            "{id}"
        );
    }
}
