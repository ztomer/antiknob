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
            println!(
                "        layer byte={} key_id={} kind={} mods=0x{:02x} code=0x{:02x}",
                packet[3], packet[2], packet[4], packet[11], packet[12]
            );
        }
        println!(
            "        3 slots x {} layer(s). Hold+twist slots are NOT bound",
            layers.len()
        );
        println!("        (which slot drives them is still unmeasured).");
        return Ok(());
    }
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let flashed = layers.clone();
    let sent =
        device::with_device(move |dev| host::bind::flash_slot_bindings(dev, buttons, &flashed))?;
    println!(
                "[ Ok  ] Flashed {} slot binding(s): CCW=ctrl-alt-F16, Press=ctrl-alt-F17, CW=ctrl-alt-F18.",
                sent
            );
    println!("        Hold+twist slots unchanged (which slot drives them is unmeasured;");
    println!("        arm candidates with `antiknob bind-seq` and run `probe-gestures`).");
    let recorded = host::default_config_path()
        .map_err(anyhow::Error::from)
        .and_then(|path| host::device_binding::record_bound_layer(&path, &layers));
    match recorded {
        Ok(Some(layer)) => {
            let arrangement = host::device_binding::arrangement(Some(layer), device::DEVICE_LAYERS);
            println!("[ Ok  ] {}", arrangement.describe());
        }
        Ok(None) => {
            println!("        Every layer was bound, so no single one is recorded as THE");
            println!("        host-translated layer; whichever the knob is on will work.");
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
    actions: Vec<String>,
) -> Result<()> {
    if layer >= device::DEVICE_LAYERS {
        anyhow::bail!(
            "layer {} does not exist; this firmware has layers 0..{}",
            layer,
            device::DEVICE_LAYERS - 1
        );
    }
    let parsed = fd::parse_sequence(&actions)?;
    let packet = fd::build_packet(key, layer, &parsed)?;

    let hex: Vec<String> = packet[..13].iter().map(|b| format!("{b:02x}")).collect();
    println!(
        "[ ==> ] Slot key={} layer={} <- {} action(s): {}",
        key,
        layer,
        parsed.len(),
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
    let readback = device::with_device(move |dev| {
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
