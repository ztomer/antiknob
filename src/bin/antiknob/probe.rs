//! `probe-gestures`: find out which slot a gesture drives, by experiment.
//!
//! Reading the slot table at a wider layer shows slots past the three bound
//! knob gestures exist, answer the read, and sit empty -- which is what an
//! unbound gesture looks like. This writes a distinct marker to each, has
//! the user perform the gestures, and reports which marker came out.
//!
//! It restores what it found. A diagnostic that leaves the hardware changed
//! is one nobody runs twice, and this one deliberately writes to slots whose
//! purpose is the thing being determined.

use antiknob::device;
use antiknob::host::gesture_probe::{self, ProbeSlot};

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

pub fn run(
    candidates: Vec<u8>,
    layer: u8,
    slots_per_layer: u8,
    capture_secs: u64,
    devices: Vec<String>,
) -> Result<()> {
    let plan = gesture_probe::plan(&candidates).map_err(|e| anyhow::anyhow!(e))?;

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
    println!("[ ==> ] Armed {}/{} marker(s):", confirmed, verdicts.len());
    for s in &plan {
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
    println!("        hold the knob and twist LEFT, then hold and twist RIGHT.");
    println!("        Capturing for {capture_secs}s...");
    let seen = capture(capture_secs, &devices)?;

    println!();
    println!("[ ==> ] Results:");
    if seen.is_empty() {
        println!("        Nothing was emitted. Either these slots are not the");
        println!("        gestures, or the gestures were not performed in time.");
    }
    for usage in &seen {
        match gesture_probe::slot_for_usage(&plan, *usage) {
            Some(key_id) => println!("        usage {usage:#06x} -> slot {key_id} IS a gesture"),
            None => println!("        usage {usage:#06x} -> not a probe marker (the knob's own)"),
        }
    }
    for s in &plan {
        if !seen.contains(&s.marker_usage) {
            println!("        slot {} never fired ({})", s.key_id, s.marker_name);
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
