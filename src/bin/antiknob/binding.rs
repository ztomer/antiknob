//! Slot-binding commands, split out of `cmds.rs` for the file-length gate.
//!
//! Both commands write firmware slots, and they are the two that have to
//! care about WHERE a slot is: `bind-slots` derives the knob's three slots
//! from the declared button count, and `bind-seq` is handed a key id
//! directly. Grouping them keeps that reasoning in one file.

use crate::cmds::layout_path;
use antiknob::{config, device, fd, host};

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

/// The declared button count, which is what places every knob slot.
pub fn button_count_for(config: Option<PathBuf>) -> Result<usize> {
    resolve_button_count(config, None)
}

fn resolve_button_count(config: Option<PathBuf>, buttons: Option<usize>) -> Result<usize> {
    if let Some(n) = buttons {
        return Ok(n);
    }
    let path = layout_path(config)?;
    let cfg = config::DeviceConfig::load_from_file(&path).with_context(|| {
        format!(
            "cannot read the hardware layout from {} -- pass --config <file> or \
             --buttons <n> so knob slots land where the firmware reads them",
            path.display()
        )
    })?;
    Ok(cfg.button_count())
}

pub fn run_bind_slots(
    config: Option<PathBuf>,
    buttons: Option<usize>,
    layer: Option<u8>,
    dry_run: bool,
) -> Result<()> {
    let buttons = resolve_button_count(config, buttons)?;
    println!(
        "[ ==> ] Layout has {} button(s); knob slots follow them.",
        buttons
    );
    let layers: Vec<u8> = match layer {
        Some(l) => vec![l],
        None => host::bind::BIND_LAYERS.to_vec(),
    };
    if dry_run {
        println!("[ ==> ] Slot binding plan (dry run, no hardware touched):");
        for packet in host::bind::binding_packets(buttons, &layers)? {
            // Decoded from the record's OWN length and entry array, not from
            // fixed byte offsets. The offsets version printed bytes 11/12 and
            // reported `mods=0x00 code=0xf3` for all fifteen slots -- byte 12
            // is the middle of entry #1, so it described a record that was
            // never written. A dry run that cannot be trusted is worse than
            // none: this one exists so the plan can be read BEFORE hardware
            // is touched.
            let entries: Vec<String> = (0..packet[6] as usize)
                .map(|i| {
                    format!(
                        "{:02x}",
                        packet[antiknob::fd::HEADER_LEN + i * antiknob::fd::ENTRY_LEN + 2]
                    )
                })
                .collect();
            println!(
                "        layer byte={} key_id={} kind={} entries={} [{}]",
                packet[3],
                packet[2],
                packet[4],
                packet[6],
                entries.join(" ")
            );
        }
        // Counted from the plan, not restated as a literal. The literal
        // said "3 slots" and "hold+twist NOT bound" for as long as it took
        // the plan to grow to five -- a summary that contradicts the lines
        // printed directly above it.
        println!(
            "        {} slots x {} layer(s): CCW/press/CW/hold+twist L/hold+twist R.",
            antiknob::protocol::GESTURES_PER_KNOB,
            layers.len()
        );
        return Ok(());
    }
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let flashed = layers.clone();
    let sent =
        device::with_device(move |dev| host::bind::flash_slot_bindings(dev, buttons, &flashed))?;
    println!("[ Ok  ] Flashed {sent} slot binding(s).");
    // Printed FROM the table that was flashed, never restated as a literal.
    // The literal version outlived two changes to the chords: it still said
    // "ctrl-alt-F16" immediately after a run that wrote ctrl-alt-SHIFT-F16,
    // which is a success message describing a flash that did not happen.
    for line in host::bind::chord_summary_lines() {
        println!("        {line}");
    }
    let recorded = host::default_config_path()
        .map_err(anyhow::Error::from)
        .and_then(|path| host::device_binding::record_bound_layers(&path, &layers));
    match recorded {
        // Every case records now. "All three" used to record NOTHING and
        // print a reassuring sentence about it, which left the config saying
        // the same thing as a knob that had never been flashed -- so the
        // per-layer backlight, which needs to know where to write, silently
        // did nothing on the most common flash there is.
        Ok(bound) => {
            let arrangement = host::device_binding::arrangement(&bound, device::DEVICE_LAYERS);
            println!("[ Ok  ] {}", arrangement.describe());
            println!("        Re-run with `--layer N` to bind just one.");
        }
        Err(e) => {
            // The flash landed; only the bookkeeping failed. Saying so beats
            // reporting the whole command as failed.
            println!("[ Wrn ] Slots flashed, but host.json could not record the layer: {e}");
        }
    }
    println!("        Verify with: antiknob listen --timeout-secs 10");
    Ok(())
}
/// Bind one slot to a sequence of actions over the vendor's `0xFD` command.
///
/// Every write is read back before it is called a success. That is not
/// belt-and-braces here: the whole reason this repo has a `verify` module is
/// that knob bindings were flashed into slots the firmware never reads for
/// months, with `hid_write` returning cleanly every time. A `0xFD` write
/// lands in the same slot table the `FA` read exposes, so there is no excuse
/// for reporting an unconfirmed write as a good one.
pub fn run_bind_seq(
    key: u8,
    layer: u8,
    width: Option<u8>,
    dry_run: bool,
    delay_ms: u16,
    actions: Vec<String>,
) -> Result<()> {
    if layer >= device::DEVICE_LAYERS {
        anyhow::bail!(
            "layer {} does not exist; this firmware has layers 0..{}",
            layer,
            device::DEVICE_LAYERS - 1
        );
    }
    let mut parsed = fd::parse_sequence(&actions)?;
    // The delay belongs to every step after the first: a wait before the
    // opening keystroke only slows the gesture down. The vendor's own
    // default is 50 ms between steps, which is what the 0x32 bytes filling
    // its records are.
    for step in parsed.iter_mut().skip(1) {
        step.delay_ms = delay_ms;
    }
    let packet = fd::build_packet(key, layer, &parsed)?;

    let hex: Vec<String> = packet[..13].iter().map(|b| format!("{b:02x}")).collect();
    println!(
        "[ ==> ] Slot key={} layer={} <- {} action(s) in {} entr(ies){}: {}",
        key,
        layer,
        parsed.len(),
        packet[6],
        if delay_ms > 0 {
            format!(", {delay_ms}ms between")
        } else {
            String::new()
        },
        actions.join(" then ")
    );
    println!("        packet: {} ...", hex.join(" "));
    if dry_run {
        println!("[ Ok  ] Dry run: nothing was written.");
        return Ok(());
    }

    // The read-back has to be at least as wide as the key, or the counter
    // addresses a slot on another layer entirely.
    let width = width.unwrap_or(key).max(key);
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let want = device::verify::SlotAddr {
        key_id: key,
        layer: layer + 1,
    };
    let sent = packet.clone();
    let wipe = fd::wipe_packet(key, layer);
    let readback = device::with_device(move |dev| {
        // Clear every entry first. The device updates only the first `len`
        // bytes' worth and leaves the rest, so a short sequence written over
        // a long one strands the old tail in the slot.
        device::send_report(dev, &wipe)?;
        sleep(Duration::from_millis(15));
        device::send_commit(dev)?;
        device::send_report(dev, &packet)?;
        sleep(Duration::from_millis(15));
        device::send_commit(dev)?;
        // The table is walked, not addressed. A lone query for counter 7
        // never answers, while the same query as step 7 of a walk from 1
        // answers every time -- so the counter is a position in a sequence
        // the firmware tracks, not a random-access index. Reading the whole
        // table is what `upload --verify` already does for exactly this
        // reason; a single-slot shortcut here would time out and report a
        // write that landed as UNCONFIRMED.
        Ok(device::read_slot_table(dev, width, device::DEVICE_LAYERS))
    })?;
    let readback = readback
        .into_iter()
        .find(|r| device::verify::parse_record(r).is_some_and(|(addr, _)| addr == want))
        .unwrap_or_default();

    let hex: Vec<String> = readback
        .iter()
        .take(13)
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("        read back: {} ...", hex.join(" "));
    if readback.is_empty() {
        anyhow::bail!(
            "no record for key {} layer {} came back; widen the read with --width \
             if the slot sits past this layout, otherwise the write is UNCONFIRMED",
            key,
            layer
        );
    }
    if !fd::record_matches(&sent, &readback) {
        anyhow::bail!("the device stored something other than what was sent; write UNCONFIRMED");
    }
    println!(
        "[ Ok  ] Slot {} on layer {} confirmed by read-back ({} action(s)).",
        key,
        layer,
        parsed.len()
    );
    println!("        Verify the gesture with: antiknob listen --timeout-secs 10");
    Ok(())
}
