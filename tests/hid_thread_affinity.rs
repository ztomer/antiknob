//! Regression tests for the HID thread-affinity crash class.
//!
//! hidapi's macOS backend binds its IOHIDManager sources to the CFRunLoop of
//! whichever thread first ran `hid_init`. A later call from a *different*
//! thread makes IOKit re-apply device matching against that stored runloop
//! from the wrong thread and CoreFoundation traps inside `CFRunLoopAddSource`
//! (EXC_BREAKPOINT / SIGTRAP). Calling it re-entrantly from a thread that is
//! already dispatching its own runloop aborts instead (SIGABRT).
//!
//! Two instances of this one class shipped: the GUI calling hidapi on its
//! event-loop thread (2026-09-06), and the daemon's socket accept thread
//! calling `list_devices` through `execute_command` (2026-09-07 -- it crashed
//! `cargo test` roughly one run in four).
//!
//! The fix is structural: `device::with_hid` / `device::with_device` marshal
//! every hidapi call onto one dedicated, event-loop-free thread that owns the
//! `HidApi` for the process's lifetime. These tests pin the behaviour (many
//! concurrent callers must not trap) and the structure (nothing outside
//! `src/device.rs` may name hidapi or hold a `HidDevice`).

use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Behavioural pin. Before the fix this trapped inside CoreFoundation with
/// SIGTRAP; the process died, so the assertion below never mattered -- a
/// clean exit *is* the assertion. Runs with or without hardware attached:
/// enumeration is the crashing call, and it enumerates either way.
#[test]
fn concurrent_workers_never_trap_in_hidapi() {
    let handles: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..4 {
                    // The result depends on what is plugged in; only the
                    // absence of a trap is under test.
                    let _ = antiknob::device::list_devices();
                }
            })
        })
        .collect();

    for h in handles {
        h.join()
            .expect("a worker thread panicked calling into hidapi");
    }
}

/// The API layer runs on the daemon's socket accept thread, which is exactly
/// where the 2026-09-07 crash came from. Drive it from a worker.
#[test]
fn api_get_status_from_a_worker_thread_never_traps() {
    let handle = std::thread::spawn(|| {
        let dir = std::env::temp_dir().join(format!("antiknob_affinity_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let mut ctx = antiknob::api::ApiContext::new(dir.join("host.json"), None);
        for _ in 0..4 {
            antiknob::api::execute_command(&mut ctx, antiknob::api::Command::GetStatus {})
                .expect("get_status must succeed with or without hardware");
        }
        let _ = std::fs::remove_dir_all(&dir);
    });
    handle
        .join()
        .expect("socket-style worker panicked in get_status");
}

/// Structural pin: the `hidapi` crate is reachable only from `src/device/`.
///
/// Naming `device::HidDevice` in a signature is fine -- a handle can only be
/// *obtained* inside a `with_hid` / `with_device` job, because `HidApi::new`
/// and `open_device_on` live behind this wall. Reaching the crate directly
/// is what lets a caller build one on the wrong thread and resurrect the
/// crash class, so that is what fails here rather than in production.
#[test]
fn hidapi_is_confined_to_the_device_module() {
    let src = crate_root().join("src");
    let owner = crate_root().join("src/device");
    let mut offenders = Vec::new();
    let mut scanned = 0usize;

    let mut stack = vec![src];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            for entry in std::fs::read_dir(&path).expect("read src dir") {
                stack.push(entry.expect("dir entry").path());
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") || path.starts_with(&owner) {
            continue;
        }
        scanned += 1;
        let text = std::fs::read_to_string(&path).expect("read source");
        for token in ["hidapi::", "use hidapi", "extern crate hidapi"] {
            if text.contains(token) {
                offenders.push(format!("{} names {:?}", rel(&path), token));
            }
        }
    }

    // Calibration: a scope that matched nothing would pass vacuously.
    assert!(
        scanned > 5,
        "expected to scan the crate's modules, only saw {scanned} files"
    );
    assert!(
        offenders.is_empty(),
        "the hidapi crate must stay inside src/device/; \
         reach handles through device::with_hid / device::with_device:\n{}",
        offenders.join("\n")
    );
}

fn rel(path: &Path) -> String {
    path.strip_prefix(crate_root())
        .unwrap_or(path)
        .display()
        .to_string()
}
