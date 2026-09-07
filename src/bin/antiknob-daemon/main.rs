//! antiknob-daemon: host-side translation daemon (Phase 3, slice 2).
//!
//! Listens for the knob's bound slot chords (ctrl+alt+F16..F20) on a global
//! event tap and dispatches them through the pure `host` engine.
//!
//! Modes:
//! - Observe (default): nothing swallowed, nothing synthesized; resolved
//!   actions log as `WOULD ...` lines.
//! - Active (`--active`): slot chords and recorded layer hotkeys are
//!   swallowed and their actions synthesized (keys, scroll, brightness).
//!   Aux media, launch/open/quit plan correctly but synthesize in slice 2b:
//!   they are swallowed and SKIP-logged so no cryptic chord ever leaks.
//!
//! Needs Accessibility (and Input Monitoring) grants to install the tap;
//! without them it exits 2 with an honest message. GUI and CLI stay
//! completely TCC-free: all permission needs live in this binary alone.

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

mod keytap;
mod permissions;
mod run;
mod synth;
mod tray;
use run::{default_config_path, load_or_default, run_active, run_observe};

#[derive(Parser)]
#[command(
    name = "antiknob-daemon",
    version,
    about = "Host-side translator for Anticater VK01 knob slot chords"
)]
struct Cli {
    /// Host config JSON path (created with defaults if missing)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Stop after N seconds (0 = run forever)
    #[arg(long, default_value = "0")]
    timeout_secs: u64,

    /// Swallow slot chords and synthesize their actions (default: observe only)
    #[arg(long)]
    active: bool,

    /// Log every key event (code + modifiers), not just slot chords.
    /// Diagnoses silent taps: type or twist anything and watch.
    #[arg(long)]
    verbose: bool,

    /// Run without the menu-bar icon (headless use)
    #[arg(long)]
    no_tray: bool,

    /// Run as an MCP (Model Context Protocol) server over stdio
    #[arg(long)]
    mcp: bool,

    /// Custom path for the Unix domain socket
    #[arg(long)]
    socket_path: Option<PathBuf>,

    /// Disable Unix domain socket listener
    #[arg(long)]
    no_socket: bool,

    /// Install the per-user LaunchAgent (start at login) and load it now
    #[arg(long)]
    install_login_item: bool,

    /// Unload and remove the per-user LaunchAgent
    #[arg(long)]
    uninstall_login_item: bool,
}

fn user_domain() -> Result<String> {
    let out = std::process::Command::new("id").arg("-u").output()?;
    if !out.status.success() {
        anyhow::bail!("could not determine uid via `id -u`");
    }
    Ok(format!(
        "gui/{}",
        String::from_utf8_lossy(&out.stdout).trim()
    ))
}

fn home_dir() -> Result<PathBuf> {
    std::env::var("HOME")
        .context("HOME is not set")
        .map(PathBuf::from)
}

/// True while launchd still knows about the label.
fn agent_is_loaded(target: &str) -> bool {
    std::process::Command::new("launchctl")
        .args(["print", target])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Unload the agent and wait for launchd to actually let go.
///
/// `bootout` returns before the job is gone, and a `bootstrap` issued into
/// that gap fails with a bare "5: Input/output error" -- which reads like a
/// broken plist and is really just impatience. Poll instead of sleeping a
/// guessed interval, and give up loudly rather than bootstrapping into a
/// domain that still holds the label.
fn bootout_and_wait(target: &str) {
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", target])
        .output();
    for _ in 0..50 {
        if !agent_is_loaded(target) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn install_login_item(config_path: &Path) -> Result<()> {
    use antiknob::host::login_item;
    let home = home_dir()?;
    let exe = login_item::preferred_executable(&std::env::current_exe()?);
    let plist_path = login_item::install(&home, &exe, config_path)?;
    println!("[ ==> ] Login item will start {}.", exe.display());
    println!("[ ==> ] Wrote {}.", plist_path.display());
    let domain = user_domain()?;
    let target = format!("{}/{}", domain, login_item::AGENT_LABEL);
    // Loading an already-loaded agent fails; unload first for idempotence.
    bootout_and_wait(&target);
    let load = std::process::Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(&plist_path)
        .output()?;
    if !load.status.success() {
        anyhow::bail!(
            "launchctl bootstrap failed: {}\n\
             The agent is written at {}; `launchctl bootstrap {} {}` retries it.",
            String::from_utf8_lossy(&load.stderr).trim(),
            plist_path.display(),
            domain,
            plist_path.display()
        );
    }
    println!(
        "[ Ok  ] Login item installed and loaded ({}).",
        login_item::AGENT_LABEL
    );
    Ok(())
}

fn uninstall_login_item() -> Result<()> {
    use antiknob::host::login_item;
    let home = home_dir()?;
    let domain = user_domain()?;
    let target = format!("{}/{}", domain, login_item::AGENT_LABEL);
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &target])
        .output();
    if login_item::uninstall(&home)? {
        println!("[ Ok  ] Login item removed.");
    } else {
        println!("[ Ok  ] No login item was installed.");
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = match cli.config {
        Some(p) => p,
        None => default_config_path()?,
    };

    if cli.mcp {
        let tap_health =
            std::sync::Arc::new(std::sync::Mutex::new(antiknob::api::TapHealth::default()));
        let ctx = antiknob::api::ApiContext::with_health(config_path, None, tap_health);
        return antiknob::api::run_mcp_server(ctx);
    }

    if cli.uninstall_login_item {
        uninstall_login_item()?;
        return Ok(());
    }
    let cfg = load_or_default(&config_path)?;
    if cli.install_login_item {
        install_login_item(&config_path)?;
        return Ok(());
    }
    println!(
        "[ ==> ] Loaded {} host layer(s) (double-tap switch: {})",
        cfg.layers.len(),
        cfg.double_tap_switch
    );

    let tap = std::sync::Arc::new(std::sync::Mutex::new(antiknob::host::tap::TapEngine::new(
        cfg,
    )));
    let tap_health =
        std::sync::Arc::new(std::sync::Mutex::new(antiknob::api::TapHealth::default()));

    let _socket_server = if !cli.no_socket {
        let sock_path = match cli.socket_path {
            Some(p) => p,
            None => antiknob::api::default_socket_path()?,
        };
        let api_ctx = antiknob::api::ApiContext::with_health(
            config_path.clone(),
            Some(tap.clone()),
            tap_health.clone(),
        );
        match antiknob::api::SocketServer::start(sock_path.clone(), api_ctx) {
            Ok(server) => {
                println!(
                    "[ ==> ] Unix domain socket listening at {}",
                    sock_path.display()
                );
                Some(server)
            }
            Err(e) => {
                println!(
                    "[ Wrn ] Could not start Unix socket at {}: {}",
                    sock_path.display(),
                    e
                );
                None
            }
        }
    } else {
        None
    };

    if cli.active {
        println!("[ ==> ] ACTIVE mode: slot chords are swallowed and synthesized.");
        run_active(
            tap,
            tap_health,
            cli.timeout_secs,
            &config_path,
            !cli.no_tray,
        );
    } else {
        println!("[ ==> ] OBSERVE mode: chords pass through, actions are only logged.");
        run_observe(
            tap,
            tap_health,
            cli.timeout_secs,
            &config_path,
            !cli.no_tray,
            cli.verbose,
        );
    }
}
