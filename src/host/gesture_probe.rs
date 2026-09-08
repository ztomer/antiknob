//! Finding out which slot a gesture actually drives.
//!
//! `bind-slots` binds three of the knob's gestures. Hold+twist has never been
//! bound because nobody knows its key IDs, and reading the slot table does
//! not answer it: the slots past the knob's three exist and answer a read,
//! but 7 through 12 all hold the same generic factory placeholder, so their
//! presence is not evidence that a gesture drives any of them. An earlier
//! version of this note read "exists and is empty" as "is an unbound
//! gesture" and narrowed the probe to slots 7 and 8 on that basis.
//!
//! The question is answerable only by experiment. Write a *distinct*,
//! recognisable action to each candidate slot, perform the gestures, and see
//! which action comes out. The planning is here and pure; performing it lives
//! in the CLI.
//!
//! The one rule that makes the experiment mean anything: no two candidates
//! may share an action. Two slots emitting the same code would be
//! indistinguishable in the capture, which is the entire thing being
//! measured.

use crate::protocol::Action;

/// The pool of consumer usages a probe marker may be drawn from.
///
/// Chosen to be individually recognisable in a capture and harmless to
/// trigger: transport controls act on whatever is playing, which during a
/// deliberate probe is nothing.
///
/// A pool rather than the marker set, because which of these are USABLE
/// depends on the device in front of you. An earlier version was a fixed
/// list of six with one exclusion rule -- no volume, because the knob's own
/// twist emits it. That rule was right and far too narrow: on this VK01 the
/// three BUTTONS are bound to play, prev and next, which were three of the
/// six markers. A real run captured `0x00B6` because a button was pressed
/// during the window, and a marker set containing prev would have reported
/// that as the candidate slot firing -- a false positive produced by the
/// very instrument added to prevent false readings.
///
/// So the exclusion is no longer a hardcoded rule about volume. The probe
/// reads what the device actually emits and avoids all of it.
const MARKER_POOL: [(&str, u16); 9] = [
    ("stop", 0x00B7),
    ("play", 0x00CD),
    ("next", 0x00B5),
    ("prev", 0x00B6),
    ("brightnessup", 0x006F),
    ("brightnessdown", 0x0070),
    ("fastforward", 0x00B3),
    ("rewind", 0x00B4),
    ("eject", 0x00B8),
];

/// Usages the knob emits whatever is bound where.
///
/// The twist gestures drive volume through the very slots being probed, so
/// a volume marker could never be told from an ordinary turn.
const ALWAYS_EXCLUDED: [u16; 3] = [0x00E9, 0x00EA, 0x00E2];

/// Every consumer usage the device currently emits, read off its slot table.
///
/// A probe marker that collides with one of these is not a marker. The
/// buttons on this VK01 are bound to play, prev and next -- three of the six
/// markers the original set used -- so a button pressed during the capture
/// window read exactly like a probed slot firing. That is a false positive
/// manufactured by the instrument, which is worse than the silent run the
/// control was added to catch.
///
/// Media records carry a 16-bit usage little-endian at bytes 9-10; keyboard
/// and mouse records emit no consumer usage and are skipped.
pub fn usages_in_use(table: &[Vec<u8>]) -> Vec<u16> {
    let mut out = Vec::new();
    for record in table {
        if record.len() > 10 && record.get(4) == Some(&2) {
            let usage = u16::from(record[9]) | (u16::from(record[10]) << 8);
            if usage != 0 && !out.contains(&usage) {
                out.push(usage);
            }
        }
    }
    out
}

/// The markers usable on a device that already emits `in_use`.
///
/// Order is stable so a plan is reproducible: a probe whose marker
/// assignment shifted between runs would make two captures incomparable.
pub fn usable_markers(in_use: &[u16]) -> Vec<(&'static str, u16)> {
    MARKER_POOL
        .into_iter()
        .filter(|(_, usage)| !ALWAYS_EXCLUDED.contains(usage) && !in_use.contains(usage))
        .collect()
}

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
    plan_avoiding(candidates, &[])
}

/// Build the probe plan, avoiding every usage the device already emits.
///
/// A marker the device shares with a live binding is not a marker: pressing
/// that button during the capture window reads exactly like the probed slot
/// firing, and the run reports a gesture that does not exist.
pub fn plan_avoiding(candidates: &[u8], in_use: &[u16]) -> Result<Vec<ProbeSlot>, String> {
    if candidates.is_empty() {
        return Err("no candidate slots to probe".to_string());
    }
    let markers = usable_markers(in_use);
    if candidates.len() > markers.len() {
        return Err(format!(
            "{} candidate(s) but only {} marker(s) this device does not already \
             emit; probe them in smaller batches so each slot stays identifiable",
            candidates.len(),
            markers.len()
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
        .zip(markers)
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

/// A probe run: one control slot plus the candidates under test.
///
/// The control is a slot ALREADY KNOWN to be a gesture -- the knob's CCW
/// slot. It exists so that "nothing fired" means something. Without it a
/// silent run is ambiguous between "these slots are not the gesture" and
/// "the capture never saw anything at all", and the two are
/// indistinguishable from the outside. This repo has already published one
/// wrong conclusion drawn from exactly that ambiguity: hold+twist was
/// declared not to exist because a probe found nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbePlan {
    pub control: ProbeSlot,
    pub candidates: Vec<ProbeSlot>,
}

impl ProbePlan {
    /// Every slot the run writes to, control first.
    pub fn all(&self) -> Vec<ProbeSlot> {
        let mut out = vec![self.control.clone()];
        out.extend(self.candidates.iter().cloned());
        out
    }
}

/// What a capture window is allowed to conclude.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The control gesture never fired, so the run proves nothing about the
    /// candidates -- whether they are silent or the capture is.
    Inconclusive,
    /// The control fired and so did these candidates.
    Fired(Vec<u8>),
    /// The control fired and no candidate did. This is real evidence.
    NoneFired,
}

/// Plan a run: the control slot, then the candidates.
///
/// The control spends one of the markers, which is why the candidate budget
/// is one smaller than the marker set. That is the correct trade: five
/// candidates with a calibrated instrument answer the question, and six with
/// an uncalibrated one do not.
pub fn plan_with_control(control_key: u8, candidates: &[u8]) -> Result<ProbePlan, String> {
    plan_with_control_avoiding(control_key, candidates, &[])
}

/// `plan_with_control`, avoiding every usage the device already emits.
pub fn plan_with_control_avoiding(
    control_key: u8,
    candidates: &[u8],
    in_use: &[u16],
) -> Result<ProbePlan, String> {
    if candidates.contains(&control_key) {
        return Err(format!(
            "slot {control_key} is the control and cannot also be a candidate;              it would carry two markers and identify neither"
        ));
    }
    let mut all = vec![control_key];
    all.extend_from_slice(candidates);
    let slots = plan_avoiding(&all, in_use)?;
    let (control, candidates) = slots.split_first().expect("plan refuses an empty list");
    Ok(ProbePlan {
        control: control.clone(),
        candidates: candidates.to_vec(),
    })
}

/// Read the capture. The control decides whether anything may be concluded.
pub fn verdict(plan: &ProbePlan, seen: &[u16]) -> Verdict {
    if !seen.contains(&plan.control.marker_usage) {
        return Verdict::Inconclusive;
    }
    let fired: Vec<u8> = plan
        .candidates
        .iter()
        .filter(|s| seen.contains(&s.marker_usage))
        .map(|s| s.key_id)
        .collect();
    if fired.is_empty() {
        Verdict::NoneFired
    } else {
        Verdict::Fired(fired)
    }
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

    /// The VK01's real layer 1, as read off the device: three buttons bound
    /// to play/prev/next and the knob to volume down/mute/up.
    fn vk01_layer_one() -> Vec<Vec<u8>> {
        [0x00CDu16, 0x00B6, 0x00B5, 0x00EA, 0x00E2, 0x00E9]
            .iter()
            .enumerate()
            .map(|(i, usage)| {
                let mut r = vec![0u8; 13];
                r[0] = 0x03;
                r[1] = 0xFA;
                r[2] = (i + 1) as u8;
                r[3] = 1;
                r[4] = 2; // media
                r[9] = (*usage & 0xFF) as u8;
                r[10] = (*usage >> 8) as u8;
                r
            })
            .collect()
    }

    #[test]
    fn the_devices_own_usages_are_read_off_its_slot_table() {
        let found = usages_in_use(&vk01_layer_one());
        assert_eq!(found.len(), 6);
        for usage in [0x00CDu16, 0x00B6, 0x00B5, 0x00EA, 0x00E2, 0x00E9] {
            assert!(
                found.contains(&usage),
                "{usage:#06x} missing from {found:?}"
            );
        }
        // Keyboard and mouse records carry no consumer usage.
        let mut kbd = vec![0u8; 13];
        kbd[4] = 1;
        kbd[9] = 0xB7;
        assert!(usages_in_use(&[kbd]).is_empty());
        // An empty media slot is not a usage in use.
        let mut blank = vec![0u8; 13];
        blank[4] = 2;
        assert!(usages_in_use(&[blank]).is_empty());
    }

    /// The defect a real run exposed. A button bound to `prev` makes `prev`
    /// useless as a marker: pressing it during the capture window is
    /// indistinguishable from the probed slot firing, and the run reports a
    /// gesture that does not exist.
    #[test]
    fn no_marker_collides_with_a_binding_the_device_already_has() {
        let in_use = usages_in_use(&vk01_layer_one());
        let p = plan_avoiding(&[7, 8, 9, 10, 11], &in_use).expect("plan");
        for s in &p {
            assert!(
                !in_use.contains(&s.marker_usage),
                "{} ({:#06x}) is already bound on this device",
                s.marker_name,
                s.marker_usage
            );
        }
        // And the control gets the same treatment.
        let withc = plan_with_control_avoiding(4, &[7, 8, 9, 10, 11], &in_use).expect("plan");
        assert!(!in_use.contains(&withc.control.marker_usage));
    }

    /// Running out of usable markers is a refusal, not a silent reuse.
    /// Reusing one would make two slots indistinguishable, which is the
    /// whole thing being measured.
    #[test]
    fn too_few_usable_markers_is_refused_and_says_why() {
        // Everything in the pool is already bound: nothing is usable.
        let all: Vec<u16> = usable_markers(&[]).iter().map(|(_, u)| *u).collect();
        let err = plan_avoiding(&[7], &all).expect_err("must refuse");
        assert!(err.contains("does not already"), "{err}");
        assert!(usable_markers(&all).is_empty());
    }

    /// Marker assignment must not shift between runs, or two captures of
    /// the same device cannot be compared.
    #[test]
    fn the_marker_assignment_is_stable_across_runs() {
        let in_use = usages_in_use(&vk01_layer_one());
        let a = plan_avoiding(&[7, 8, 9], &in_use).expect("plan");
        let b = plan_avoiding(&[7, 8, 9], &in_use).expect("plan");
        assert_eq!(a, b);
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

    /// The control is what makes a silent run mean anything. Without it,
    /// "no candidate fired" and "the capture saw nothing" are the same
    /// observation -- and this repo shipped a wrong claim off exactly that.
    #[test]
    fn a_run_whose_control_never_fired_concludes_nothing() {
        let p = plan_with_control(4, &[7, 8]).expect("plan");
        // Candidate 7 fired but the control did not: the run is still
        // inconclusive, because a capture that misses the control is not
        // one whose silences can be trusted.
        let seen = vec![p.candidates[0].marker_usage];
        assert_eq!(verdict(&p, &seen), Verdict::Inconclusive);
        assert_eq!(verdict(&p, &[]), Verdict::Inconclusive);
    }

    /// With the control seen, silence from the candidates is evidence.
    #[test]
    fn a_control_that_fired_licenses_the_candidate_result() {
        let p = plan_with_control(4, &[7, 8]).expect("plan");
        assert_eq!(verdict(&p, &[p.control.marker_usage]), Verdict::NoneFired);
        let both = vec![p.control.marker_usage, p.candidates[1].marker_usage];
        assert_eq!(verdict(&p, &both), Verdict::Fired(vec![8]));
    }

    /// The control needs a marker no candidate has, or it cannot be told
    /// apart from the thing it is calibrating.
    #[test]
    fn the_control_carries_a_marker_of_its_own() {
        let p = plan_with_control(4, &[7, 8, 9, 10, 11]).expect("plan");
        assert_eq!(p.control.key_id, 4);
        assert_eq!(p.candidates.len(), 5);
        for c in &p.candidates {
            assert_ne!(c.marker_usage, p.control.marker_usage);
        }
        assert_eq!(p.all().len(), 6, "the control is written like any other");
    }

    /// A control that is also a candidate would carry two markers.
    #[test]
    fn the_control_may_not_also_be_a_candidate() {
        let err = plan_with_control(7, &[7, 8]).expect_err("must refuse");
        assert!(err.contains("control"), "{err}");
    }
}
