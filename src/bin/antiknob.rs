use antiknob::{apps, config, device, host, protocol};

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "antiknob",
    version,
    about = "Native macOS Apple Silicon configurator for Anticater VK01 Knob (MIT OR Apache-2.0)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Probe and display status of connected Anticater / CH57x hardware without sudo
    Status {
        /// Machine-readable JSON device list (for the GUI subprocess bridge)
        #[arg(long)]
        json: bool,
    },

    /// Validate configuration YAML file syntax offline
    Validate {
        #[arg(default_value = "config.yaml")]
        file: PathBuf,
    },

    /// Flash keymaps and knob configurations to device over USB without sudo
    Upload {
        #[arg(default_value = "config.yaml")]
        file: PathBuf,
        /// Flash only this device layer (default: all layers)
        #[arg(long)]
        layer: Option<u8>,
    },

    /// Set LED lighting mode (e.g., led 0 backlight white, led 0 shock blue, led 0 off)
    Led {
        layer: u8,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        mode: Vec<String>,
    },

    /// Show all supported key names, media keys, and modifiers
    ShowKeys,

    /// Flash one-time host-translate slot bindings (ctrl-alt-F16..F18) to firmware
    BindSlots {
        /// Device layer to bind (default: all layers 0-2)
        #[arg(long)]
        layer: Option<u8>,
        /// Print the packet plan without touching hardware
        #[arg(long)]
        dry_run: bool,
    },

    /// Dump raw input reports for a few seconds (verify what the knob sends)
    Listen {
        /// How long to listen, in seconds
        #[arg(long, default_value = "10")]
        timeout_secs: u64,
    },

    /// Migrate the six built-in presets to host-layer JSON for the daemon
    ImportPresets {
        /// Write host.json here (default: print to stdout)
        #[arg(long)]
        out: Option<PathBuf>,
        /// Overwrite an existing output file
        #[arg(long)]
        force: bool,
    },

    /// List installed apps (names + bundle IDs) for launch/quit actions
    ListApps,

    /// Dump the device slot table (read-only diagnostic for reverse engineering)
    ReadSlots {
        /// Scan all groups 0x00-0x2F instead of just the vendor-observed ones
        #[arg(long)]
        wide: bool,
    },
}

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Status { json } => {
            let devices = device::list_devices()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&devices)?);
                return Ok(());
            }
            println!("[ ==> ] Scanning for Anticater / CH57x USB devices...");

            if devices.is_empty() {
                println!("[ Wrn ] No supported devices detected on USB.");
                println!("        Please check your USB cable connection.");
                return Ok(());
            }

            for d in &devices {
                println!(
                    "[ Ok  ] Found: {} (VID: 0x{:04x}, PID: 0x{:04x}, UsagePage: 0x{:04x}, Usage: 0x{:04x})",
                    d.name, d.vendor_id, d.product_id, d.usage_page, d.usage
                );
                println!("        Path: {}", d.path);
                if let Some(ref sn) = d.serial_number {
                    println!("        Serial Number: {}", sn);
                }
            }

            // Test unprivileged access to the vendor configuration interface
            match device::open_device() {
                Ok(_) => {
                    println!("[ Ok  ] Unprivileged access verified: Device can be configured WITHOUT sudo!");
                }
                Err(e) => {
                    println!("[ Wrn ] Could not open device interface: {}", e);
                }
            }
        }

        Commands::Validate { file } => {
            println!("[ ==> ] Validating configuration file '{:?}'...", file);
            let cfg = config::DeviceConfig::load_from_file(&file)?;
            cfg.validate()?;
            println!(
                "[ Ok  ] Configuration is valid ({} layers defined).",
                cfg.layers.len()
            );
        }

        Commands::Upload { file, layer } => {
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
                        let key_id =
                            protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::RotateCCW);
                        let packet = action.to_packet(key_id, layer_u8);
                        device::send_report(&dev, &packet)?;
                        sleep(Duration::from_millis(10));
                    }
                    if let Some(ref press_str) = knob.press {
                        let action = protocol::Action::parse(press_str)?;
                        let key_id =
                            protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::Press);
                        let packet = action.to_packet(key_id, layer_u8);
                        device::send_report(&dev, &packet)?;
                        sleep(Duration::from_millis(10));
                    }
                    if let Some(ref cw_str) = knob.cw {
                        let action = protocol::Action::parse(cw_str)?;
                        let key_id =
                            protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::RotateCW);
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
        }

        Commands::Led { layer, mode } => {
            let mode_str = mode.join(" ");
            println!(
                "[ ==> ] Setting LED mode for layer {}: '{}'...",
                layer, mode_str
            );
            let packet = protocol::build_led_packet(layer, &mode_str)?;
            let dev = device::open_device()?;
            device::send_report(&dev, &packet)?;
            println!("[ Ok  ] LED configuration sent to device.");
        }

        Commands::ShowKeys => {
            println!("Modifiers:");
            println!(
                "  ctrl, shift, alt / opt, cmd / win, rctrl, rshift, ralt / ropt, rcmd / rwin"
            );
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
        }

        Commands::BindSlots { layer, dry_run } => {
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
            println!(
                "        Hold+twist slots unchanged (key IDs unverified; use the vendor app)."
            );
            println!("        Verify with: antiknob listen --timeout-secs 10");
        }

        Commands::Listen { timeout_secs } => {
            use std::time::Instant;
            println!(
                "[ ==> ] Opening all knob interfaces for snooping (non-exclusive, no sudo)..."
            );
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
        }

        Commands::ImportPresets { out, force } => {
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
        }

        Commands::ListApps => {
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
        }

        Commands::ReadSlots { wide } => {
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
                            let hex: Vec<String> =
                                bytes.iter().map(|b| format!("{:02x}", b)).collect();
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
        }
    }

    Ok(())
}
