//! Regression tests for the two HID crash classes (2026-09-06 SIGTRAP,
//! 2026-09-07 SIGABRT).
//!
//! hidapi's macOS backend pumps the calling thread's CFRunLoop inside
//! hid_enumerate: worker threads trap (SIGTRAP), and the GUI main thread
//! aborts reentrantly inside the running event loop (SIGABRT). The fix is
//! architectural: the GUI process never calls hidapi at all and shells
//! out to the CLI binary instead (see `gui::clihid`). These tests pin
//! both halves: an I/O-free constructor and zero in-process HID calls in
//! `src/gui`.

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

/// Structural pin: no in-process HID calls may exist anywhere under
/// src/gui. If anyone re-adds direct hidapi use to the GUI, the crash
/// classes return, so fail loudly here instead of in production.
#[test]
fn no_in_process_hid_in_gui() {
    let gui_dir = crate_root().join("src/gui");
    let mut offenders = Vec::new();
    let mut files = vec![gui_dir.clone()];
    while let Some(path) = files.pop() {
        if path.is_dir() {
            for entry in std::fs::read_dir(&path).expect("read gui dir") {
                files.push(entry.expect("dir entry").path());
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read gui source");
        for token in [
            "hidapi::",
            "open_device(",
            "list_devices(",
            "send_report(",
            "HidDevice",
            "thread::spawn",
        ] {
            if src.contains(token) {
                offenders.push(format!(
                    "{} contains forbidden token {:?}",
                    path.display(),
                    token
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "GUI must shell out to the CLI, never touch HID in-process:\n{}",
        offenders.join("\n")
    );
}
