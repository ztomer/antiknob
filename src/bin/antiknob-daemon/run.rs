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
use anyhow::{Context, Result};
use rdev::{Event, EventType, Key};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

enum DaemonMsg {
    Input(Event),
    TapDead(String),
}

/// CG keycode mirror of rdev's macOS mapping for the keys the daemon
/// cares about; F16+ arrive as `Key::Unknown` carrying the raw code.
fn cg_code(key: &Key) -> Option<u16> {
    match key {
        Key::ControlLeft => Some(59),
        Key::ControlRight => Some(62),
        Key::Alt => Some(58),
        Key::AltGr => Some(61),
        Key::ShiftLeft => Some(56),
        Key::ShiftRight => Some(60),
        Key::MetaLeft => Some(55),
        Key::MetaRight => Some(54),
        Key::F1 => Some(122),
        Key::F2 => Some(120),
        Key::F3 => Some(99),
        Key::F4 => Some(118),
        Key::F5 => Some(96),
        Key::F6 => Some(97),
        Key::F7 => Some(98),
        Key::F8 => Some(100),
        Key::F9 => Some(101),
        Key::F10 => Some(109),
        Key::F11 => Some(103),
        Key::F12 => Some(111),
        Key::Unknown(code) => (*code).try_into().ok(),
        _ => None,
    }
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
fn handle_events(out: Vec<EngineEvent>, active: bool) {
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
            EngineEvent::LayerChanged(idx) => println!("[slot] layer -> {}", idx + 1),
            EngineEvent::NoOp => {}
        }
    }
}

fn log_tap_warning(reason: &str) {
    eprintln!("[ Wrn ] Event tap unavailable: {}", reason);
    eprintln!("        Grant Accessibility (and Input Monitoring) in macOS System Settings.");
    eprintln!("        Daemon socket server and USB HID management remain active.");
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
            handle_events(tap.set_layer(i), active);
            false
        }
        TrayAction::ReloadNow => {
            force_reload(tap, config_path);
            false
        }
        TrayAction::Quit => {
            println!("[ Ok  ] Quit from menu bar.");
            true
        }
    }
}

/// Verbose tap diagnostics: every non-movement event, so gestures that
/// emit wheel, button, or media actions are visible too — not just
/// keyboard chords. MouseMove is skipped (flood).
pub(crate) fn log_raw_event(ev: &Event) {
    match &ev.event_type {
        EventType::KeyPress(key) => match cg_code(key) {
            Some(code) => println!("[raw] key down code={}", code),
            None => println!("[raw] key down {:?}", key),
        },
        EventType::KeyRelease(key) => match cg_code(key) {
            Some(code) => println!("[raw] key up code={}", code),
            None => println!("[raw] key up {:?}", key),
        },
        EventType::ButtonPress(button) => println!("[raw] button press {:?}", button),
        EventType::ButtonRelease(button) => println!("[raw] button release {:?}", button),
        EventType::Wheel { delta_x, delta_y } => {
            println!("[raw] wheel x={} y={}", delta_x, delta_y)
        }
        EventType::MouseMove { .. } => {}
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
        let res = rdev::listen(move |ev| {
            let _ = tx_clone.send(DaemonMsg::Input(ev));
        });
        let reason = match res {
            Err(e) => format!("{:?}", e),
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
    let mut tray = if with_tray {
        match TrayUi::new(&tap) {
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
                if verbose {
                    log_raw_event(&ev);
                }
                let code = match ev.event_type {
                    EventType::KeyPress(key) => cg_code(&key).map(|c| (c, true)),
                    EventType::KeyRelease(key) => cg_code(&key).map(|c| (c, false)),
                    _ => None,
                };
                if let Some((code, pressed)) = code {
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
                        handle_events(out, false);
                    }
                }
            }
            Ok(DaemonMsg::TapDead(reason)) => {
                log_tap_warning(&reason);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                log_tap_warning("listener thread disconnected");
            }
        }
        if let Ok(mut t) = tap.lock() {
            maybe_reload(&mut t, &mut watch);
            handle_events(t.poll_expiry(start.elapsed().as_millis() as u64), false);
        }
        if let Some(tray) = tray.as_mut() {
            tray.refresh(&tap);
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
                handle_events(t.poll_expiry(now_ms), true);
            }
        }
    });

    // The grab tap runs on a worker thread (proven safe by probe); the
    // menu bar must live on the main thread, which runs the loop below.
    let grab_tap = Arc::clone(&tap);
    let grab_health = Arc::clone(&tap_health);
    std::thread::spawn(move || loop {
        if let Ok(mut h) = grab_health.lock() {
            h.active = true;
            h.error = None;
        }
        let start = Instant::now();
        let thread_tap = Arc::clone(&grab_tap);
        let res = rdev::grab(move |ev: Event| -> Option<Event> {
            let code = match ev.event_type {
                EventType::KeyPress(key) => cg_code(&key).map(|c| (c, true)),
                EventType::KeyRelease(key) => cg_code(&key).map(|c| (c, false)),
                _ => None,
            };
            let Some((code, pressed)) = code else {
                return Some(ev);
            };
            let now_ms = start.elapsed().as_millis() as u64;
            let Ok(mut t) = thread_tap.lock() else {
                return Some(ev);
            };
            if pressed {
                let out = t.key(code, true, now_ms);
                if out.is_empty() {
                    Some(ev)
                } else {
                    handle_events(out, true);
                    None
                }
            } else if t.release_swallow(code) {
                None
            } else {
                Some(ev)
            }
        });
        let reason = match res {
            Err(e) => format!("{:?}", e),
            Ok(()) => "event tap ended".to_string(),
        };
        if let Ok(mut h) = grab_health.lock() {
            h.active = false;
            h.error = Some(reason.clone());
        }
        log_tap_warning(&reason);
        std::thread::sleep(Duration::from_secs(3));
    });

    let mut tray = if with_tray {
        match TrayUi::new(&tap) {
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
            tray.refresh(&tap);
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
