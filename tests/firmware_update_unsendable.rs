//! The vendor's firmware-update entry packet, `03 EF EF`
//! (`KB_UpData_SoftWare` @0x10003bd70), is never sent.
//!
//! Two halves, because "no builder makes it" is not the same claim as "no
//! write can carry it":
//!
//! * every packet builder, swept over every address and value it takes, never
//!   produces a payload that starts `EF EF` (`restore_packet` is the one that
//!   takes caller-supplied bytes, so it is fed `EF EF` on purpose);
//! * the single door every HID write passes (`device::send_report`) calls
//!   `firmware::ensure_sendable`, which refuses the packet with or without the
//!   report id, and `send_report` holds the only `.write(` in the crate.

use antiknob::firmware::{ensure_sendable, is_firmware_update, REPORT_LEN};
use antiknob::protocol::{build_led_packet, Action};
use antiknob::{fd, led};
use std::path::{Path, PathBuf};

fn update_packet() -> Vec<u8> {
    let mut p = vec![0u8; REPORT_LEN];
    p[0] = 0x03;
    p[1] = 0xEF;
    p[2] = 0xEF;
    p
}

#[test]
fn the_guard_refuses_the_update_packet_in_every_framing() {
    let with_id = update_packet();
    let without_id = with_id[1..].to_vec();
    for p in [
        &with_id,
        &without_id,
        &vec![0x03, 0xEF, 0xEF],
        &vec![0xEF, 0xEF],
    ] {
        assert!(is_firmware_update(p), "{p:02x?}");
        let err = ensure_sendable(p).expect_err("must refuse");
        assert!(err.to_string().contains("firmware-update"), "{err}");
    }
}

#[test]
fn the_guard_passes_everything_else() {
    for p in [
        vec![0x03, 0xFD, 2, 1, 1],
        vec![0x03, 0xFE, 0xB0, 0, 3],
        vec![0x03, 0xFB, 0xFB, 0xFB],
        vec![0x03, 0xEF, 0x00],
        vec![0x03, 0x00, 0xEF],
        vec![0xEF],
        vec![],
    ] {
        assert!(!is_firmware_update(&p), "{p:02x?}");
        ensure_sendable(&p).unwrap_or_else(|e| panic!("{p:02x?}: {e}"));
    }
}

fn never_the_update(label: &str, p: &[u8]) {
    assert!(
        !is_firmware_update(p),
        "{label} produced the firmware-update packet: {:02x?}",
        &p[..4.min(p.len())]
    );
}

#[test]
fn no_builder_can_produce_the_update_packet() {
    let actions: Vec<Action> = [
        "a",
        "cmd-c",
        "ctrl-alt-f16",
        "mute",
        "click",
        "rclick",
        "mclick",
        "wheelup",
        "wheeldown",
        "swipe-left",
        "swipe-right",
        "swipe-up",
        "swipe-down",
        "like",
    ]
    .iter()
    .map(|s| Action::parse(s).unwrap())
    .collect();
    // Every slot address and layer the u8 fields allow, for every action.
    for key_id in 0..=u8::MAX {
        for layer in 0..3u8 {
            for a in &actions {
                never_the_update("Action::to_packet", &a.to_packet(key_id, layer));
                if let Ok(p) = fd::build_packet(key_id, layer, &[fd::Step::now(a.clone())]) {
                    never_the_update("fd::build_packet", &p);
                }
            }
            never_the_update("fd::wipe_packet", &fd::wipe_packet(key_id, layer));
        }
    }
    // Garbage and update-shaped records through the one builder that copies
    // caller bytes: it rewrites byte 1, so byte 1 can never stay `EF`.
    for record in [
        update_packet(),
        vec![0xEF; REPORT_LEN],
        vec![0x03, 0xEF, 0xEF],
    ] {
        never_the_update("fd::restore_packet", &fd::restore_packet(&record));
    }
    for layer in 0..=u8::MAX {
        for mode in [
            "off",
            "red",
            "green",
            "ripple",
            "rainbow",
            "rgb",
            "mode0",
            "mode5",
            "rgb keys=vendor",
            "off keys=ef,ef",
            "ef ef",
        ] {
            if let Ok(p) = build_led_packet(layer, mode) {
                never_the_update("build_led_packet", &p);
            }
        }
    }
    never_the_update("led_init_packet", &led::led_init_packet());
}

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

/// A second `.write(` would be a door the guard does not stand in.
#[test]
fn the_only_hid_write_sits_behind_the_guard() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sources = rust_sources(&root.join("src"));
    assert!(sources.len() > 40, "the scan lost its subject");
    let mut doors = Vec::new();
    for path in sources {
        let rel = path.strip_prefix(&root).unwrap().display().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains("dev.write(") || code.contains("device.write(") {
                doors.push((rel.clone(), n + 1));
            }
        }
    }
    assert_eq!(doors.len(), 1, "HID write sites: {doors:?}");
    assert_eq!(doors[0].0, "src/device/mod.rs");
    let text = std::fs::read_to_string(root.join("src/device/mod.rs")).unwrap();
    let write_at = text.lines().position(|l| l.contains("dev.write(")).unwrap();
    let guard_at = text
        .lines()
        .position(|l| l.contains("ensure_sendable("))
        .expect("send_report must call ensure_sendable");
    assert!(guard_at < write_at, "the guard must come before the write");
}
