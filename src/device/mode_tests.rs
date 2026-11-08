//! Tests for the knob-mode report.
//!
//! Split from `mode.rs` for the file-length gate.

use super::*;
use crate::firmware::{GESTURES_PER_KNOB, SLOT_TABLE_QUERY_CMD};
use crate::protocol::Action;

/// A write re-framed as the reply the device gives for it: `0xFA` out,
/// the same bytes (`VENDOR_UI_MAP`: a record reads back as it was stored).
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

/// The record `Action::to_packet` writes for `name` at `key_id` on
/// 0-based device `layer`, as the device reports it.
fn rec_on(layer: u8, key_id: u8, name: &str) -> Vec<u8> {
    as_reply(Action::parse(name).unwrap().to_packet(key_id, layer))
}

/// The table H.2 left on the knob (2026-11-01): `bind-seq` put the
/// shift-less `ctrl-alt-f16..f20` on keys 2-6 of layer 1, and layers 2
/// and 3 still carried the slot chords `bind-slots` flashes.
fn h2_table() -> Vec<Vec<u8>> {
    let mut t: Vec<Vec<u8>> = (16..=20)
        .zip(2u8..=6)
        .map(|(f, key)| rec_on(0, key, &format!("ctrl-alt-f{f}")))
        .collect();
    t.extend(
        crate::host::bind::binding_packets(1, &[1, 2])
            .unwrap()
            .into_iter()
            .map(as_reply),
    );
    t
}

/// S.9: the mode is read per layer. Layer 1's knob sends chords the
/// daemon does not listen for, so it is standalone, whatever layers 2
/// and 3 hold -- and it is named, keys and all.
#[test]
fn each_device_layer_is_classified_from_its_own_records() {
    let layers = classify_layers(&h2_table(), 1);
    let modes: Vec<KnobMode> = layers.iter().map(|l| l.mode).collect();
    assert_eq!(
        modes,
        vec![
            KnobMode::Standalone,
            KnobMode::HostTranslate,
            KnobMode::HostTranslate
        ]
    );
    assert_eq!(layers[0].not_slot_chords, vec![2, 3, 4, 5, 6]);
    assert_eq!(layers[1].not_slot_chords, Vec::<u8>::new());
    assert_eq!(host_translated_layers(&layers), vec![1, 2]);
    // The knob as a whole can still fire host layers -- on 2 and 3.
    assert_eq!(aggregate(&layers), KnobMode::HostTranslate);
}

#[test]
fn the_summary_names_the_layer_whose_chords_are_not_the_slot_chords() {
    let said = describe_layers(&classify_layers(&h2_table(), 1));
    assert!(!said.contains("All 3"), "{said}");
    assert!(
        said.contains("Device layer(s) 2, 3 are host-translated"),
        "{said}"
    );
    assert!(said.contains("layer(s) 1 run standalone"), "{said}");
    assert!(
        said.contains(
            "Device layer 1: key(s) 2, 3, 4, 5, 6 carry something other than the slot chords"
        ),
        "{said}"
    );
    assert!(!said.contains("bind-slots"), "{said}");
}

/// A slot chord on one layer is no evidence about another: a knob bound
/// on layer 3 alone does not make layers 1 and 2 host-translate.
#[test]
fn a_chord_on_one_layer_says_nothing_about_another() {
    let table: Vec<Vec<u8>> = crate::host::bind::binding_packets(1, &[2])
        .unwrap()
        .into_iter()
        .map(as_reply)
        .chain([rec_on(0, 2, "volumedown"), rec_on(1, 2, "mute")])
        .collect();
    let modes: Vec<KnobMode> = classify_layers(&table, 1).iter().map(|l| l.mode).collect();
    assert_eq!(
        modes,
        vec![
            KnobMode::Standalone,
            KnobMode::Standalone,
            KnobMode::HostTranslate
        ]
    );
    let said = describe_layers(&classify_layers(&table, 1));
    assert!(
        said.contains("Device layer(s) 3 are host-translated"),
        "{said}"
    );
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
