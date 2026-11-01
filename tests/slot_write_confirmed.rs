//! Every slot write is confirmed by a read-back, structurally.
//!
//! `bind-slots` flashed fifteen slots and printed "Flashed 15 slot
//! binding(s)" on the strength of `hid_write` returning, which this repo
//! has learned many times over says nothing about what the slot holds. The
//! daemon's keymap flash did the same. `bind-seq` and `upload` read back;
//! the rest each decided for themselves, and two decided not to.
//!
//! So there is ONE writer, `device::write_slots`: wipe, write, commit, read
//! the whole table back, compare. A slot write is not committed without the
//! commit, so no source outside that writer may send one -- a new path that
//! writes slots and skips the read-back turns this red by construction.

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

/// The commit is defined in `device/mod.rs` and sent only by the writer.
const ALLOWED: [&str; 2] = ["src/device/mod.rs", "src/device/slot_write.rs"];

#[test]
fn only_the_confirming_writer_commits_slot_writes() {
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
            if code.contains("send_commit(") {
                offenders.push(format!("{rel}:{}", n + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "slot writes committed outside device::write_slots, so nothing reads them back: {offenders:?}"
    );
    let writer = std::fs::read_to_string(root.join("src/device/slot_write.rs")).unwrap();
    assert!(
        writer.contains("read_full_table(") && writer.contains("verify("),
        "the one writer must read the whole table back and compare it"
    );
}
