//! One-time slot bindings for host-side translation (Phase 2).
//!
//! "Bind once": programs the knob's firmware slots to fixed host chords
//! (`ctrl+alt+F16..F18`) so the translation engine in `super::engine` can
//! swallow them and run layered host actions. Only knob 0's three known
//! slots (CCW / Press / CW, key IDs 16-18) are bound here. Hold+twist slot
//! key IDs are unverified against this firmware, so hold gestures must
//! still be bound with the vendor app until they are reverse-engineered;
//! the UI states this explicitly instead of flashing blind packets.

use crate::device::send_report;
use crate::device::HidDevice;
use crate::protocol::{key_id_for_knob, Action, KnobEvent};
use anyhow::Result;

/// Device layers that carry the slot bindings (3-layer firmware model).
pub const BIND_LAYERS: [u8; 3] = [0, 1, 2];

/// (gesture, slot F-key name) pairs bound by the plan, in flash order.
const SLOT_SPECS: [(KnobEvent, &str); 3] = [
    (KnobEvent::RotateCCW, "ctrl-alt-f16"),
    (KnobEvent::Press, "ctrl-alt-f17"),
    (KnobEvent::RotateCW, "ctrl-alt-f18"),
];

/// One firmware slot binding: slot key ID, device layer, chord action.
pub struct SlotBinding {
    pub key_id: u8,
    pub layer: u8,
    pub action: Action,
}

/// Build the binding plan for the given device layers. Pure: no HID I/O,
/// so this is fully unit-testable and powers CLI `--dry-run`.
pub fn slot_binding_plan(layers: &[u8]) -> Result<Vec<SlotBinding>> {
    let mut plan = Vec::new();
    for &layer in layers {
        for (event, chord) in SLOT_SPECS {
            plan.push(SlotBinding {
                key_id: key_id_for_knob(0, event),
                layer,
                action: Action::parse(chord)?,
            });
        }
    }
    Ok(plan)
}

/// Serialize the plan to raw 64-byte HID packets (byte-for-byte identical
/// to what `flash_slot_bindings` sends).
pub fn binding_packets(layers: &[u8]) -> Result<Vec<Vec<u8>>> {
    slot_binding_plan(layers)?
        .iter()
        .map(|b| Ok(b.action.to_packet(b.key_id, b.layer)))
        .collect()
}

/// Flash the plan to the device with inter-packet pacing, then commit.
/// Main thread only (same IOHIDManager thread-affinity contract as
/// `crate::device`).
pub fn flash_slot_bindings(dev: &HidDevice, layers: &[u8]) -> Result<usize> {
    let mut sent = 0;
    for packet in binding_packets(layers)? {
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

    #[test]
    fn plan_covers_three_slots_per_layer() {
        let plan = slot_binding_plan(&BIND_LAYERS).unwrap();
        assert_eq!(plan.len(), 9);
        let ids: Vec<u8> = plan.iter().map(|b| b.key_id).collect();
        assert_eq!(ids, vec![16, 17, 18, 16, 17, 18, 16, 17, 18]);
        let layers: Vec<u8> = plan.iter().map(|b| b.layer).collect();
        assert_eq!(layers, vec![0, 0, 0, 1, 1, 1, 2, 2, 2]);
    }

    #[test]
    fn packets_match_expected_bytes() {
        let packets = binding_packets(&[0]).unwrap();
        assert_eq!(packets.len(), 3);
        // CCW -> ctrl-alt-F16 on layer 0.
        let ccw = &packets[0];
        assert_eq!(&ccw[0..5], &[0x03, 0xFE, 16, 1, 1]);
        assert_eq!(ccw[10], 1);
        assert_eq!(ccw[11], 0x01 | 0x04);
        assert_eq!(ccw[12], 0x6B);
        // Press -> ctrl-alt-F17.
        assert_eq!(packets[1][2], 17);
        assert_eq!(packets[1][12], 0x6C);
        // CW -> ctrl-alt-F18.
        assert_eq!(packets[2][2], 18);
        assert_eq!(packets[2][12], 0x6D);
        // Layer 1 uses 1-based layer byte 2.
        let l1 = &binding_packets(&[1]).unwrap()[0];
        assert_eq!(l1[3], 2);
    }

    #[test]
    fn single_layer_subset_only_binds_that_layer() {
        let packets = binding_packets(&[2]).unwrap();
        assert_eq!(packets.len(), 3);
        assert!(packets.iter().all(|p| p[3] == 3));
    }
}
