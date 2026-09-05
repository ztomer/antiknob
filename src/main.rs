mod config;
mod device;
mod protocol;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "antiknob",
    version = "0.1.0",
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
                    "[ Ok  ] Found: {} (VID: 0x{:04x}, PID: 0x{:04x}, UsagePage: 0x{:04x})",
                    d.name, d.vendor_id, d.product_id, d.usage_page
                );
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
            println!("[ Ok  ] Configuration is valid ({} layers defined).", cfg.layers.len());
        }

        Commands::Upload { file } => {
            println!("[ ==> ] Loading and validating '{:?}'...", file);
            let cfg = config::DeviceConfig::load_from_file(&file)?;
            cfg.validate()?;

            println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
            let dev = device::open_device()?;

            println!("[ ==> ] Flashing keymaps across {} layer(s)...", cfg.layers.len());

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

            println!("[ Ok  ] Configuration successfully written to Anticater VK01!");
        }

        Commands::Led { layer, mode } => {
            let mode_str = mode.join(" ");
            println!("[ ==> ] Setting LED mode for layer {}: '{}'...", layer, mode_str);
            let packet = protocol::build_led_packet(layer, &mode_str)?;
            let dev = device::open_device()?;
            device::send_report(&dev, &packet)?;
            println!("[ Ok  ] LED configuration sent to device.");
        }

        Commands::ShowKeys => {
            println!("Modifiers:");
            println!("  ctrl, shift, alt / opt, cmd / win, rctrl, rshift, ralt / ropt, rcmd / rwin");
            println!();
            println!("Keys:");
            println!("  a-z, 1-0, enter, esc, backspace, tab, space, minus, equal");
            println!("  leftbracket, rightbracket, backslash, semicolon, quote, grave");
            println!("  comma, period, slash, capslock, f1-f12, printscreen, scrolllock");
            println!("  pause, insert, home, pageup, delete, end, pagedown, up, down, left, right");
            println!();
            println!("Media Keys:");
            println!("  volumeup, volumedown, mute, play, prev, next, stop, brightnessup, brightnessdown");
            println!();
            println!("Mouse Actions:");
            println!("  click, rclick, mclick, wheelup, wheeldown");
        }
    }

    Ok(())
}
