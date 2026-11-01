//! Verification and mode detection read the WHOLE slot table.
//!
//! The per-slot walk (`read_slot_table`) stops at the declared layout's
//! width, so a record past it is invisible to it: `upload --verify` called
//! a slot above the layout "not seen", and `get_knob_mode` could only look
//! where the declared button count pointed. `read_full_table` is three
//! queries instead of eighteen and strictly more complete (PLAN S.4).
//!
//! The device calls need hardware, so two pins: the pure comparison sees a
//! record above the layout width once it is in the table it is given, and
//! the two call sites are tied to the burst read by source.

use antiknob::device::verify::{verify, SlotVerdict};
use antiknob::protocol::Action;
use std::path::PathBuf;

/// A write re-framed as the reply the device gives for it.
fn as_reply(mut packet: Vec<u8>) -> Vec<u8> {
    packet[1] = antiknob::firmware::SLOT_TABLE_QUERY_CMD;
    packet
}

/// Key 16 sits above a one-button layout's six slots. A record there is
/// seen when the table holds it, and unseen when the table stopped short.
#[test]
fn a_record_above_the_layout_width_is_seen_in_the_full_table() {
    let above = Action::parse("ctrl-alt-shift-f16")
        .unwrap()
        .to_packet(16, 0);
    let in_range = Action::parse("mute").unwrap().to_packet(3, 0);
    let written = vec![above.clone(), in_range.clone()];

    let full = vec![as_reply(in_range.clone()), as_reply(above)];
    let walked = vec![as_reply(in_range)];

    let verdicts: Vec<SlotVerdict> = verify(&written, &full).iter().map(|(_, v)| *v).collect();
    assert_eq!(verdicts, vec![SlotVerdict::Confirmed; 2]);

    let verdicts: Vec<SlotVerdict> = verify(&written, &walked).iter().map(|(_, v)| *v).collect();
    assert_eq!(
        verdicts,
        vec![SlotVerdict::NotSeen, SlotVerdict::Confirmed],
        "the walk's table is what hid the slot"
    );
}

fn source(rel: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("cannot read {rel}: {e}"))
}

/// The two paths S.4 moved. A reintroduced `read_slot_table` turns this red.
#[test]
fn verify_and_get_knob_mode_call_the_burst_read() {
    for rel in ["src/bin/antiknob/cmds.rs", "src/api/dispatch.rs"] {
        let text = source(rel);
        // `device::write_slots` is the burst read after a write; its own
        // read is pinned by `tests/slot_write_confirmed.rs`.
        assert!(
            text.contains("read_full_table") || text.contains("write_slots("),
            "{rel} no longer reads the whole table"
        );
        assert!(
            !text.contains("read_slot_table"),
            "{rel} walks the table at the layout's width again"
        );
    }
}

/// S.8: the hardware tooling that writes and then reads back -- `bind-seq`
/// and the `probe-gestures` family -- reads the whole table too. A walk at a
/// layout width cannot see a key past it, so a probe marker or a bind-seq
/// record above the width read back as missing. No file in `src/` may walk
/// the table that way; `read-slots` keeps its exploratory per-slot walk
/// through `read_slot`, which this does not forbid.
#[test]
fn no_source_walks_the_table_at_a_layout_width() {
    let mut offenders = Vec::new();
    let mut stack = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && std::fs::read_to_string(&path)
                    .unwrap()
                    .contains("read_slot_table(")
            {
                offenders.push(path.display().to_string());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "per-slot walk still called in {offenders:?}"
    );
    for rel in ["src/bin/antiknob/binding.rs", "src/bin/antiknob/probe.rs"] {
        let text = source(rel);
        assert!(
            text.contains("read_full_table") || text.contains("write_slots("),
            "{rel} no longer reads the whole table"
        );
    }
}
