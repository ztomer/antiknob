//! Finding out which slot a gesture actually drives.
//!
//! `bind-slots` binds three of the knob's gestures. Hold+twist has never been
//! bound because nobody knew its key IDs, and `PLAN.md` recorded that as
//! needing the vendor app to diff against. It does not: reading the slot
//! table at a wider layer shows slots 7 and 8 exist, answer the read, and sit
//! empty -- exactly what an unbound gesture looks like.
//!
//! So the question is answerable by experiment. Write a *distinct*,
//! recognisable action to each candidate slot, perform the gestures, and see
//! which action comes out. The planning is here and pure; performing it lives
//! in the CLI.
//!
//! The one rule that makes the experiment mean anything: no two candidates
//! may share an action. Two slots emitting the same code would be
//! indistinguishable in the capture, which is the entire thing being
//! measured.

use crate::protocol::Action;

/// Consumer usages used as probe markers.
///
/// Chosen to be individually recognisable in a capture and harmless to
/// trigger: transport controls act on whatever is playing, which during a
/// deliberate probe is nothing. Volume is deliberately NOT here -- the
/// knob's own bindings already emit it, so a volume marker could not be told
/// apart from an ordinary twist.
const MARKERS: [(&str, u16); 6] = [
    ("stop", 0x00B7),
    ("play", 0x00CD),
    ("next", 0x00B5),
    ("prev", 0x00B6),
    ("brightnessup", 0x006F),
    ("brightnessdown", 0x0070),
];

/// One slot to probe, and the marker it will carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeSlot {
    pub key_id: u8,
    pub marker_name: &'static str,
    pub marker_usage: u16,
}

/// Build the probe plan for a set of candidate key IDs.
///
/// Fails rather than truncating when there are more candidates than
/// markers: a plan that silently probed only the first six slots would
/// report "gesture not found" for slots it never wrote to.
pub fn plan(candidates: &[u8]) -> Result<Vec<ProbeSlot>, String> {
    if candidates.is_empty() {
        return Err("no candidate slots to probe".to_string());
    }
    if candidates.len() > MARKERS.len() {
        return Err(format!(
            "{} candidates but only {} distinct markers; probe them in smaller batches \
             so each slot stays identifiable",
            candidates.len(),
            MARKERS.len()
        ));
    }
    let mut seen = Vec::new();
    for c in candidates {
        if seen.contains(c) {
            return Err(format!("candidate slot {c} listed twice"));
        }
        seen.push(*c);
    }
    Ok(candidates
        .iter()
        .zip(MARKERS)
        .map(|(key_id, (marker_name, marker_usage))| ProbeSlot {
            key_id: *key_id,
            marker_name,
            marker_usage,
        })
        .collect())
}

/// The packets that arm the probe on one device layer.
pub fn probe_packets(plan: &[ProbeSlot], layer: u8) -> Result<Vec<Vec<u8>>, String> {
    plan.iter()
        .map(|s| {
            Action::parse(s.marker_name)
                .map(|a| a.to_packet(s.key_id, layer))
                .map_err(|e| format!("marker {} is not a valid action: {e}", s.marker_name))
        })
        .collect()
}

/// Which slot emitted a captured consumer usage, if any.
pub fn slot_for_usage(plan: &[ProbeSlot], usage: u16) -> Option<u8> {
    plan.iter()
        .find(|s| s.marker_usage == usage)
        .map(|s| s.key_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole experiment rests on this. Two slots sharing a marker would
    /// be indistinguishable in the capture -- and the failure would look
    /// like a gesture that does not exist rather than a broken probe.
    #[test]
    fn every_candidate_gets_a_marker_no_other_candidate_has() {
        let p = plan(&[7, 8, 9, 10]).expect("plan");
        assert_eq!(p.len(), 4);
        let usages: Vec<u16> = p.iter().map(|s| s.marker_usage).collect();
        let mut sorted = usages.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), usages.len(), "markers repeat: {usages:?}");
    }

    /// No marker may collide with what the knob already emits, or an
    /// ordinary twist would read as a probe hit.
    #[test]
    fn no_marker_is_a_volume_usage() {
        let p = plan(&[7, 8, 9, 10, 11, 12]).expect("plan");
        for s in &p {
            assert!(
                ![0x00E9u16, 0x00EA, 0x00E2].contains(&s.marker_usage),
                "{} collides with the knob's own bindings",
                s.marker_name
            );
        }
    }

    /// Truncating would report "not found" for slots never written to.
    #[test]
    fn more_candidates_than_markers_is_refused_rather_than_truncated() {
        let too_many: Vec<u8> = (7..=20).collect();
        let err = plan(&too_many).expect_err("must refuse");
        assert!(err.contains("smaller batches"), "{err}");
        assert!(plan(&[]).is_err(), "an empty probe measures nothing");
        assert!(plan(&[7, 7]).is_err(), "a repeated slot would double-write");
    }

    #[test]
    fn the_packets_address_the_candidate_slots_on_the_given_layer() {
        let p = plan(&[7, 8]).expect("plan");
        let packets = probe_packets(&p, 0).expect("packets");
        assert_eq!(packets.len(), 2);
        // 03 FE <key_id> <layer+1> <kind=media> ... <usage low at byte 9>
        assert_eq!(packets[0][2], 7);
        assert_eq!(packets[1][2], 8);
        assert_eq!(packets[0][3], 1, "layer byte is 1-based");
        assert_eq!(packets[0][4], 2, "media kind");
        assert_eq!(u16::from(packets[0][9]), p[0].marker_usage);
        assert_eq!(u16::from(packets[1][9]), p[1].marker_usage);
    }

    #[test]
    fn a_captured_usage_names_the_slot_that_emitted_it() {
        let p = plan(&[7, 8]).expect("plan");
        assert_eq!(slot_for_usage(&p, p[0].marker_usage), Some(7));
        assert_eq!(slot_for_usage(&p, p[1].marker_usage), Some(8));
        // Volume is the knob's own; it must never be attributed to a probe.
        assert_eq!(slot_for_usage(&p, 0x00E9), None);
    }
}
