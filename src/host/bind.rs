//! One-time slot bindings for host-side translation (Phase 2).
//!
//! "Bind once": programs the knob's firmware slots to fixed host chords
//! (`ctrl+alt+shift+F16..F20`) so the translation engine in `super::engine` can
//! swallow them and run layered host actions. All FIVE of knob 0's gestures
//! are bound; they follow the buttons in the key-ID space, so a 1-button
//! VK01 reads them from 2/3/4/5/6.
//!
//! Hold+twist used to be left out of this plan as "which slot it drives is
//! unmeasured". It is measured: `probe-gestures --map` put a distinct marker
//! on keys 1-6 and watched which key each gesture drove, and hold+twist
//! left/right came back as keys 5 and 6 (see `protocol::key_id_for_knob`).
//! Leaving them unbound while `super::slot_chord` claimed F19/F20 and the
//! settings app offered both gestures as bindable rows meant two of the five
//! gestures could never fire, and nothing on any surface said so -- the same
//! intention-presented-as-fact defect as drawing a standalone knob's host
//! layers as live.

use crate::device::send_report;
use crate::device::HidDevice;
use crate::firmware::{GESTURES_PER_KNOB, SLOT_WRITE_GAP_MS};
use crate::protocol::{key_id_for_knob, Action, KnobEvent};
use anyhow::{Context, Result};

/// (gesture, slot F-key name) pairs bound by the plan, in flash order.
///
/// One entry per `KnobEvent`, and the length is `GESTURES_PER_KNOB` so
/// adding a gesture to that enum fails to compile here rather than silently
/// shipping a gesture the daemon listens for and the firmware never sends.
/// The chords must match `super::slot_chord`, which is the decoder's side of
/// the same agreement; `slot_specs_match_the_decoder` pins that.
const SLOT_SPECS: [(KnobEvent, &str); GESTURES_PER_KNOB] = [
    (KnobEvent::RotateCCW, "ctrl-alt-shift-f16"),
    (KnobEvent::Press, "ctrl-alt-shift-f17"),
    (KnobEvent::RotateCW, "ctrl-alt-shift-f18"),
    (KnobEvent::HoldTwistL, "ctrl-alt-shift-f19"),
    (KnobEvent::HoldTwistR, "ctrl-alt-shift-f20"),
];

/// Human-readable "gesture = chord" lines, derived from `SLOT_SPECS`.
///
/// Exists so no caller can restate the chords as prose that drifts from the
/// packets. Two lines, to keep the CLI's width.
pub fn chord_summary_lines() -> Vec<String> {
    let named: Vec<String> = SLOT_SPECS
        .iter()
        .map(|(event, chord)| format!("{}={}", gesture_label(*event), chord))
        .collect();
    named
        .chunks(3)
        .map(|c| c.join(", "))
        .collect::<Vec<_>>()
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                format!("{l},")
            } else {
                format!("{l}.")
            }
        })
        .collect()
}

fn gesture_label(event: KnobEvent) -> &'static str {
    match event {
        KnobEvent::RotateCCW => "CCW",
        KnobEvent::Press => "Press",
        KnobEvent::RotateCW => "CW",
        KnobEvent::HoldTwistL => "Hold+Twist L",
        KnobEvent::HoldTwistR => "Hold+Twist R",
    }
}

/// One firmware slot binding: slot key ID, device layer, chord action.
pub struct SlotBinding {
    pub key_id: u8,
    pub layer: u8,
    pub action: Action,
}

/// Build the binding plan for the given device layers. Pure: no HID I/O,
/// so this is fully unit-testable and powers CLI `--dry-run`.
///
/// `button_count` places the knob's slots, which follow the buttons in the
/// key-ID space. Passing the wrong count writes real packets into slots the
/// firmware never reads, and nothing anywhere reports a failure -- see
/// `protocol::key_id_for_knob`.
pub fn slot_binding_plan(button_count: usize, layers: &[u8]) -> Result<Vec<SlotBinding>> {
    let mut plan = Vec::new();
    for &layer in layers {
        for (event, chord) in SLOT_SPECS {
            plan.push(SlotBinding {
                key_id: key_id_for_knob(button_count, 0, event).with_context(|| {
                    format!("knob slot out of range for {button_count} button(s)")
                })?,
                layer,
                action: Action::parse(chord)?,
            });
        }
    }
    Ok(plan)
}

/// Serialize the plan to raw 64-byte HID packets (byte-for-byte identical
/// to what `flash_slot_bindings` sends).
pub fn binding_packets(button_count: usize, layers: &[u8]) -> Result<Vec<Vec<u8>>> {
    slot_binding_plan(button_count, layers)?
        .iter()
        .map(|b| Ok(b.action.to_packet(b.key_id, b.layer)))
        .collect()
}

/// Flash the plan to the device with inter-packet pacing, then commit.
/// Main thread only (same IOHIDManager thread-affinity contract as
/// `crate::device`).
pub fn flash_slot_bindings(dev: &HidDevice, button_count: usize, layers: &[u8]) -> Result<usize> {
    let mut sent = 0;
    for packet in binding_packets(button_count, layers)? {
        send_report(dev, &packet)?;
        sent += 1;
        std::thread::sleep(std::time::Duration::from_millis(SLOT_WRITE_GAP_MS));
    }
    crate::device::send_commit(dev)?;
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmware::BIND_LAYERS;
    use crate::host::{slot_chord, Gesture};

    /// The device layout this VK01 actually has: one button, so the knob's
    /// five gestures are keys 2..6. Used instead of the old `3` so the
    /// numbers in these tests are the numbers the hardware uses.
    const ONE_BUTTON: usize = 1;

    #[test]
    fn plan_covers_every_gesture_on_every_layer() {
        let plan = slot_binding_plan(ONE_BUTTON, &BIND_LAYERS).unwrap();
        assert_eq!(plan.len(), GESTURES_PER_KNOB * BIND_LAYERS.len());
        let ids: Vec<u8> = plan.iter().map(|b| b.key_id).collect();
        assert_eq!(
            ids,
            vec![2, 3, 4, 5, 6, 2, 3, 4, 5, 6, 2, 3, 4, 5, 6],
            "the knob spans five slots starting after the one button"
        );
        let layers: Vec<u8> = plan.iter().map(|b| b.layer).collect();
        assert_eq!(
            layers,
            vec![0; 5]
                .into_iter()
                .chain([1; 5])
                .chain([2; 5])
                .collect::<Vec<_>>()
        );
    }

    /// Hold+twist was the gesture this plan used to skip. The settings app
    /// offered it, the daemon listened for its chord, and no packet ever
    /// bound it -- so the row was decoration. Named for the defect so a
    /// future truncation of `SLOT_SPECS` fails with the reason attached.
    #[test]
    fn hold_twist_is_bound_rather_than_offered_and_never_flashed() {
        let plan = slot_binding_plan(ONE_BUTTON, &[0]).unwrap();
        let bound: Vec<(u8, &Action)> = plan.iter().map(|b| (b.key_id, &b.action)).collect();
        let f19 = Action::parse("ctrl-alt-shift-f19").unwrap();
        let f20 = Action::parse("ctrl-alt-shift-f20").unwrap();
        assert!(
            bound.contains(&(5, &f19)),
            "hold+twist left unbound: {bound:?}"
        );
        assert!(
            bound.contains(&(6, &f20)),
            "hold+twist right unbound: {bound:?}"
        );
    }

    /// The flasher and the decoder must name the same key for the same
    /// gesture. They live in different modules over different keycode
    /// spaces -- USB HID here, Core Graphics in `slot_chord` -- so nothing
    /// but this test stops one of them being edited alone.
    ///
    /// The table is the USB-HID-to-CGKeyCode mapping for F16..F20, the same
    /// pairing `super::super::slot_chord` documents.
    #[test]
    fn slot_specs_match_the_decoder() {
        const HID_TO_CG: [(u8, u16); 5] = [
            (0x6B, 106), // F16
            (0x6C, 64),  // F17
            (0x6D, 79),  // F18
            (0x6E, 80),  // F19
            (0x6F, 90),  // F20
        ];
        const AS_GESTURE: [(KnobEvent, Gesture); GESTURES_PER_KNOB] = [
            (KnobEvent::RotateCCW, Gesture::TwistL),
            (KnobEvent::Press, Gesture::Press),
            (KnobEvent::RotateCW, Gesture::TwistR),
            (KnobEvent::HoldTwistL, Gesture::HoldTwistL),
            (KnobEvent::HoldTwistR, Gesture::HoldTwistR),
        ];

        for (event, chord) in SLOT_SPECS {
            let Action::Key { modifiers, code } = Action::parse(chord).unwrap() else {
                panic!("{chord} is not a keyboard chord");
            };
            let gesture = AS_GESTURE
                .iter()
                .find(|(e, _)| *e == event)
                .map(|(_, g)| *g)
                .expect("every KnobEvent has a host Gesture");
            let (want_cg, want_mods) = slot_chord(gesture);
            let cg = HID_TO_CG
                .iter()
                .find(|(hid, _)| *hid == code)
                .map(|(_, cg)| *cg)
                .unwrap_or_else(|| panic!("{chord} is not one of F16..F20"));
            assert_eq!(cg, want_cg, "{chord} flashes a key the decoder ignores");
            assert_eq!(want_mods, ["ctrl", "alt", "shift"]);
            // USB HID modifier bits: LeftCtrl 0x01, LeftShift 0x02,
            // LeftAlt 0x04. The flasher's bits and the decoder's names are
            // two spellings of one agreement, which is the whole point of
            // asserting them in the same test.
            assert_eq!(
                modifiers,
                0x01 | 0x02 | 0x04,
                "{chord} carries the wrong modifiers"
            );
        }
    }

    /// The summary must NAME the chords that were flashed. This is the
    /// cheap gate for a whole class: prose restating what the code did,
    /// which goes stale silently because nothing compiles against English.
    #[test]
    fn the_summary_names_the_chords_that_are_flashed() {
        let text = chord_summary_lines().join(" ");
        for (_, chord) in SLOT_SPECS {
            assert!(
                text.contains(chord),
                "summary does not mention {chord}: {text}"
            );
        }
    }

    #[test]
    fn packets_match_expected_bytes() {
        let packets = binding_packets(ONE_BUTTON, &[0]).unwrap();
        assert_eq!(packets.len(), GESTURES_PER_KNOB);
        // CCW -> ctrl-alt-shift-F16 on layer 0, at key 2 on a one-button
        // device. Asserted as an EXECUTABLE record: 0xFD, an entry count at
        // byte 6, and the payload in the entry array from byte 7. The old
        // version of this test pinned 0xFE with bytes 10-12 and passed for
        // months over a record the firmware stored and never ran.
        let ccw = &packets[0];
        assert_eq!(&ccw[0..5], &[0x03, 0xFD, 2, 1, 1]);
        // Three modifier markers plus the key: a chord costs one entry per
        // modifier, which is why the count is not simply 1.
        assert_eq!(ccw[6], 4, "ctrl + alt + shift + F16");
        assert_eq!(ccw[9], crate::fd::modifier_entry(0), "ctrl marker first");
        assert_eq!(ccw[18], 0x6B, "F16 is the last entry");
        // The rest of the run, in slot order. The keycode is the LAST
        // declared entry, so its offset follows the count.
        for (i, (key_id, code)) in [(3u8, 0x6Cu8), (4, 0x6D), (5, 0x6E), (6, 0x6F)]
            .into_iter()
            .enumerate()
        {
            let p = &packets[i + 1];
            assert_eq!(p[2], key_id, "slot {i}");
            let last = crate::firmware::FD_HEADER_LEN
                + (p[6] as usize - 1) * crate::firmware::FD_ENTRY_LEN;
            assert_eq!(p[last + 2], code, "slot {i}");
        }
        // Layer 1 uses 1-based layer byte 2.
        let l1 = &binding_packets(ONE_BUTTON, &[1]).unwrap()[0];
        assert_eq!(l1[3], 2);
    }

    #[test]
    fn single_layer_subset_only_binds_that_layer() {
        let packets = binding_packets(ONE_BUTTON, &[2]).unwrap();
        assert_eq!(packets.len(), GESTURES_PER_KNOB);
        assert!(packets.iter().all(|p| p[3] == 3));
    }

    /// The button count still places the slots. A three-button layout puts
    /// the same five gestures at 4..8.
    #[test]
    fn the_button_count_still_places_the_run() {
        let plan = slot_binding_plan(3, &[0]).unwrap();
        let ids: Vec<u8> = plan.iter().map(|b| b.key_id).collect();
        assert_eq!(ids, vec![4, 5, 6, 7, 8]);
    }
}
