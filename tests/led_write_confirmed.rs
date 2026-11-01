//! Every LED write is read back, structurally (PLAN S.11).
//!
//! `upload` and the daemon's keymap flash sent `send_led` packets after the
//! confirmed slot write and reported nothing about them; only `set_led` read
//! back. So there is ONE LED writer, `device::write_led`: init, write, read
//! the layer's mode back, compare. The raw send is private to it, and a new
//! path that calls `send_led(` anywhere else turns this red.

use std::path::{Path, PathBuf};

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out
}

/// The raw send lives beside the one writer that reads it back.
const ALLOWED: [&str; 1] = ["src/device/led_state.rs"];

#[test]
fn only_the_confirming_writer_sends_led_packets() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sources = rust_sources(&root.join("src"));
    assert!(
        sources.len() > 40,
        "the scan lost its subject: {}",
        sources.len()
    );
    let mut offenders = Vec::new();
    for path in sources {
        let rel = path.strip_prefix(&root).unwrap().display().to_string();
        if ALLOWED.contains(&rel.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains("send_led(") {
                offenders.push(format!("{rel}:{}", n + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "LED packets sent outside device::write_led, so nothing reads them back: {offenders:?}"
    );
    let writer = std::fs::read_to_string(root.join("src/device/led_state.rs")).unwrap();
    assert!(
        writer.contains("pub fn write_led(") && writer.contains("led_verdict("),
        "the one LED writer must read the mode back and compare it"
    );
}
