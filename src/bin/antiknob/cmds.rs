//! `antiknob` command handlers (split from main.rs for the file gate).

use antiknob::{api, apps, config, device, host, protocol};

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

pub fn run_status(json: bool) -> Result<()> {
    let devices = device::list_devices()?;
    let connected = !devices.is_empty();
    let primary = device::primary_transport(&devices);
    let power = api::types::PowerStatus::current(&devices);
    if json {
        let out = serde_json::json!({
            "connected": connected,
            "transport": primary.map(|t| t.as_str()).unwrap_or("disconnected"),
            "power": power,
            "devices": devices,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    println!("[ ==> ] Scanning for Anticater / CH57x devices...");

    if devices.is_empty() {
        println!("[ Wrn ] No supported devices detected (USB / 2.4G / Bluetooth).");
        println!("        Please check your connection.");
        return Ok(());
    }

    for d in &devices {
        println!(
            "[ Ok  ] Found: {} [{}] (VID: 0x{:04x}, PID: 0x{:04x}, UsagePage: 0x{:04x}, Usage: 0x{:04x}, Iface: {})",
            d.name, d.transport.display_name(), d.vendor_id, d.product_id, d.usage_page, d.usage, d.interface_number
        );
        println!("        Path: {}", d.path);
        if let Some(ref sn) = d.serial_number {
            println!("        Serial Number: {}", sn);
        }
    }
    if let Some(t) = primary {
        println!("[ Ok  ] Transport: {}", t.display_name());
    }
    println!("[ Ok  ] Power: {}", power.description);

    // Test unprivileged access to the vendor configuration interface
    match device::with_device(|_| Ok(())) {
        Ok(()) => {
            println!(
                "[ Ok  ] Unprivileged access verified: Device can be configured WITHOUT sudo!"
            );
        }
        Err(e) => {
            println!("[ Wrn ] Could not open device interface: {}", e);
        }
    }
    Ok(())
}
/// The layout file to act on: the explicit argument, else the one in
/// Application Support, seeded from the packaged starter on first use.
pub fn layout_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
    config::resolve_device_config_path(&home, explicit)
}

/// How wide one device layer is, from the installed layout.
pub fn layout_slots_per_layer(explicit: Option<PathBuf>) -> Result<u8> {
    let path = layout_path(explicit)?;
    let cfg = config::DeviceConfig::load_from_file(&path)?;
    u8::try_from(cfg.slots_per_layer())
        .context("layout declares more slots per layer than the protocol can address")
}

pub fn run_validate(file: Option<PathBuf>) -> Result<()> {
    let file = layout_path(file)?;
    println!("[ ==> ] Validating configuration file '{:?}'...", file);
    let cfg = config::DeviceConfig::load_from_file(&file)?;
    cfg.validate()?;
    println!(
        "[ Ok  ] Configuration is valid ({} layers defined).",
        cfg.layers.len()
    );
    Ok(())
}
pub fn run_upload(
    file: Option<PathBuf>,
    layer: Option<u8>,
    skip_verify: bool,
    knob_only: bool,
) -> Result<()> {
    let file = layout_path(file)?;
    println!("[ ==> ] Loading and validating {}...", file.display());
    let cfg = config::DeviceConfig::load_from_file(&file)?;
    cfg.validate()?;
    if cfg.knob_slots_start_at_the_first_button() && !knob_only {
        anyhow::bail!(
            "{} declares no buttons (rows {} x columns {}), so its knob bindings \n\
             would be written to key IDs 1/2/3 -- which on a device WITH keys are \n\
             the keys, silently replacing them. Pass --knob-only if the target \n\
             device really has no keys, or set rows/columns to the real layout.",
            file.display(),
            cfg.rows,
            cfg.columns
        );
    }

    let selected: Vec<usize> = match layer {
        Some(l) => {
            let idx = l as usize;
            if idx >= cfg.layers.len() {
                anyhow::bail!(
                    "Layer {} out of range (config has {} layer(s))",
                    idx,
                    cfg.layers.len()
                );
            }
            vec![idx]
        }
        None => (0..cfg.layers.len()).collect(),
    };
    println!(
        "[ ==> ] Flashing keymaps across {} layer(s)...",
        selected.len()
    );

    // Packets are built before the device is touched: parse errors abort
    // without leaving the firmware half-programmed, and the HID job owns
    // plain bytes rather than borrowing `cfg`.
    let mut packets: Vec<Vec<u8>> = Vec::new();
    for layer_idx in selected {
        let layer = &cfg.layers[layer_idx];
        let layer_u8 = layer_idx as u8;

        // 1. Program buttons
        let mut button_count = 0;
        for row in &layer.buttons {
            for key_str in row {
                let action = protocol::Action::parse(key_str)?;
                let key_id = protocol::key_id_for_button(button_count);
                packets.push(action.to_packet(key_id, layer_u8));
                button_count += 1;
            }
        }

        // 2. Program knobs. Their slot IDs continue after the buttons, so
        // the count above is what places them -- see `key_id_for_knob`.
        for (knob_idx, knob) in layer.knobs.iter().enumerate() {
            for (event, binding) in knob.bindings() {
                let key_id = protocol::key_id_for_knob(button_count, knob_idx, event);
                // A sequence needs the 0xFD writer; a single action keeps the
                // 0xFE path. `Binding::to_packet` decides, so the CLI and the
                // daemon cannot drift on it.
                packets.push(binding.to_packet(key_id, layer_u8)?);
            }
        }

        // 3. Program layer LED if specified in config
        if let Some(ref led_mode) = layer.led {
            packets.push(protocol::build_led_packet(layer_u8, led_mode)?);
        }
    }

    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    // Clear each slot before writing it. Neither write command clears the
    // bytes it does not set, so a slot that used to hold a keyboard chord
    // keeps that chord's modifier byte under a mouse binding written over
    // it -- and the read-back then reports a mismatch that is real: the
    // device really does hold bytes the new binding never wrote. Found
    // when a corrected layout flashed two mouse gestures over old chords
    // and came back with `01 08` still sitting at bytes 10-11.
    let wipes: Vec<Vec<u8>> = packets
        .iter()
        .filter_map(|p| {
            device::verify::parse_record(p).map(|(addr, _)| {
                antiknob::fd::wipe_packet(addr.key_id, addr.layer.saturating_sub(1))
            })
        })
        .collect();
    // Keep a copy to check against: `hid_write` succeeding says the packet
    // was well-formed, not that the firmware reads the slot it addressed.
    let written = packets.clone();
    let slots_per_layer = u8::try_from(cfg.slots_per_layer())
        .context("layout declares more slots per layer than the protocol can address")?;
    let observed = device::with_device(move |dev| {
        for wipe in &wipes {
            device::send_report(dev, wipe)?;
            sleep(Duration::from_millis(10));
        }
        if !wipes.is_empty() {
            device::send_commit(dev)?;
        }
        for packet in &packets {
            device::send_report(dev, packet)?;
            sleep(Duration::from_millis(10));
        }
        device::send_commit(dev)?;
        if skip_verify {
            return Ok(Vec::new());
        }
        sleep(Duration::from_millis(100));
        Ok(device::read_slot_table(
            dev,
            slots_per_layer,
            device::DEVICE_LAYERS,
        ))
    })?;

    if skip_verify {
        println!("[ Ok  ] Packets accepted by the device (read-back skipped).");
        return Ok(());
    }
    report_flash_verdict(&written, &observed);
    Ok(())
}

/// Say what the device now holds, not what was sent to it.
///
/// The old message was "Configuration successfully written", printed on the
/// strength of `hid_write` not erroring -- which it does not do for a
/// well-formed packet addressed to a slot the firmware stores and never
/// reads. That is how knob bindings went to key IDs 16/17/18 on a device
/// that reads 4/5/6, for every flash, with a success line each time.
fn report_flash_verdict(written: &[Vec<u8>], observed: &[Vec<u8>]) {
    use device::verify::{summarize, verify, SlotVerdict};
    let verdicts = verify(written, observed);
    let confirmed = verdicts
        .iter()
        .filter(|(_, v)| *v == SlotVerdict::Confirmed)
        .count();
    match summarize(&verdicts) {
        None => println!(
            "[ Ok  ] Configuration written and read back: {}/{} slot(s) confirmed.",
            confirmed,
            verdicts.len()
        ),
        Some(problem) => {
            println!(
                "[ Wrn ] Wrote {} slot(s); only {} read back as expected.",
                verdicts.len(),
                confirmed
            );
            println!("        {}", problem);
            // Only a MISMATCH means the write went somewhere the firmware
            // does not read. A slot the dump simply never showed says
            // nothing about the write, and sending the user to check their
            // layout over it would be advice about the wrong problem.
            let wrong = verdicts.iter().any(|(_, v)| *v == SlotVerdict::Mismatched);
            if wrong {
                println!("        Check the device's button count: knob slots follow the");
                println!("        buttons, and a wrong count writes to slots nothing reads.");
            }
        }
    }
}
pub fn run_led(layer: u8, mode: Vec<String>) -> Result<()> {
    let mode_str = mode.join(" ");
    println!(
        "[ ==> ] Setting LED mode for layer {}: '{}'...",
        layer, mode_str
    );
    let packet = protocol::build_led_packet(layer, &mode_str)?;
    device::with_device(move |dev| device::send_led(dev, &packet))?;
    println!("[ Ok  ] LED configuration sent to device.");
    Ok(())
}
pub fn run_led_read(layer: u8, raw: bool) -> Result<()> {
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let (mode, dump) = device::with_device(move |dev| {
        let mode = device::read_led_mode(dev, layer);
        let dump = if raw {
            let mut payload = [0u8; 64];
            payload[0] = 0xFA;
            payload[1] = 0xB0;
            payload[2] = layer;
            let mut buf = [0u8; 64];
            match device::send_report(dev, &payload)
                .and_then(|()| dev.read_timeout(&mut buf, 500).map_err(anyhow::Error::from))
            {
                Ok(n) => Some(buf[..n].to_vec()),
                Err(_) => None,
            }
        } else {
            None
        };
        Ok((mode, dump))
    })?;

    match mode {
        Ok(mode) => {
            // One definition. This used to carry its own copy of the mode
            // table -- the 1189:884x names -- so `led-read` kept reporting
            // "press" for mode 4 after the shared table was corrected to
            // "rainbow" for this device.
            let label = api::led_mode_name(mode);
            println!("[ Ok  ] Layer {} LED mode: {} ({})", layer, mode, label);
        }
        Err(e) => println!("[ Wrn ] LED read failed: {}", e),
    }
    if let Some(bytes) = dump {
        let hex: Vec<String> = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        println!("        raw ({}B): {}", bytes.len(), hex.join(" "));
    }
    Ok(())
}
pub fn run_show_keys() -> Result<()> {
    println!("Modifiers:");
    println!("  ctrl, shift, alt / opt, cmd / win, rctrl, rshift, ralt / ropt, rcmd / rwin");
    println!();
    println!("Keys:");
    println!("  a-z, 1-0, enter, esc, backspace, tab, space, minus, equal");
    println!("  leftbracket, rightbracket, backslash, semicolon, quote, grave");
    println!("  comma, period, slash, capslock, f1-f12, f13-f24, printscreen, scrolllock");
    println!("  pause, insert, home, pageup, delete, end, pagedown, up, down, left, right");
    println!();
    println!("Media Keys:");
    println!("  volumeup, volumedown, mute, play, prev, next, stop, brightnessup, brightnessdown");
    println!();
    println!("Mouse Actions:");
    println!("  click, rclick, mclick, wheelup, wheeldown");
    Ok(())
}
/// How many buttons the target device has, from the layout rather than a
/// constant. `--buttons` wins when given; otherwise the config file is the
/// source of truth, and its absence is an error rather than a fallback --
/// a guessed count is how knob bindings landed in slots nothing reads.
pub fn run_import_presets(out: Option<PathBuf>, force: bool) -> Result<()> {
    let (cfg, leds, warnings) = host::migrate::migrate_all_presets();
    println!(
        "[ ==> ] Migrated {} preset(s) to host layers.",
        cfg.layers.len()
    );
    println!("        Note: {}", host::migrate::REINTERPRET_NOTE);
    for w in &warnings {
        println!("[ Wrn ] {}", w);
    }
    println!("[ ==> ] Companion device LED specs (apply with: antiknob led <layer> ...):");
    for (i, (name, spec)) in leds.iter().enumerate() {
        println!("        layer {} ({}): {}", i, name, spec);
    }
    let json = cfg.to_json_pretty();
    match out {
        Some(path) => {
            if path.exists() && !force {
                println!(
                    "[ Err ] {} exists; refusing to overwrite without --force.",
                    path.display()
                );
                return Ok(());
            }
            std::fs::write(&path, &json)?;
            println!("[ Ok  ] Wrote host config to {}.", path.display());
        }
        None => {
            println!("--- host.json ---");
            println!("{}", json);
        }
    }
    Ok(())
}
pub fn run_list_apps() -> Result<()> {
    let dirs = apps::default_app_dirs();
    println!("[ ==> ] Scanning {:?} ...", dirs);
    let found = apps::scan_app_dirs(&dirs);
    if found.is_empty() {
        println!("[ Wrn ] No .app bundles found.");
        return Ok(());
    }
    for app in &found {
        match &app.bundle_id {
            Some(id) => println!("        {:<40} {}", app.name, id),
            None => println!("        {:<40} (no bundle id)", app.name),
        }
    }
    println!(
        "[ Ok  ] {} app(s). Use bundle IDs in launchApp/quitApp actions.",
        found.len()
    );
    Ok(())
}
