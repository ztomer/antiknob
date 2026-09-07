//! Regression tests for the lighting-mode crash (2026-09-06).
//!
//! Crash: `GuiState::apply_led_to_device` (and `save_to_device` /
//! `refresh_device`) spawned a worker thread that called `HidApi::new()`.
//! hidapi's vendored C backend schedules its IOHIDManager on the calling
//! thread's CFRunLoop (`IOHIDManagerScheduleWithRunLoop(hid_mgr,
//! CFRunLoopGetCurrent(), ...)`), which traps (SIGTRAP in `hid_enumerate`)
//! on any non-main thread, killing the app. Fix: all HID work runs
//! synchronously on the GUI main thread, and the constructor performs no
//! HID I/O at all. These tests pin both halves of that fix without
//! touching real hardware (libtest itself runs tests on worker threads,
//! so any HID call here would trap whenever any HID device is present).

use antiknob::gui::state::GuiState;

fn crate_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The constructor must stay I/O-free: no HID calls means it can never
/// trap, on any thread, with or without hardware attached. The GUI performs
/// the first scan explicitly on the main thread after construction.
#[test]
fn constructor_performs_no_hid_io() {
    let state = GuiState::new();
    assert!(
        state.device.is_none(),
        "constructor must not scan: device must start as None"
    );
    assert_eq!(
        state.status_message, "Scanning for Anticater VK01 USB device...",
        "constructor must leave the pending placeholder for the GUI scan"
    );
}

/// Structural pin: no worker threads may exist around HID calls in the GUI
/// state machine. If anyone re-adds `thread::spawn` to `state.rs`, the
/// crash class returns, so fail loudly here instead of in production.
#[test]
fn no_worker_threads_around_hid() {
    let state_src = std::fs::read_to_string(crate_root().join("src/gui/state.rs"))
        .expect("read src/gui/state.rs");
    assert!(
        !state_src.contains("thread::spawn"),
        "src/gui/state.rs must not spawn threads: HID calls are main-thread-only"
    );
    let device_src =
        std::fs::read_to_string(crate_root().join("src/device.rs")).expect("read src/device.rs");
    assert!(
        device_src.contains("main thread") || device_src.contains("main-thread"),
        "src/device.rs must document the main-thread requirement"
    );
}
