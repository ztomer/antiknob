use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod binding;
mod cmds;
mod diag;
mod probe;

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
        /// Layout file. Defaults to the one in Application Support,
        /// seeded from the packaged starter on first use.
        #[arg()]
        file: Option<PathBuf>,
    },

    /// Flash keymaps and knob configurations to device over USB without sudo
    Upload {
        /// Layout file. Defaults to the one in Application Support,
        /// seeded from the packaged starter on first use.
        #[arg()]
        file: Option<PathBuf>,
        /// Flash only this device layer (default: all layers)
        #[arg(long)]
        layer: Option<u8>,
        /// Skip the post-flash read-back. Faster, and reports only that the
        /// device accepted the packets -- which is not the same as the
        /// bindings being live.
        #[arg(long)]
        no_verify: bool,
        /// Confirm the target device really has no keys. Required for a
        /// config declaring zero buttons, because knob slots then start at
        /// key ID 1 -- which on a device WITH keys are the keys.
        #[arg(long)]
        knob_only: bool,
    },

    /// Set LED lighting mode (e.g., led 0 backlight white, led 0 shock blue, led 0 off)
    Led {
        layer: u8,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        mode: Vec<String>,
    },

    /// Read back a layer's current LED mode (read-only diagnostic)
    LedRead {
        layer: u8,
        /// Dump the full raw reply bytes
        #[arg(long)]
        raw: bool,
    },

    /// Show all supported key names, media keys, and modifiers
    ShowKeys,

    /// Flash one-time host-translate slot bindings (ctrl-alt-F16..F18) to firmware
    BindSlots {
        /// Hardware layout to read the button count from. Knob slot IDs
        /// follow the buttons, so this decides where the bindings land.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Override the button count instead of reading it from a config.
        /// For a device with no config file to hand.
        #[arg(long)]
        buttons: Option<usize>,
        /// Device layer to bind (default: all layers 0-2)
        #[arg(long)]
        layer: Option<u8>,
        /// Print the packet plan without touching hardware
        #[arg(long)]
        dry_run: bool,
    },

    /// Bind one slot to a SEQUENCE of actions using the vendor's 0xFD command
    BindSeq {
        /// Slot key ID to bind. Buttons come first (1..n), then three per
        /// knob; `read-slots` shows what each one currently holds.
        #[arg(long)]
        key: u8,
        /// Device layer to bind (0-2)
        #[arg(long, default_value_t = 0)]
        layer: u8,
        /// Width to read the table back at. Must be at least --key, or the
        /// read-back addresses a different slot. Defaults to the key id.
        #[arg(long)]
        width: Option<u8>,
        /// Print the packet without touching hardware
        #[arg(long)]
        dry_run: bool,
        /// Actions to run in order, e.g. `cmd-c cmd-v`. All must be the same
        /// kind: one slot record holds one kind.
        actions: Vec<String>,
    },

    /// Dump raw input reports for a few seconds (verify what the knob sends)
    Listen {
        /// Only watch these devices (repeatable), e.g. --device 514c:8850.
        /// Default watches every supported device, which mixes the knob's
        /// reports in with any other Anticater hardware on the same host.
        #[arg(long = "device", value_name = "VID:PID")]
        devices: Vec<String>,

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
        /// Layout whose width to read the table at. Defaults to the
        /// installed one; the device needs telling how wide a layer is.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Exploratory sweep across groups 0x00-0x40 for protocol work,
        /// instead of reading the table at its real width.
        #[arg(long)]
        wide: bool,
    },

    /// Determine which firmware slot a gesture drives, by writing a
    /// distinct marker to each candidate and watching what comes out
    ProbeGestures {
        /// Slots to probe. Defaults to the two just past the three bound
        /// knob gestures, which read as present-and-empty on this hardware.
        #[arg(long, value_delimiter = ',', default_value = "7,8")]
        candidates: Vec<u8>,
        /// Device layer to probe on
        #[arg(long, default_value = "0")]
        layer: u8,
        /// Seconds to capture gestures for
        #[arg(long, default_value = "45")]
        capture_secs: u64,
        /// Only watch these devices, e.g. 514c:8850
        #[arg(long = "device", value_name = "VID:PID")]
        devices: Vec<String>,
        /// Layout to read the layer width from
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Walk every LED mode on one layer so an unmapped one can be identified
    /// by eye (which mode, if any, is a breathe)
    LedProbe {
        /// Device layer to cycle. Its LED is restored afterwards.
        #[arg(default_value = "0")]
        layer: u8,
        /// Seconds to hold each mode
        #[arg(long, default_value = "3")]
        dwell_secs: u64,
        /// Colour to use for the modes that take one
        #[arg(long, default_value = "red")]
        color: String,
    },

    /// Send a raw payload (hex bytes, report 0x03 prepended) for RE work
    Raw {
        /// Print the device's reply. Safe for queries (0xFA); a write
        /// (0xFE) has no reply and will simply time out.
        #[arg(long)]
        read: bool,
        /// Hex bytes, e.g. FC FC 02 00
        #[arg(trailing_var_arg = true)]
        bytes: Vec<String>,
    },

    /// Run as an MCP (Model Context Protocol) server over stdio
    Mcp {
        /// Host config JSON path (created with defaults if missing)
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status { json } => cmds::run_status(json)?,
        Commands::Validate { file } => cmds::run_validate(file)?,
        Commands::Upload {
            file,
            layer,
            no_verify,
            knob_only,
        } => cmds::run_upload(file, layer, no_verify, knob_only)?,
        Commands::Led { layer, mode } => cmds::run_led(layer, mode)?,
        Commands::LedRead { layer, raw } => cmds::run_led_read(layer, raw)?,
        Commands::ShowKeys => cmds::run_show_keys()?,
        Commands::BindSlots {
            config,
            buttons,
            layer,
            dry_run,
        } => binding::run_bind_slots(config, buttons, layer, dry_run)?,
        Commands::BindSeq {
            key,
            layer,
            width,
            dry_run,
            actions,
        } => binding::run_bind_seq(key, layer, width, dry_run, actions)?,
        Commands::Listen {
            timeout_secs,
            devices,
        } => diag::run_listen(timeout_secs, devices)?,
        Commands::ImportPresets { out, force } => cmds::run_import_presets(out, force)?,
        Commands::ListApps => cmds::run_list_apps()?,
        Commands::ReadSlots { config, wide } => {
            diag::run_read_slots(cmds::layout_slots_per_layer(config)?, wide)?
        }
        Commands::ProbeGestures {
            candidates,
            layer,
            capture_secs,
            devices,
            config,
        } => {
            // Widen by the two candidates so the read can address them:
            // the device only walks a table as wide as it is told.
            let width = cmds::layout_slots_per_layer(config)?
                .max(candidates.iter().copied().max().unwrap_or(0));
            probe::run(candidates, layer, width, capture_secs, devices)?
        }
        Commands::LedProbe {
            layer,
            dwell_secs,
            color,
        } => diag::run_led_probe(layer, dwell_secs, &color)?,
        Commands::Raw { read, bytes } => diag::run_raw(bytes, read)?,
        Commands::Mcp { config } => {
            let config_path = match config {
                Some(p) => p,
                None => antiknob::host::default_config_path()?,
            };
            let ctx = antiknob::api::ApiContext::new(config_path, None);
            antiknob::api::run_mcp_server(ctx)?;
        }
    }

    Ok(())
}
