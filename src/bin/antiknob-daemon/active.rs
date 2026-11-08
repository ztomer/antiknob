//! Active mode: the run loop that swallows and synthesizes slot chords.
//!
//! Split from `run.rs` to respect the file-length gate and the pedantic
//! `too_many_lines` limit: each thread the loop starts is a named function.

use super::hold_recorder;
use super::run::{
    apply_tray_action, elapsed_ms, elapsed_us, handle_events, log_tap_warning, maybe_reload,
    tap_is_up, ConfigWatch,
};
use super::tray::TrayUi;
use crate::keytap::{self, KeyEvent};
use antiknob::api::TapHealth;
use antiknob::host::hold_stats::HoldSample;
use antiknob::host::tap::{TapEngine, Verdict};
use antiknob::policy::{LOOP_TICK_MS, MOD_HOLD_TICK_MS, TAP_REBUILD_DELAY_SECS};
use goh_tui::{OK, WARN};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) fn run_active(
    tap: &Arc<Mutex<TapEngine>>,
    tap_health: &Arc<Mutex<TapHealth>>,
    timeout_secs: u64,
    config_path: &Path,
    with_tray: bool,
    verbose: bool,
) -> ! {
    if timeout_secs > 0 {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout_secs));
            goh_tui::outln!("{OK} Active window closed.");
            std::process::exit(0);
        });
    }

    // ONE clock for every timestamp the engine compares: key presses, the
    // double-tap expiry and the modifier hold. The grab loop used to restart
    // its own clock on every tap rebuild while the timer kept the first one,
    // so after a rebuild a press looked ages old to the expiry check.
    let epoch = Instant::now();

    spawn_expiry_timer(tap, config_path, epoch);

    // The hold instrument (AK.HOLDMEASURE). The tap callback must never touch
    // the disk -- a slow callback is how the system disables a tap -- so it
    // only sends; this thread owns the file.
    let sample_tx = hold_recorder::spawn();

    spawn_hold_replay(tap, &sample_tx, epoch, verbose);
    spawn_grab(tap, tap_health, sample_tx, epoch, verbose);

    let mut tray = if with_tray {
        match TrayUi::new(tap, tap_is_up(tap_health)) {
            Ok(t) => Some(t),
            Err(e) => {
                goh_tui::outln!("{WARN} Menu bar unavailable ({}); continuing headless.", e);
                None
            }
        }
    } else {
        None
    };
    loop {
        std::thread::sleep(Duration::from_millis(LOOP_TICK_MS));
        if let Some(tray) = tray.as_mut() {
            tray.refresh(tap, tap_is_up(tap_health));
            if let Ok(mut t) = tap.lock() {
                for action in tray.poll() {
                    if apply_tray_action(&action, &mut t, config_path, true) {
                        std::process::exit(0);
                    }
                }
            }
        }
    }
}

/// Double-tap expiry has no event to ride on when the user stops touching
/// the knob, so poll it on a timer. Same timer also picks up external config
/// edits (instant apply).
fn spawn_expiry_timer(tap: &Arc<Mutex<TapEngine>>, config_path: &Path, epoch: Instant) {
    let timer_tap = Arc::clone(tap);
    let watch_path = config_path.to_path_buf();
    std::thread::spawn(move || {
        let mut watch = ConfigWatch::new(&watch_path);
        loop {
            std::thread::sleep(Duration::from_millis(LOOP_TICK_MS));
            let now_ms = elapsed_ms(epoch);
            if let Ok(mut t) = timer_tap.lock() {
                maybe_reload(&mut t, &mut watch);
                let out = t.poll_expiry(now_ms);
                handle_events(out, true, t.config());
            }
        }
    });
}

/// A held slot-chord modifier press that no key completed goes back into
/// the stream when its hold runs out (AK.LEAK; `TapEngine::decide`).
/// Posted with the lock released, so the tap callback is never blocked
/// behind a post that re-enters it.
fn spawn_hold_replay(
    tap: &Arc<Mutex<TapEngine>>,
    samples: &Sender<HoldSample>,
    epoch: Instant,
    verbose: bool,
) {
    let hold_tap = Arc::clone(tap);
    let hold_samples = samples.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(MOD_HOLD_TICK_MS));
        let now_us = elapsed_us(epoch);
        let (due, samples) = hold_tap
            .lock()
            .map(|mut t| (t.expire_held(now_us), t.take_hold_samples()))
            .unwrap_or_default();
        hold_recorder::send_all(&hold_samples, samples);
        if !due.is_empty() {
            if verbose {
                println!(
                    "[hold] replay {:?}",
                    due.iter().map(|h| h.code).collect::<Vec<_>>()
                );
            }
            keytap::replay_held(&due);
        }
    });
}

fn spawn_grab(
    tap: &Arc<Mutex<TapEngine>>,
    tap_health: &Arc<Mutex<TapHealth>>,
    sample_tx: Sender<HoldSample>,
    epoch: Instant,
    verbose: bool,
) {
    // The grab tap runs on a worker thread (proven safe by probe); the
    // menu bar must live on the main thread, which runs the loop below.
    let grab_tap = Arc::clone(tap);
    let grab_health = Arc::clone(tap_health);
    std::thread::spawn(move || {
        let mut last_warning: Option<String> = None;
        loop {
            if let Ok(mut h) = grab_health.lock() {
                h.active = true;
                h.error = None;
            }
            let thread_tap = Arc::clone(&grab_tap);
            // Counted and logged HERE, in the grab callback, because this is
            // the mode the daemon actually runs in. `--verbose` used to log
            // only in observe mode while its own help promised it
            // "diagnoses silent taps" -- so the one flag for the job printed
            // nothing in the one mode that needed it, and its silence read
            // as a dead tap rather than as a missing instrument.
            let ev_health = Arc::clone(&grab_health);
            let ev_samples = sample_tx.clone();
            let res = keytap::grab(move |ev: KeyEvent, flags: u64| -> Verdict {
                let KeyEvent { code, pressed } = ev;
                let now_us = elapsed_us(epoch);
                if let Ok(mut h) = ev_health.lock() {
                    h.events_seen += 1;
                }
                let Ok(mut t) = thread_tap.lock() else {
                    return Verdict::Pass;
                };
                // Logged AFTER the lock so the held modifiers can be shown:
                // a slot chord is a keycode AND its mods, and a log of bare
                // keycodes cannot distinguish "wrong key" from "right key,
                // mods not seen" -- the two failures that look identical
                // from outside.
                let mods_now = t.held_mods();
                let step = t.decide(code, pressed, flags, now_us);
                hold_recorder::send_all(&ev_samples, t.take_hold_samples());
                if !step.out.is_empty() {
                    handle_events(step.out, true, t.config());
                }
                // The verdict is the half that matters when a chord also
                // reaches another application: "we saw it" and "we consumed
                // it" are different claims, and only the second says whether
                // the system still gets the keystroke. Logging the first
                // alone sent this session chasing a phantom collision -- and
                // logging only the KEY's verdict hid that the chord's
                // modifiers were passing (AK.LEAK).
                if verbose {
                    let shown = match &step.verdict {
                        Verdict::Pass => "pass".to_string(),
                        Verdict::Swallow => "SWALLOW".to_string(),
                        Verdict::Hold => "HOLD".to_string(),
                        Verdict::Replay(h) => format!(
                            "replay {:?} then pass",
                            h.iter().map(|h| h.code).collect::<Vec<_>>()
                        ),
                    };
                    println!(
                        "[key] code={} {} mods={:?} -> {}",
                        code,
                        if pressed { "down" } else { "up" },
                        mods_now,
                        shown
                    );
                }
                step.verdict
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
            std::thread::sleep(Duration::from_secs(TAP_REBUILD_DELAY_SECS));
        }
    });
}
