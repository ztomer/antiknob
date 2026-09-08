//! Daemon run loops (observe + active) plus tap plumbing.
//!
//! Split from main.rs to respect the file-length gate. Entry points
//! `run_observe` and `run_active` both diverge.

use antiknob::api::TapHealth;
use antiknob::host::engine::{EngineEvent, FiredAction};
use antiknob::host::output::plan;
use antiknob::host::tap::TapEngine;
use antiknob::host::HostConfig;

use super::synth::synthesize;
use super::tray::{TrayAction, TrayUi};
use crate::keytap::{self, Decision, KeyEvent};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

enum DaemonMsg {
    Input(KeyEvent),
    TapDead(String),
}

pub(crate) fn default_config_path() -> Result<PathBuf> {
    antiknob::host::default_config_path().context("HOME is not set")
}

pub(crate) fn load_or_default(path: &Path) -> Result<HostConfig> {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let cfg = HostConfig::default_config();
        std::fs::write(path, cfg.to_json_pretty())?;
        println!("[ ==> ] Wrote default host config to {}", path.display());
        return Ok(cfg);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(HostConfig::load_json(&text))
}

/// Whether the keyboard tap is currently up, for the menu bar to show.
fn tap_is_up(health: &Arc<Mutex<TapHealth>>) -> bool {
    health.lock().map(|h| h.active).unwrap_or(false)
}

fn describe_fire(action: &FiredAction) -> String {
    match action {
        FiredAction::Scroll { lines } => format!("scroll {:+} lines", lines),
        FiredAction::KeyChord { key, mods } => format!("keystroke key={} mods={:?}", key, mods),
        FiredAction::Sequence(steps) => format!("sequence ({} steps)", steps.len()),
        FiredAction::Aux(key) => format!("media/brightness {:?}", key),
        FiredAction::MouseClick { button } => format!("mouse click {:?}", button),
        FiredAction::LaunchApp { bundle_id } => format!("launch {}", bundle_id),
        FiredAction::OpenUrl { url } => format!("open url {}", url),
        FiredAction::OpenPath { path } => format!("open path {}", path),
        FiredAction::QuitApp { bundle_id, force } => {
            format!("quit {} (force={})", bundle_id, force)
        }
    }
}

/// Observe-mode logging; active mode synthesizes instead.
///
/// Takes the config because a layer change is not only a log line: the
/// knob's backlight follows the active HOST layer, and the firmware has no
/// idea host layers exist, so the daemon writes the new layer's mode on
/// every switch. `led_sync` decides whether there is anything to write and
/// does it off this thread -- one of these call sites is the CGEventTap
/// callback, and macOS disables a tap whose callback runs long.
fn handle_events(out: Vec<EngineEvent>, active: bool, cfg: &HostConfig) {
    for ev in out {
        match ev {
            EngineEvent::Fire(action) => {
                if active {
                    for op in plan(&action) {
                        synthesize(&op);
                    }
                    println!("[slot] FIRED {}", describe_fire(&action));
                } else {
                    println!("[slot] WOULD {}", describe_fire(&action));
                }
            }
            EngineEvent::LayerChanged(idx) => {
                println!("[slot] layer -> {}", idx + 1);
                antiknob::host::led_sync::sync_led(cfg, idx);
            }
            EngineEvent::NoOp => {}
        }
    }
}

/// Report a dead tap once, not once every retry.
///
/// The supervisor rebuilds the tap every three seconds; the reason it failed
/// changes far less often than that, and the old warning spent three lines on
/// each attempt -- a log that grows forever while saying one thing. Print a
/// reason the first time it is seen and stay quiet until it changes, and let
/// macOS raise its own Accessibility prompt once, which puts the daemon in
/// the pane's list with a switch beside it instead of leaving the user to
/// find a path.
fn log_tap_warning(last: &mut Option<String>, reason: &str) {
    if last.as_deref() == Some(reason) {
        return;
    }
    *last = Some(reason.to_string());
    eprintln!("[ Wrn ] {}", reason);
    if !crate::permissions::current_grants().accessibility {
        crate::permissions::prompt_for_accessibility_once();
    }
}

/// Watches the host config file for external edits (hand edits, GUI
/// export). mtime-granular; a changed-but-unparseable file keeps the
/// last-good config with a warning instead of resetting to defaults.
struct ConfigWatch {
    path: PathBuf,
    last_mtime: Option<std::time::SystemTime>,
}

impl ConfigWatch {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            last_mtime: std::fs::metadata(path).and_then(|m| m.modified()).ok(),
        }
    }

    /// True once per external modification. Callers reload via
    /// `HostConfig::try_load_json` and keep last-good on failure.
    fn changed(&mut self) -> bool {
        let current = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        if current != self.last_mtime {
            self.last_mtime = current;
            true
        } else {
            false
        }
    }
}

fn maybe_reload(tap: &mut TapEngine, watch: &mut ConfigWatch) {
    if !watch.changed() {
        return;
    }
    match std::fs::read_to_string(&watch.path)
        .ok()
        .and_then(|text| HostConfig::try_load_json(&text))
    {
        Some(cfg) => {
            let n = cfg.layers.len();
            tap.apply_config(cfg);
            println!("[ ==> ] Reloaded host config ({} layer(s)).", n);
        }
        None => println!("[ Wrn ] Host config changed but unreadable; keeping last-good."),
    }
}

/// Read the config file now and apply it, keeping last-good on failure.
/// Used by the tray Reload action in both modes.
fn force_reload(tap: &mut TapEngine, path: &Path) {
    match std::fs::read_to_string(path)
        .ok()
        .and_then(|text| HostConfig::try_load_json(&text))
    {
        Some(cfg) => {
            let n = cfg.layers.len();
            tap.apply_config(cfg);
            println!("[ ==> ] Reloaded host config ({} layer(s)).", n);
        }
        None => println!("[ Wrn ] Host config unreadable; keeping last-good."),
    }
}

/// Apply one tray action. Returns true when the daemon should exit.
fn apply_tray_action(
    action: TrayAction,
    tap: &mut TapEngine,
    config_path: &Path,
    active: bool,
) -> bool {
    match action {
        TrayAction::SwitchLayer(i) => {
            let out = tap.set_layer(i);
            handle_events(out, active, tap.config());
            false
        }
        TrayAction::ReloadNow => {
            force_reload(tap, config_path);
            false
        }
        TrayAction::OpenAccessibility => {
            crate::permissions::open_accessibility_pane();
            false
        }
        TrayAction::Quit => {
            println!("[ Ok  ] Quit from menu bar.");
            true
        }
    }
}

pub(crate) fn run_observe(
    tap: Arc<Mutex<TapEngine>>,
    tap_health: Arc<Mutex<TapHealth>>,
    timeout_secs: u64,
    config_path: &Path,
    with_tray: bool,
    verbose: bool,
) -> ! {
    let (tx, rx) = channel();
    let thread_health = Arc::clone(&tap_health);
    std::thread::spawn(move || loop {
        if let Ok(mut h) = thread_health.lock() {
            h.active = true;
            h.error = None;
        }
        let tx_clone = tx.clone();
        let res = keytap::listen(move |ev| {
            let _ = tx_clone.send(DaemonMsg::Input(ev));
        });
        let reason = match res {
            Err(e) => e,
            Ok(()) => "listener ended".to_string(),
        };
        if let Ok(mut h) = thread_health.lock() {
            h.active = false;
            h.error = Some(reason.clone());
        }
        let _ = tx.send(DaemonMsg::TapDead(reason));
        std::thread::sleep(Duration::from_secs(3));
    });

    let mut watch = ConfigWatch::new(config_path);
    let mut last_tap_warning: Option<String> = None;
    let mut tray = if with_tray {
        match TrayUi::new(&tap, tap_is_up(&tap_health)) {
            Ok(t) => Some(t),
            Err(e) => {
                println!("[ Wrn ] Menu bar unavailable ({}); continuing headless.", e);
                None
            }
        }
    } else {
        None
    };
    let start = Instant::now();
    let tick = Duration::from_millis(50);

    loop {
        if timeout_secs > 0 && start.elapsed().as_secs() >= timeout_secs {
            println!("[ Ok  ] Observe window closed.");
            std::process::exit(0);
        }
        let now_ms = start.elapsed().as_millis() as u64;
        match rx.recv_timeout(tick) {
            Ok(DaemonMsg::Input(ev)) => {
                let KeyEvent { code, pressed } = ev;
                if let Ok(mut h) = tap_health.lock() {
                    h.events_seen += 1;
                }
                if let Ok(mut t) = tap.lock() {
                    if verbose {
                        println!(
                            "[key] code={} {} mods={:?}",
                            code,
                            if pressed { "down" } else { "up" },
                            t.held_mods()
                        );
                    }
                    let out = if pressed {
                        t.key(code, true, now_ms)
                    } else {
                        t.release_swallow(code);
                        vec![]
                    };
                    handle_events(out, false, t.config());
                }
            }
            Ok(DaemonMsg::TapDead(reason)) => {
                log_tap_warning(&mut last_tap_warning, &reason);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                log_tap_warning(&mut last_tap_warning, "listener thread disconnected");
            }
        }
        if let Ok(mut t) = tap.lock() {
            maybe_reload(&mut t, &mut watch);
            let out = t.poll_expiry(start.elapsed().as_millis() as u64);
            handle_events(out, false, t.config());
        }
        if let Some(tray) = tray.as_mut() {
            tray.refresh(&tap, tap_is_up(&tap_health));
            if let Ok(mut t) = tap.lock() {
                for action in tray.poll() {
                    if apply_tray_action(action, &mut t, config_path, false) {
                        std::process::exit(0);
                    }
                }
            }
        }
    }
}

pub(crate) fn run_active(
    tap: Arc<Mutex<TapEngine>>,
    tap_health: Arc<Mutex<TapHealth>>,
    timeout_secs: u64,
    config_path: &Path,
    with_tray: bool,
    verbose: bool,
) -> ! {
    if timeout_secs > 0 {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout_secs));
            println!("[ Ok  ] Active window closed.");
            std::process::exit(0);
        });
    }

    // Double-tap expiry has no event to ride on when the user stops
    // touching the knob, so poll it on a timer. Same timer also picks up
    // external config edits (instant apply).
    let timer_tap = Arc::clone(&tap);
    let watch_path: PathBuf = config_path.to_path_buf();
    std::thread::spawn(move || {
        let start = Instant::now();
        let mut watch = ConfigWatch::new(&watch_path);
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let now_ms = start.elapsed().as_millis() as u64;
            if let Ok(mut t) = timer_tap.lock() {
                maybe_reload(&mut t, &mut watch);
                let out = t.poll_expiry(now_ms);
                handle_events(out, true, t.config());
            }
        }
    });

    // The grab tap runs on a worker thread (proven safe by probe); the
    // menu bar must live on the main thread, which runs the loop below.
    let grab_tap = Arc::clone(&tap);
    let grab_health = Arc::clone(&tap_health);
    std::thread::spawn(move || {
        let mut last_warning: Option<String> = None;
        loop {
            if let Ok(mut h) = grab_health.lock() {
                h.active = true;
                h.error = None;
            }
            let start = Instant::now();
            let thread_tap = Arc::clone(&grab_tap);
            // Counted and logged HERE, in the grab callback, because this is
            // the mode the daemon actually runs in. `--verbose` used to log
            // only in observe mode while its own help promised it
            // "diagnoses silent taps" -- so the one flag for the job printed
            // nothing in the one mode that needed it, and its silence read
            // as a dead tap rather than as a missing instrument.
            let ev_health = Arc::clone(&grab_health);
            let res = keytap::grab(move |ev: KeyEvent| -> Decision {
                let KeyEvent { code, pressed } = ev;
                let now_ms = start.elapsed().as_millis() as u64;
                if let Ok(mut h) = ev_health.lock() {
                    h.events_seen += 1;
                }
                let Ok(mut t) = thread_tap.lock() else {
                    return Decision::Pass;
                };
                // Logged AFTER the lock so the held modifiers can be shown:
                // a slot chord is a keycode AND its mods, and a log of bare
                // keycodes cannot distinguish "wrong key" from "right key,
                // mods not seen" -- the two failures that look identical
                // from outside.
                let mods_now = t.held_mods();
                let decision = if pressed {
                    let out = t.key(code, true, now_ms);
                    if out.is_empty() {
                        Decision::Pass
                    } else {
                        handle_events(out, true, t.config());
                        Decision::Swallow
                    }
                } else if t.release_swallow(code) {
                    Decision::Swallow
                } else {
                    Decision::Pass
                };
                // The SWALLOW decision is the half that matters when a chord
                // also reaches another application: "we saw it" and "we
                // consumed it" are different claims, and only the second says
                // whether the system still gets the keystroke. Logging the
                // first alone sent this session chasing a phantom collision.
                if verbose {
                    println!(
                        "[key] code={} {} mods={:?} -> {}",
                        code,
                        if pressed { "down" } else { "up" },
                        mods_now,
                        if matches!(decision, Decision::Swallow) {
                            "SWALLOW"
                        } else {
                            "pass"
                        }
                    );
                }
                decision
            });
            let reason = match res {
                Err(e) => e,
                Ok(()) => "event tap ended".to_string(),
            };
            if let Ok(mut h) = grab_health.lock() {
                h.active = false;
                h.error = Some(reason.clone());
            }
            log_tap_warning(&mut last_warning, &reason);
            std::thread::sleep(Duration::from_secs(3));
        }
    });

    let mut tray = if with_tray {
        match TrayUi::new(&tap, tap_is_up(&tap_health)) {
            Ok(t) => Some(t),
            Err(e) => {
                println!("[ Wrn ] Menu bar unavailable ({}); continuing headless.", e);
                None
            }
        }
    } else {
        None
    };
    loop {
        std::thread::sleep(Duration::from_millis(50));
        if let Some(tray) = tray.as_mut() {
            tray.refresh(&tap, tap_is_up(&tap_health));
            if let Ok(mut t) = tap.lock() {
                for action in tray.poll() {
                    if apply_tray_action(action, &mut t, config_path, true) {
                        std::process::exit(0);
                    }
                }
            }
        }
    }
}
