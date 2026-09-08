//! `probe-gestures`: find out which slot a gesture drives, by experiment.
//!
//! Reading the slot table at a wider layer shows slots past the three bound
//! knob gestures exist and answer the read. That is ALL it shows: slots 7
//! through 12 hold the same generic factory placeholder (`key N -> the Nth
//! letter`), which is a table sized for a bigger sibling, not a gesture map.
//! An earlier note read those slots as "empty, which is what an unbound
//! gesture looks like" and probed only 7 and 8 on the strength of it.
//!
//! So the slot has to be found by experiment, not inference. This writes a
//! distinct marker to each candidate, has the user perform the gestures, and
//! reports which marker came out -- the one form of evidence that actually
//! distinguishes a gesture slot from a slot that merely exists.
//!
//! It restores what it found. A diagnostic that leaves the hardware changed
//! is one nobody runs twice, and this one deliberately writes to slots whose
//! purpose is the thing being determined.

use antiknob::device;
use antiknob::host::gesture_probe::{self, ProbeSlot, Verdict};

use anyhow::{Context, Result};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Read every candidate slot's current contents so they can be put back.
fn snapshot(slots: &[ProbeSlot], layer: u8, slots_per_layer: u8) -> Result<Vec<Vec<u8>>> {
    let wanted: Vec<u8> = slots.iter().map(|s| s.key_id).collect();
    let table = device::with_device(move |dev| {
        Ok(device::read_slot_table(
            dev,
            slots_per_layer,
            device::DEVICE_LAYERS,
        ))
    })
    .context("cannot read the slot table; refusing to write to slots it cannot restore")?;
    Ok(table
        .into_iter()
        .filter(|r| {
            device::verify::parse_record(r)
                .is_some_and(|(a, _)| a.layer == layer + 1 && wanted.contains(&a.key_id))
        })
        .collect())
}

/// Map EVERY gesture at once: which key id does each one drive?
///
/// The ordinary probe assumes it already knows which key is which -- its
/// control sits on the knob's CCW slot. That assumption is exactly what was
/// in doubt, and this command is what settled it.
///
/// SETTLED on a VK01: the knob is FIVE gestures at keys 2-6, one button at
/// key 1, and `key_id_for_knob` agrees given the declared count of one
/// button. Re-measured 2026-09-08 through the fixed `0xFD` encoder, because
/// the first run went through the `0xFE` one, whose records the firmware
/// stored and never executed -- a probe writing markers nothing runs would
/// have reported silence as an answer. It fired 2, 3, 4, 5, 6 in gesture
/// order both times.
///
/// This settles it without assuming anything: put a distinct marker on
/// EVERY key in the range, perform every gesture, and read which key each
/// one emitted. There is no control because there is nothing to control
/// against -- but a run where nothing at all fires is reported as measuring
/// nothing rather than as five silent keys.
pub fn run_map(keys: Vec<u8>, layer: u8, capture_secs: u64, devices: Vec<String>) -> Result<()> {
    println!("[ ==> ] Reading the device's own bindings so the markers cannot collide...");
    let table = device::with_device(|dev| Ok(device::read_full_table(dev)))?;
    // Only usages OUTSIDE the keys being overwritten matter: the ones on
    // those keys are about to be replaced, so reserving markers against them
    // would spend the budget for nothing.
    let outside: Vec<Vec<u8>> = table
        .into_iter()
        .filter(|r| {
            device::verify::parse_record(r)
                .is_none_or(|(a, _)| !keys.contains(&a.key_id) || a.layer != layer + 1)
        })
        .collect();
    let in_use = gesture_probe::usages_in_use(&outside);

    let plan = gesture_probe::plan_avoiding(&keys, &in_use).map_err(|e| anyhow::anyhow!(e))?;
    let width = keys.iter().copied().max().unwrap_or(6);

    println!("[ ==> ] Reading the current contents of the candidate slots first...");
    let before = snapshot(&plan, layer, width)?;
    println!("        {} slot(s) captured for restore.", before.len());

    let packets = gesture_probe::probe_packets(&plan, layer).map_err(|e| anyhow::anyhow!(e))?;
    let armed = packets.clone();
    device::with_device(move |dev| {
        for p in &armed {
            device::send_report(dev, p)?;
            sleep(Duration::from_millis(15));
        }
        device::send_commit(dev)
    })
    .context("could not arm the probe")?;

    let check = device::with_device(move |dev| {
        Ok(device::read_slot_table(dev, width, device::DEVICE_LAYERS))
    })?;
    let verdicts = device::verify::verify(&packets, &check);
    let confirmed = verdicts
        .iter()
        .filter(|(_, v)| *v == device::verify::SlotVerdict::Confirmed)
        .count();
    println!("[ ==> ] Armed {}/{} marker(s):", confirmed, verdicts.len());
    for s in &plan {
        println!("        key {} -> {}", s.key_id, s.marker_name);
    }
    if confirmed != verdicts.len() {
        println!("[ Wrn ] Not every marker landed; a silent key below would be a");
        println!("        failed write rather than a key no gesture drives. Stopping.");
        restore(&before);
        return Ok(());
    }

    println!();
    println!("[ ==> ] Now perform each gesture ONCE, slowly, in this order:");
    println!("        1. twist counter-clockwise");
    println!("        2. press");
    println!("        3. twist clockwise");
    println!("        4. hold and twist LEFT");
    println!("        5. hold and twist RIGHT");
    println!("        Do NOT press any buttons. Capturing for {capture_secs}s...");
    let seen = capture(capture_secs, &devices)?;

    println!();
    println!("[ ==> ] Results, in the order the device emitted them:");
    let mut fired = Vec::new();
    for usage in &seen {
        match gesture_probe::slot_for_usage(&plan, *usage) {
            Some(key_id) => {
                fired.push(key_id);
                println!("        key {key_id} fired  (usage {usage:#06x})");
            }
            None => println!("        usage {usage:#06x} is not one of the markers"),
        }
    }
    println!();
    if fired.is_empty() {
        println!("[ Wrn ] INCONCLUSIVE: nothing fired, so this run measured nothing.");
        println!("        Re-run and make sure a gesture happens inside the window.");
    } else {
        println!(
            "[ Ok  ] {} of {} key(s) are driven by a gesture.",
            fired.len(),
            plan.len()
        );
        for s in &plan {
            if !fired.contains(&s.key_id) {
                println!("        key {} was never driven", s.key_id);
            }
        }
        println!();
        println!("        The ORDER above is the answer: the first key listed is the");
        println!("        gesture you performed first. Compare it against");
        // Derived from the DECLARED layout and all five gestures, never a
        // literal. This line used to hard-code three buttons and three
        // gestures, so on a one-button knob it printed "[4, 5, 6]" directly
        // beneath a measurement of keys 2-6 -- a summary contradicting the
        // evidence above it, in the one command whose entire job is to
        // settle that question. It also outlived the fix that made a knob
        // five gestures wide, because nothing compiles against a `println!`.
        match super::binding::button_count_for(None) {
            Ok(buttons) => {
                let expected: Vec<u8> = antiknob::protocol::KnobEvent::ALL
                    .iter()
                    .map(|e| antiknob::protocol::key_id_for_knob(buttons, 0, *e))
                    .collect();
                println!(
                    "        `protocol::key_id_for_knob`, which for the declared {buttons} \
                     button(s)"
                );
                println!("        says the knob is at {expected:?}.");
            }
            // Reported rather than defaulted: a guessed button count is
            // exactly how this line went wrong, and a comparison against a
            // made-up model is worse than none.
            Err(e) => {
                println!("        `protocol::key_id_for_knob` -- but the layout could not be");
                println!("        read, so there is nothing to compare against: {e}");
            }
        }
    }

    restore(&before);
    Ok(())
}

pub fn run(
    control: u8,
    candidates: Vec<u8>,
    layer: u8,
    slots_per_layer: u8,
    capture_secs: u64,
    devices: Vec<String>,
) -> Result<()> {
    println!("[ ==> ] Reading the device's own bindings so the markers cannot collide...");
    let table = device::with_device(move |dev| {
        Ok(device::read_slot_table(
            dev,
            slots_per_layer,
            device::DEVICE_LAYERS,
        ))
    })
    .context("cannot read the slot table; refusing to probe blind")?;
    let in_use = gesture_probe::usages_in_use(&table);
    println!(
        "        {} usage(s) this device already emits; markers avoid every one.",
        in_use.len()
    );

    let probe = gesture_probe::plan_with_control_avoiding(control, &candidates, &in_use)
        .map_err(|e| anyhow::anyhow!(e))?;
    let plan = probe.all();

    println!("[ ==> ] Reading the current contents of the candidate slots first...");
    let before = snapshot(&plan, layer, slots_per_layer)?;
    println!("        {} slot(s) captured for restore.", before.len());

    let packets = gesture_probe::probe_packets(&plan, layer).map_err(|e| anyhow::anyhow!(e))?;
    let armed = packets.clone();
    device::with_device(move |dev| {
        for p in &armed {
            device::send_report(dev, p)?;
            sleep(Duration::from_millis(15));
        }
        device::send_commit(dev)
    })
    .context("could not arm the probe")?;

    // Confirm the markers actually landed. Without this a gesture that
    // produces nothing is ambiguous: unbound slot, or a write that missed?
    let check = device::with_device(move |dev| {
        Ok(device::read_slot_table(
            dev,
            slots_per_layer,
            device::DEVICE_LAYERS,
        ))
    })?;
    let verdicts = device::verify::verify(&packets, &check);
    let confirmed = verdicts
        .iter()
        .filter(|(_, v)| *v == device::verify::SlotVerdict::Confirmed)
        .count();
    // Say which candidates were never configured slots to begin with. Every
    // key id this firmware is asked about answers, so "the slot exists" is
    // not a finding -- and the hold+twist search started from exactly that
    // fabrication being read as one.
    let fabricated: Vec<u8> = before
        .iter()
        .filter(|r| device::verify::is_synthetic_default(r))
        .filter_map(|r| device::verify::parse_record(r).map(|(a, _)| a.key_id))
        .collect();
    if !fabricated.is_empty() {
        println!(
            "[ --- ] Slot(s) {:?} hold the firmware's synthetic default, not a",
            fabricated
        );
        println!("        configured binding. Every key id answers this device, so their");
        println!("        existing is not evidence that a gesture drives them.");
    }
    println!("[ ==> ] Armed {}/{} marker(s):", confirmed, verdicts.len());
    println!(
        "        slot {} -> {}   (CONTROL: a gesture already known to work)",
        probe.control.key_id, probe.control.marker_name
    );
    for s in &probe.candidates {
        println!("        slot {} -> {}", s.key_id, s.marker_name);
    }
    if confirmed != verdicts.len() {
        println!("[ Wrn ] Not every marker landed; a silent gesture below may be a");
        println!("        failed write rather than an unbound slot. Restoring and stopping.");
        restore(&before);
        return Ok(());
    }

    println!();
    println!("[ ==> ] Now perform each gesture in turn, several times each:");
    println!("        FIRST the control: twist the knob counter-clockwise.");
    println!("        Then hold the knob and twist LEFT, then hold and twist RIGHT.");
    println!("        Capturing for {capture_secs}s...");
    let seen = capture(capture_secs, &devices)?;

    println!();
    println!("[ ==> ] Results:");
    for usage in &seen {
        match gesture_probe::slot_for_usage(&plan, *usage) {
            Some(key_id) => println!("        usage {usage:#06x} -> slot {key_id} fired"),
            None => println!("        usage {usage:#06x} -> not a probe marker (the knob's own)"),
        }
    }
    match gesture_probe::verdict(&probe, &seen) {
        Verdict::Inconclusive => {
            println!();
            println!("[ Wrn ] INCONCLUSIVE: the control slot never fired.");
            println!("        The control is a gesture that is known to work, so a run");
            println!("        that misses it is measuring nothing -- a silent candidate");
            println!("        here is not evidence that the slot is unbound. Re-run and");
            println!("        make sure to twist the knob during the capture window.");
        }
        Verdict::Fired(slots) => {
            println!();
            for key_id in &slots {
                println!("[ Ok  ] Slot {key_id} IS driven by one of the gestures performed.");
            }
        }
        Verdict::NoneFired => {
            println!();
            println!("[ Ok  ] The control fired, so the capture was working.");
            println!("        No candidate slot fired: on this firmware none of");
            println!("        {candidates:?} is driven by the gestures performed.");
        }
    }

    restore(&before);
    Ok(())
}

/// Put the candidate slots back, retrying while the capture's handles clear.
///
/// The capture opens every interface of the device non-exclusively, and the
/// first restore attempt after it reliably fails with "Failed to write HID
/// report" -- macOS has not finished tearing those handles down. Retrying is
/// not optimism here: the probe deliberately overwrites slots whose purpose
/// is the thing being measured, so leaving them changed would be worse than
/// never having run. Failure after every attempt prints the exact commands
/// to put them back by hand.
fn restore(before: &[Vec<u8>]) {
    if before.is_empty() {
        println!("[ --- ] The candidate slots were empty; nothing to restore.");
        return;
    }
    let packets: Vec<Vec<u8>> = before.iter().map(|r| to_write(r)).collect();
    let mut last = String::new();
    for attempt in 1..=5 {
        sleep(Duration::from_millis(300 * attempt));
        let batch = packets.clone();
        match device::with_device(move |dev| {
            for p in &batch {
                device::send_report(dev, p)?;
                sleep(Duration::from_millis(15));
            }
            device::send_commit(dev)
        }) {
            Ok(()) => match confirm_restored(before, &packets) {
                Ok(()) => {
                    println!("[ Ok  ] Candidate slots restored, and read back to confirm.");
                    return;
                }
                Err(e) => last = e.to_string(),
            },
            Err(e) => last = e.to_string(),
        }
    }
    println!("[ Wrn ] Could not restore the candidate slots after 5 attempts: {last}");
    println!("        Put them back with: antiknob upload");
}

/// Read the candidate slots again and check they hold what they held.
///
/// The write returning Ok says the packet was accepted, which this session
/// has already learned is not the same as the slot changing -- and a probe
/// that says "restored" without looking would leave the knob carrying probe
/// markers while reporting that it does not.
fn confirm_restored(before: &[Vec<u8>], packets: &[Vec<u8>]) -> Result<()> {
    let width = before
        .iter()
        .filter_map(|r| device::verify::parse_record(r).map(|(a, _)| a.key_id))
        .max()
        .unwrap_or(device::DEVICE_LAYERS);
    let table = device::with_device(move |dev| {
        Ok(device::read_slot_table(dev, width, device::DEVICE_LAYERS))
    })?;
    let verdicts = device::verify::verify(packets, &table);
    match device::verify::summarize(&verdicts) {
        None => Ok(()),
        Some(problem) => Err(anyhow::anyhow!(problem)),
    }
}

/// Turn a read record back into the write that reproduces it. The two share
/// every offset; only the tag byte differs (0xFA read, 0xFE write).
fn to_write(record: &[u8]) -> Vec<u8> {
    let mut p = record.to_vec();
    p.resize(64, 0);
    p[1] = 0xFE;
    p
}

/// Collect the distinct consumer usages the knob emits during the window.
fn capture(secs: u64, devices: &[String]) -> Result<Vec<u16>> {
    let filters = devices
        .iter()
        .map(|d| device::snoop::parse_device_filter(d).map_err(|e| anyhow::anyhow!(e)))
        .collect::<Result<Vec<_>>>()?;
    let deadline = Instant::now() + Duration::from_secs(secs);
    device::with_hid(move |api| {
        let ifaces = device::open_all_interfaces_on(api, &filters)?;
        let mut seen: Vec<u16> = Vec::new();
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            for iface in &ifaces {
                if let Ok(n) = iface.device.read_timeout(&mut buf, 20) {
                    // A consumer report is a report id plus a 16-bit usage.
                    if n == 3 && buf[1] != 0 {
                        let usage = u16::from(buf[1]) | (u16::from(buf[2]) << 8);
                        if !seen.contains(&usage) {
                            seen.push(usage);
                        }
                    }
                }
            }
        }
        Ok(seen)
    })
}
