//! The process-wide HID executor.
//!
//! Thread-affinity contract (macOS). hidapi's IOHIDManager backend schedules
//! its device sources onto the CFRunLoop of whichever thread first ran
//! hid_init. A later call from a *different* thread makes IOKit re-apply
//! device matching against that stored runloop from the wrong thread, and
//! CoreFoundation traps inside CFRunLoopAddSource (EXC_BREAKPOINT / SIGTRAP,
//! observed 2026-09-06 and again 2026-09-07 from the daemon's socket accept
//! thread). Calling it re-entrantly from a thread that is *currently*
//! dispatching its own runloop aborts instead (SIGABRT, observed in the GUI).
//!
//! Both crashes are the same class: "hidapi touched from more than one
//! thread, or from inside a live event loop". The fix is structural rather
//! than disciplinary -- ONE dedicated worker thread owns the HidApi for the
//! process's lifetime and every hidapi call is marshalled onto it through
//! `with_hid` / `with_device`. That thread runs no event loop of its own, so
//! neither crash is representable, and the HidApi is created once instead of
//! per call (hid_init/hid_exit churn was itself corrupting the manager).
//!
//! Consequence: nothing outside `src/device/` may reach the `hidapi` crate.
//! Handles are obtained only inside a `with_hid` / `with_device` job. Pinned
//! structurally by `tests/hid_thread_affinity.rs`.
//!
//! Second contract, same thread: the enumeration is REFRESHED before every
//! job. `HidApi::new` takes one snapshot of the bus, and a long-lived
//! process that never re-enumerates keeps answering from it forever. Unplug
//! the knob and plug it back in and macOS issues a new device path, so the
//! daemon's cached entry names a device that no longer exists: every open
//! fails, `get_status` still reports the stale list as connected, and the
//! settings app draws a green dot beside hardware it cannot touch. Observed
//! 2026-09-08 -- a replug left the daemon reporting `DevSrvsID:4315664188`
//! while a freshly started CLI saw `DevSrvsID:4316393885`, and every LED
//! write through the daemon had been failing silently since.
//!
//! Refreshing here rather than at the call sites is deliberate: a rule that
//! every device command must remember to re-enumerate is a rule one of them
//! eventually forgets, and the failure it produces is invisible.

use anyhow::{anyhow, Result};
use hidapi::HidApi;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread;

use super::open_device_on;

/// Re-exported so signatures elsewhere can name the handle without
/// reaching for the `hidapi` crate. A handle is only obtainable inside a
/// `with_hid` / `with_device` job, which is what keeps hidapi on one thread.
pub use hidapi::HidDevice;

/// A unit of hidapi work, handed to the dedicated HID thread. Receives the
/// process-wide `HidApi`, or the initialisation error if it never came up.
type HidJob = Box<dyn FnOnce(Result<&HidApi, &str>) + Send>;

static HID_TX: OnceLock<Sender<HidJob>> = OnceLock::new();

/// How many times the HID thread has re-enumerated the bus.
///
/// Exists so the refresh-before-every-job contract is testable without a
/// human unplugging a knob: the alternative is asserting nothing and
/// discovering the regression the way it was discovered the first time.
static ENUMERATION_REFRESHES: AtomicU64 = AtomicU64::new(0);

/// Reads that counter. Pinned by `tests/hid_thread_affinity.rs`.
pub fn enumeration_refreshes() -> u64 {
    ENUMERATION_REFRESHES.load(Ordering::Relaxed)
}

/// Lazily starts the process's single HID thread and returns its job queue.
fn hid_tx() -> &'static Sender<HidJob> {
    HID_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<HidJob>();
        thread::Builder::new()
            .name("antiknob-hid".to_string())
            .spawn(move || match HidApi::new() {
                // Created once and never dropped: repeated hid_init/hid_exit
                // is part of the crash class described above.
                Ok(mut api) => {
                    for job in rx {
                        // Before the job, never after: the job is about to
                        // enumerate, and a list refreshed afterwards is a
                        // list the job never saw.
                        match api.refresh_devices() {
                            Ok(()) => {
                                ENUMERATION_REFRESHES.fetch_add(1, Ordering::Relaxed);
                                job(Ok(&api));
                            }
                            // Reported rather than swallowed. A refresh that
                            // fails leaves a stale list behind, and running
                            // the job against it is how a dead handle gets
                            // presented as a live device.
                            Err(e) => {
                                let msg = format!("Failed to re-enumerate HID devices: {}", e);
                                job(Err(&msg));
                            }
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to initialize HIDAPI: {}", e);
                    for job in rx {
                        job(Err(&msg));
                    }
                }
            })
            .expect("failed to spawn the antiknob-hid thread");
        tx
    })
}

/// Runs `job` on the dedicated HID thread and blocks until it returns.
///
/// This is the ONLY way to reach hidapi. Jobs run one at a time in arrival
/// order, so no additional locking is needed. Never call `with_hid` (or
/// `with_device`) from inside a job -- that self-deadlocks.
pub fn with_hid<T, F>(job: F) -> Result<T>
where
    F: FnOnce(&HidApi) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    let (reply_tx, reply_rx) = mpsc::channel();
    let boxed: HidJob = Box::new(move |api| {
        let outcome = match api {
            Ok(api) => job(api),
            Err(e) => Err(anyhow!("{}", e)),
        };
        // A closed receiver just means the caller went away; not our problem.
        let _ = reply_tx.send(outcome);
    });

    hid_tx()
        .send(boxed)
        .map_err(|_| anyhow!("HID worker thread is not running"))?;
    reply_rx
        .recv()
        .map_err(|_| anyhow!("HID worker thread died while handling the request"))?
}

/// Opens the knob's vendor interface on the HID thread and runs `job`
/// against it. The handle never leaves that thread.
pub fn with_device<T, F>(job: F) -> Result<T>
where
    F: FnOnce(&HidDevice) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    with_hid(move |api| {
        let dev = open_device_on(api)?;
        job(&dev)
    })
}
