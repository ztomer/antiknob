//! `antiknob` command handlers (split from main.rs for the file gate).

use antiknob::{api, apps, config, device, host, protocol};

use anyhow::Result;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

/// Best-effort decode of one input report for the snoop log. Keyboard
/// boot reports show modifiers + keycodes, mouse shows buttons/wheel,
/// everything else prints raw. Never fails: unknown layouts fall back
/// to an empty note next to the hex dump.
fn decode_input(iface: &device::SnoopIface, buf: &[u8]) -> String {
    // Keyboard boot report: [mods, 00, k1..k6], optionally prefixed
    // with a zero report ID.
    let body: &[u8] = if buf.len() == 9 && buf[0] == 0 {
        &buf[1..]
    } else {
        buf
    };
    if iface.usage_page == 0x01 && iface.usage == 0x06 && body.len() == 8 && body[1] == 0 {
        let mods = body[0];
        let keys: Vec<String> = body[2..8]
            .iter()
            .filter(|&&k| k != 0)
            .map(|k| format!("{:02x}", k))
            .collect();
        let mut mod_names = Vec::new();
        if mods & 0x01 != 0 {
            mod_names.push("ctrl");
        }
        if mods & 0x02 != 0 {
            mod_names.push("shift");
        }
        if mods & 0x04 != 0 {
            mod_names.push("alt");
        }
        if mods & 0x08 != 0 {
            mod_names.push("cmd");
        }
        return format!(
            "<-- keyboard mods=[{}] keys=[{}]",
            mod_names.join("+"),
            keys.join(" ")
        );
    }
    if iface.usage_page == 0x01 && iface.usage == 0x02 && buf.len() >= 3 {
        let buttons = buf[0];
        let mut notes = Vec::new();
        if buttons & 0x01 != 0 {
            notes.push("left".to_string());
        }
        if buttons & 0x02 != 0 {
            notes.push("right".to_string());
        }
        if buttons & 0x04 != 0 {
            notes.push("middle".to_string());
        }
        return format!(
            "<-- mouse buttons=[{}] x={} wheel={}",
            notes.join("+"),
            buf[1] as i8,
            buf[2] as i8
        );
    }
    if iface.usage_page == 0xFF00 && buf.len() > 2 && buf[0] == 0x03 && (16..=27).contains(&buf[2])
    {
        return format!("<-- vendor slot key_id={}", buf[2]);
    }
    String::new()
}

/// Parse hex byte strings ("FC", "0xfc") into a payload. Pure for testing.
fn parse_hex_bytes(parts: &[String]) -> Result<Vec<u8>> {
    parts
        .iter()
        .map(|s| {
            let s = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            u8::from_str_radix(s, 16).map_err(|_| anyhow::anyhow!("Bad hex byte '{}'", s))
        })
        .collect()
}

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
    match device::open_device() {
        Ok(_) => {
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
pub fn run_validate(file: PathBuf) -> Result<()> {
    println!("[ ==> ] Validating configuration file '{:?}'...", file);
    let cfg = config::DeviceConfig::load_from_file(&file)?;
    cfg.validate()?;
    println!(
        "[ Ok  ] Configuration is valid ({} layers defined).",
        cfg.layers.len()
    );
    Ok(())
}
pub fn run_upload(file: PathBuf, layer: Option<u8>) -> Result<()> {
    println!("[ ==> ] Loading and validating '{:?}'...", file);
    let cfg = config::DeviceConfig::load_from_file(&file)?;
    cfg.validate()?;

    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let dev = device::open_device()?;

    let layers: Vec<(usize, &config::LayerConfig)> = match layer {
        Some(l) => {
            let idx = l as usize;
            if idx >= cfg.layers.len() {
                anyhow::bail!(
                    "Layer {} out of range (config has {} layer(s))",
                    idx,
                    cfg.layers.len()
                );
            }
            vec![(idx, &cfg.layers[idx])]
        }
        None => cfg.layers.iter().enumerate().collect(),
    };
    println!(
        "[ ==> ] Flashing keymaps across {} layer(s)...",
        layers.len()
    );

    for (layer_idx, layer) in layers {
        let layer_u8 = layer_idx as u8;

        // 1. Program buttons
        let mut button_count = 0;
        for row in &layer.buttons {
            for key_str in row {
                let action = protocol::Action::parse(key_str)?;
                let key_id = protocol::key_id_for_button(button_count);
                let packet = action.to_packet(key_id, layer_u8);
                device::send_report(&dev, &packet)?;
                sleep(Duration::from_millis(10));
                button_count += 1;
            }
        }

        // 2. Program knobs
        for (knob_idx, knob) in layer.knobs.iter().enumerate() {
            if let Some(ref ccw_str) = knob.ccw {
                let action = protocol::Action::parse(ccw_str)?;
                let key_id = protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::RotateCCW);
                let packet = action.to_packet(key_id, layer_u8);
                device::send_report(&dev, &packet)?;
                sleep(Duration::from_millis(10));
            }
            if let Some(ref press_str) = knob.press {
                let action = protocol::Action::parse(press_str)?;
                let key_id = protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::Press);
                let packet = action.to_packet(key_id, layer_u8);
                device::send_report(&dev, &packet)?;
                sleep(Duration::from_millis(10));
            }
            if let Some(ref cw_str) = knob.cw {
                let action = protocol::Action::parse(cw_str)?;
                let key_id = protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::RotateCW);
                let packet = action.to_packet(key_id, layer_u8);
                device::send_report(&dev, &packet)?;
                sleep(Duration::from_millis(10));
            }
        }

        // 3. Program layer LED if specified in config
        if let Some(ref led_mode) = layer.led {
            let packet = protocol::build_led_packet(layer_u8, led_mode)?;
            device::send_report(&dev, &packet)?;
            sleep(Duration::from_millis(10));
        }
    }

    device::send_commit(&dev)?;
    println!("[ Ok  ] Configuration successfully written to Anticater VK01!");
    Ok(())
}
pub fn run_led(layer: u8, mode: Vec<String>) -> Result<()> {
    let mode_str = mode.join(" ");
    println!(
        "[ ==> ] Setting LED mode for layer {}: '{}'...",
        layer, mode_str
    );
    let packet = protocol::build_led_packet(layer, &mode_str)?;
    let dev = device::open_device()?;
    device::send_report(&dev, &packet)?;
    device::send_commit(&dev)?;
    println!("[ Ok  ] LED configuration sent to device.");
    Ok(())
}
pub fn run_led_read(layer: u8, raw: bool) -> Result<()> {
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let dev = device::open_device()?;
    match device::read_led_mode(&dev, layer) {
        Ok(mode) => {
            let label = match mode {
                0 => "off",
                1 => "backlight",
                2 => "shock",
                3 => "shock2",
                4 => "press",
                5 => "custom",
                _ => "unknown",
            };
            println!("[ Ok  ] Layer {} LED mode: {} ({})", layer, mode, label);
        }
        Err(e) => println!("[ Wrn ] LED read failed: {}", e),
    }
    if raw {
        let mut payload = [0u8; 64];
        payload[0] = 0xFA;
        payload[1] = 0xB0;
        payload[2] = layer;
        if device::send_report(&dev, &payload).is_ok() {
            let mut buf = [0u8; 64];
            if let Ok(n) = dev.read_timeout(&mut buf, 500) {
                let hex: Vec<String> = buf[..n].iter().map(|b| format!("{:02x}", b)).collect();
                println!("        raw ({}B): {}", n, hex.join(" "));
            }
        }
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
pub fn run_bind_slots(layer: Option<u8>, dry_run: bool) -> Result<()> {
    let layers: Vec<u8> = match layer {
        Some(l) => vec![l],
        None => host::bind::BIND_LAYERS.to_vec(),
    };
    if dry_run {
        println!("[ ==> ] Slot binding plan (dry run, no hardware touched):");
        for packet in host::bind::binding_packets(&layers)? {
            println!(
                "        layer byte={} key_id={} kind={} mods=0x{:02x} code=0x{:02x}",
                packet[3], packet[2], packet[4], packet[11], packet[12]
            );
        }
        println!(
            "        3 slots x {} layer(s). Hold+twist slots are NOT bound",
            layers.len()
        );
        println!("        (key IDs unverified; use the vendor app for those).");
        return Ok(());
    }
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let dev = device::open_device()?;
    let sent = host::bind::flash_slot_bindings(&dev, &layers)?;
    println!(
                "[ Ok  ] Flashed {} slot binding(s): CCW=ctrl-alt-F16, Press=ctrl-alt-F17, CW=ctrl-alt-F18.",
                sent
            );
    println!("        Hold+twist slots unchanged (key IDs unverified; use the vendor app).");
    println!("        Verify with: antiknob listen --timeout-secs 10");
    Ok(())
}
pub fn run_listen(timeout_secs: u64) -> Result<()> {
    use std::time::Instant;
    println!("[ ==> ] Opening all knob interfaces for snooping (non-exclusive, no sudo)...");
    let ifaces = device::open_all_interfaces()?;
    if ifaces.is_empty() {
        println!("[ Wrn ] No supported devices detected on USB.");
        return Ok(());
    }
    for iface in &ifaces {
        println!("        watching {}", iface.label);
    }
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    println!(
        "[ ==> ] Snooping for {}s: twist / press / hold the knob (mouse stays usable)...",
        timeout_secs
    );
    let mut buf = [0u8; 64];
    while Instant::now() < deadline {
        for iface in &ifaces {
            match iface.device.read_timeout(&mut buf, 20) {
                Ok(0) => {}
                Ok(n) => {
                    if buf[..n].iter().any(|&b| b != 0) {
                        let hex: Vec<String> =
                            buf[..n].iter().map(|b| format!("{:02x}", b)).collect();
                        print!("        [{}] +{}B: {}", iface.label, n, hex.join(" "));
                        print!("   {}", decode_input(iface, &buf[..n]));
                        println!();
                    }
                }
                Err(e) => {
                    println!("[ Wrn ] Read error on {}: {}", iface.label, e);
                }
            }
        }
    }
    println!("[ Ok  ] Listen window closed.");
    Ok(())
}
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
pub fn run_read_slots(wide: bool) -> Result<()> {
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let dev = device::open_device()?;
    let groups: Vec<u8> = if wide {
        (0x00u8..=0xFFu8).collect()
    } else {
        vec![0x0F, 0x19]
    };
    println!(
        "[ ==> ] Reading slot table ({} groups, counters 1-3)...",
        groups.len()
    );
    for group in groups {
        for counter in 1u8..=3 {
            match device::read_slot(&dev, group, counter) {
                Ok(bytes) => {
                    let hex: Vec<String> = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                    println!(
                        "        group=0x{:02x} counter={} ({}B): {}",
                        group,
                        counter,
                        bytes.len(),
                        hex.join(" ")
                    );
                }
                Err(e) => {
                    println!(
                        "        group=0x{:02x} counter={}: READ FAILED: {}",
                        group, counter, e
                    );
                }
            }
            sleep(Duration::from_millis(50));
        }
    }
    println!("[ Ok  ] Slot dump complete (device state unchanged).");
    Ok(())
}
pub fn run_raw(bytes: Vec<String>) -> Result<()> {
    let payload = parse_hex_bytes(&bytes)?;
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    let dev = device::open_device()?;
    device::send_report(&dev, &payload)?;
    println!("[ Ok  ] Raw {}-byte payload sent.", payload.len());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_bytes_parse_with_and_without_prefix() {
        assert_eq!(
            parse_hex_bytes(&["FC".to_string(), "0xfc".to_string(), "00".to_string()]).unwrap(),
            vec![0xFC, 0xFC, 0x00]
        );
        assert!(parse_hex_bytes(&["zz".to_string()]).is_err());
        assert!(parse_hex_bytes(&["123".to_string()]).is_err());
    }
}
