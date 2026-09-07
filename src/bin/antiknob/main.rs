use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cmds;

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
        /// Scan all groups 0x00-0xFF instead of just the vendor-observed ones
        #[arg(long)]
        wide: bool,
    },

    /// Send a raw payload (hex bytes, report 0x03 prepended) for RE work
    Raw {
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
        Commands::Upload { file, layer } => cmds::run_upload(file, layer)?,
        Commands::Led { layer, mode } => cmds::run_led(layer, mode)?,
        Commands::LedRead { layer, raw } => cmds::run_led_read(layer, raw)?,
        Commands::ShowKeys => cmds::run_show_keys()?,
        Commands::BindSlots { layer, dry_run } => cmds::run_bind_slots(layer, dry_run)?,
        Commands::Listen { timeout_secs } => cmds::run_listen(timeout_secs)?,
        Commands::ImportPresets { out, force } => cmds::run_import_presets(out, force)?,
        Commands::ListApps => cmds::run_list_apps()?,
        Commands::ReadSlots { wide } => cmds::run_read_slots(wide)?,
        Commands::Raw { bytes } => cmds::run_raw(bytes)?,
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
