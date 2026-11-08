//! Slot-binding commands, split out of `cmds.rs` for the file-length gate.
//!
//! Both commands write firmware slots, and they are the two that have to
//! care about WHERE a slot is: `bind-slots` derives the knob's three slots
//! from the declared button count, and `bind-seq` is handed a key id
//! directly. Grouping them keeps that reasoning in one file.

use crate::cmds::layout_path;
use antiknob::firmware::{DEVICE_LAYERS, GESTURES_PER_KNOB};
use antiknob::{config, device, fd, host};
use goh_tui::{OK, START, WARN};

use anyhow::{Context, Result};
use std::path::PathBuf;

/// The declared button count, which is what places every knob slot.
pub fn button_count_for(config: Option<PathBuf>) -> Result<usize> {
    resolve_button_count(config, None)
}

fn resolve_button_count(config: Option<PathBuf>, buttons: Option<usize>) -> Result<usize> {
    if let Some(n) = buttons {
        // A flag is distrusted like any remote input: past the slot space
        // it would wrap onto someone else's slot.
        if n > antiknob::firmware::SLOT_MAX_KEY_ID as usize {
            anyhow::bail!(
                "button count {} is past the addressable slots (max {})",
                n,
                antiknob::firmware::SLOT_MAX_KEY_ID
            );
        }
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
    goh_tui::outln!(
        "{START} Layout has {} button(s); knob slots follow them.",
        buttons
    );
    let layers: Vec<u8> = match layer {
        Some(l) => vec![l],
        None => antiknob::firmware::BIND_LAYERS.to_vec(),
    };
    if dry_run {
        goh_tui::outln!("{START} Slot binding plan (dry run, no hardware touched):");
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
                        packet[antiknob::firmware::FD_HEADER_LEN
                            + i * antiknob::firmware::FD_ENTRY_LEN
                            + 2]
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
            GESTURES_PER_KNOB,
            layers.len()
        );
        return Ok(());
    }
    goh_tui::outln!("{START} Opening Anticater device via native IOHIDManager (no sudo)...");
    let flashed = layers.clone();
    // `flash_slot_bindings` reads the whole table back and fails unless
    // every slot holds what was sent. This printed "Flashed 15 slot
    // binding(s)" on `hid_write` alone, a count of packets, not of slots.
    let written =
        device::with_device(move |dev| host::bind::flash_slot_bindings(dev, buttons, &flashed))?;
    goh_tui::outln!(
        "{OK} Flashed {} slot binding(s), every one confirmed by read-back.",
        written.confirmed()
    );
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
            let arrangement = host::device_binding::arrangement(&bound, DEVICE_LAYERS);
            goh_tui::outln!("{OK} {}", arrangement.describe());
            println!("        Re-run with `--layer N` to bind just one.");
        }
        Err(e) => {
            // The flash landed; only the bookkeeping failed. Saying so beats
            // reporting the whole command as failed.
            goh_tui::outln!("{WARN} Slots flashed, but host.json could not record the layer: {e}");
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
    dry_run: bool,
    delay_ms: u16,
    actions: &[String],
) -> Result<()> {
    if layer >= DEVICE_LAYERS {
        anyhow::bail!(
            "layer {} does not exist; this firmware has layers 0..{}",
            layer,
            DEVICE_LAYERS - 1
        );
    }
    let mut parsed = fd::parse_sequence(actions)?;
    // The delay belongs to every step after the first: a wait before the
    // opening keystroke only slows the gesture down. The vendor's own
    // default is 50 ms between steps, which is what the 0x32 bytes filling
    // its records are.
    for step in parsed.iter_mut().skip(1) {
        step.delay_ms = delay_ms;
    }
    let packet = fd::build_packet(key, layer, &parsed)?;

    let hex: Vec<String> = packet[..13].iter().map(|b| format!("{b:02x}")).collect();
    goh_tui::outln!(
        "{START} Slot key={} layer={} <- {} action(s) in {} entr(ies){}: {}",
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
        goh_tui::outln!("{OK} Dry run: nothing was written.");
        return Ok(());
    }

    goh_tui::outln!("{START} Opening Anticater device via native IOHIDManager (no sudo)...");
    let want = device::verify::SlotAddr {
        key_id: key,
        layer: layer + 1,
    };
    // `write_slots` wipes the slot first (a media write updates only its
    // first `len` entries, so a short record over a long one keeps the old
    // tail), then reads the WHOLE table back: a lone per-slot query never
    // answers, and a walk at a layout width cannot see a key past it.
    let written = device::with_device(move |dev| device::write_slots(dev, &[packet]))?;
    let readback = written
        .table
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
            "no record for key {key} layer {layer} came back from the full table; the write is UNCONFIRMED"
        );
    }
    if written
        .verdicts
        .iter()
        .any(|(_, v)| *v != device::verify::SlotVerdict::Confirmed)
    {
        anyhow::bail!("the device stored something other than what was sent; write UNCONFIRMED");
    }
    goh_tui::outln!(
        "{OK} Slot {} on layer {} confirmed by read-back ({} action(s)).",
        key,
        layer,
        parsed.len()
    );
    println!("        Verify the gesture with: antiknob listen --timeout-secs 10");
    Ok(())
}
