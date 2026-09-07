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
    Status,

    /// Validate configuration YAML file syntax offline
    Validate {
        #[arg(default_value = "config.yaml")]
        file: PathBuf,
    },

    /// Flash keymaps and knob configurations to device over USB without sudo
    Upload {
        #[arg(default_value = "config.yaml")]
        file: PathBuf,
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            println!("[ ==> ] Scanning for Anticater / CH57x USB devices...");
            let devices = device::list_devices()?;

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

        Commands::Upload { file } => {
            println!("[ ==> ] Loading and validating '{:?}'...", file);
            let cfg = config::DeviceConfig::load_from_file(&file)?;
            cfg.validate()?;

            println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
            let dev = device::open_device()?;

            println!(
                "[ ==> ] Flashing keymaps across {} layer(s)...",
                cfg.layers.len()
            );

            for (layer_idx, layer) in cfg.layers.iter().enumerate() {
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
            println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
            let dev = device::open_device()?;
            dev.set_blocking_mode(false)?;
            let deadline = Instant::now() + Duration::from_secs(timeout_secs);
            println!(
                "[ ==> ] Listening for input reports for {}s (twist / press the knob)...",
                timeout_secs
            );
            let mut buf = [0u8; 64];
            while Instant::now() < deadline {
                match dev.read_timeout(&mut buf, 200) {
                    Ok(0) => {}
                    Ok(n) => {
                        if buf[..n].iter().any(|&b| b != 0) {
                            let hex: Vec<String> =
                                buf[..n].iter().map(|b| format!("{:02x}", b)).collect();
                            print!("        +{}B: {}", n, hex.join(" "));
                            // Best-effort hint only: the input layout mirrors
                            // the output layout on this firmware family.
                            if n > 2 && buf[0] == 0x03 && (16..=18).contains(&buf[2]) {
                                print!("   <-- possible slot key_id={}", buf[2]);
                            }
                            println!();
                        }
                    }
                    Err(e) => {
                        println!("[ Wrn ] Read error: {}", e);
                        break;
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
    }

    Ok(())
}
