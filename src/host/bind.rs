//! One-time slot bindings for host-side translation (Phase 2).
//!
//! "Bind once": programs the knob's firmware slots to fixed host chords
//! (`ctrl+alt+F16..F20`) so the translation engine in `super::engine` can
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
use crate::protocol::{key_id_for_knob, Action, KnobEvent, GESTURES_PER_KNOB};
use anyhow::Result;

/// Device layers that carry the slot bindings (3-layer firmware model).
pub const BIND_LAYERS: [u8; 3] = [0, 1, 2];

/// (gesture, slot F-key name) pairs bound by the plan, in flash order.
///
/// One entry per `KnobEvent`, and the length is `GESTURES_PER_KNOB` so
/// adding a gesture to that enum fails to compile here rather than silently
/// shipping a gesture the daemon listens for and the firmware never sends.
/// The chords must match `super::slot_chord`, which is the decoder's side of
/// the same agreement; `slot_specs_match_the_decoder` pins that.
const SLOT_SPECS: [(KnobEvent, &str); GESTURES_PER_KNOB] = [
    (KnobEvent::RotateCCW, "ctrl-alt-f16"),
    (KnobEvent::Press, "ctrl-alt-f17"),
    (KnobEvent::RotateCW, "ctrl-alt-f18"),
    (KnobEvent::HoldTwistL, "ctrl-alt-f19"),
    (KnobEvent::HoldTwistR, "ctrl-alt-f20"),
];

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
                key_id: key_id_for_knob(button_count, 0, event),
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
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    crate::device::send_commit(dev)?;
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let f19 = Action::parse("ctrl-alt-f19").unwrap();
        let f20 = Action::parse("ctrl-alt-f20").unwrap();
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
            assert_eq!(want_mods, ["ctrl", "alt"]);
            assert_eq!(
                modifiers,
                0x01 | 0x04,
                "{chord} carries the wrong modifiers"
            );
        }
    }

    #[test]
    fn packets_match_expected_bytes() {
        let packets = binding_packets(ONE_BUTTON, &[0]).unwrap();
        assert_eq!(packets.len(), GESTURES_PER_KNOB);
        // CCW -> ctrl-alt-F16 on layer 0, at key 2 on a one-button device.
        let ccw = &packets[0];
        assert_eq!(&ccw[0..5], &[0x03, 0xFE, 2, 1, 1]);
        assert_eq!(ccw[10], 1);
        assert_eq!(ccw[11], 0x01 | 0x04);
        assert_eq!(ccw[12], 0x6B);
        // The rest of the run, in slot order.
        for (i, (key_id, code)) in [(3u8, 0x6Cu8), (4, 0x6D), (5, 0x6E), (6, 0x6F)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(packets[i + 1][2], key_id, "slot {i}");
            assert_eq!(packets[i + 1][12], code, "slot {i}");
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
