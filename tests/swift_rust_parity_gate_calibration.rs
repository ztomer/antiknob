//! Calibration for `tools/check_swift_rust_model_parity.py`.
//!
//! The companion to `swift_rust_model_parity.rs`, which proves the daemon can
//! read what the app writes. This file proves the GATE that keeps the two
//! models agreeing can actually go red.
//!
//! A parity gate is easy to write and easy to make vacuous: tune the
//! extractors until the two sides agree, and every later run reports green
//! whether or not it can tell a real disagreement from a renamed struct. Each
//! test below re-introduces one actual drift — the four that shipped, plus the
//! two ways this gate was itself caught being blind — and requires a non-zero
//! exit AND the finding's own words in the output. Checking the exit code
//! alone would accept a gate that died of an exception, which is red without
//! having detected anything.
//!
//! Split from `swift_rust_model_parity.rs` for the file-length gate.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GATE: &str = "tools/check_swift_rust_model_parity.py";
const HELPER: &str = "tools/_parity_extract.py";
const HELPER_NAME: &str = "_parity_extract.py";

/// The model files the gate reads, mirrored into a scratch tree.
///
/// Relative paths, because the gate takes a root through
/// `ANTIKNOB_PARITY_ROOT` — the seam that lets a test point it at a copy with
/// one thing deliberately broken. Without it none of this is testable, which
/// is the state these tests were written from.
const MODEL_FILES: &[&str] = &[
    "src/host/mod.rs",
    "src/host/gesture.rs",
    "src/host/virtual_layer.rs",
    "src/firmware.rs",
    "ui/Sources/AntiknobUI/Core/Models.swift",
    "ui/Sources/AntiknobUI/Core/LayerVariant.swift",
    "ui/Sources/AntiknobUI/Core/LedModes.swift",
    "ui/Sources/AntiknobUI/Core/Gesture.swift",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Copy the real model files into a fresh temp tree; the caller then edits one.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("antiknob-parity-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for rel in MODEL_FILES {
        let dst = dir.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).expect("scratch dirs");
        }
        std::fs::copy(repo_root().join(rel), &dst).expect("copy model file");
    }
    dir
}

/// Copy the tree, then apply one edit to one file in it.
fn scratch_with_edit(tag: &str, target: &str, old: &str, new: &str) -> PathBuf {
    let dir = scratch(tag);
    let victim = dir.join(target);
    let text = std::fs::read_to_string(&victim).expect("read scratch model file");
    assert!(
        text.contains(old),
        "the anchor for this test no longer exists in {target}: {old:?}. A calibration \
         test whose anchor has rotted passes for the wrong reason, so it refuses to run \
         rather than reporting green."
    );
    std::fs::write(&victim, text.replacen(old, new, 1)).expect("write scratch model file");
    dir
}

fn run_gate_at(root: &Path) -> Output {
    Command::new("python3")
        .arg(repo_root().join(GATE))
        .env("ANTIKNOB_PARITY_ROOT", root)
        .output()
        .expect("python3 is required: the parity gate is written in it")
}

fn run_gate_in_repo() -> Output {
    Command::new("python3")
        .arg(repo_root().join(GATE))
        .env_remove("ANTIKNOB_PARITY_ROOT")
        .output()
        .expect("run the parity gate")
}

/// Require RED, and require the output to name the finding.
///
/// The second half is the one that matters. A gate that exits non-zero because
/// it crashed has detected nothing, and a test that only checked the exit code
/// would call that a success.
fn assert_red(tag: &str, root: &Path, must_mention: &str) {
    let out = run_gate_at(root);
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "the parity gate reported PASS on a tree where {tag} is broken.\n\
         A gate that cannot go red is a rug, not a ratchet.\n--- its output ---\n{text}"
    );
    assert!(
        text.contains(must_mention),
        "the gate went red for {tag} but never mentioned {must_mention:?}, so it may have \
         failed for an unrelated reason.\n--- its output ---\n{text}"
    );
    let _ = std::fs::remove_dir_all(root);
}

// ── the real tree must be green ─────────────────────────────────────────────
//
// Without this, "every test below is red" would look identical to "every test
// below works".

#[test]
fn the_parity_gate_passes_on_the_real_tree() {
    let out = run_gate_in_repo();
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "the parity gate is red on the real tree:\n{text}"
    );
    assert!(text.contains("allowlist EMPTY"), "{text}");
}

/// An unmodified COPY must be green too, which is what makes the red results
/// below attributable to the edit rather than to the copying.
#[test]
fn an_unmodified_copy_of_the_tree_is_green() {
    let dir = scratch("control");
    let out = run_gate_at(&dir);
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "an unmodified copy was red:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── the four drifts that shipped ────────────────────────────────────────────

#[test]
fn the_openurl_spelling_is_caught() {
    let root = scratch_with_edit(
        "openurl",
        "ui/Sources/AntiknobUI/Core/Models.swift",
        r#"try c.encode("openUrl", forKey: .type)"#,
        r#"try c.encode("openURL", forKey: .type)"#,
    );
    assert_red("the app writes openURL", &root, "action-tags");
}

#[test]
fn the_bundle_id_spelling_is_caught() {
    let root = scratch_with_edit(
        "bundleid",
        "ui/Sources/AntiknobUI/Core/Models.swift",
        "try c.encode(bundleId, forKey: .bundle_id)",
        "try c.encode(bundleId, forKey: .bundleId)",
    );
    assert_red("the app writes bundleId", &root, "action-fields");
}

#[test]
fn the_delay_spelling_is_caught() {
    let root = scratch_with_edit(
        "delay",
        "ui/Sources/AntiknobUI/Core/Models.swift",
        "try c.encodeIfPresent(delayMs, forKey: .delay_ms)",
        "try c.encodeIfPresent(delayMs, forKey: .delayMs)",
    );
    assert_red("the app writes delayMs", &root, "seqstep-keys");
}

#[test]
fn losing_variants_is_caught() {
    let root = scratch_with_edit(
        "variants",
        "ui/Sources/AntiknobUI/Core/Models.swift",
        "        case variants\n",
        "",
    );
    assert_red(
        "the app's LayerConfig has no variants key",
        &root,
        "layer-keys",
    );
}

// ── the ways this gate was itself caught being blind ────────────────────────
//
// These four are not hypothetical. Each one left the gate GREEN while a real
// disagreement existed, and each is here because being wrong that way is
// invisible: the gate reported a clean bill of health it had not earned.

/// A declaration the extractor cannot find must FAIL, not pass.
///
/// Renaming `AuxKey` and leaving the regex unmatched produced a silent pass. A
/// gate that reports "no disagreement" when it matched nothing is reporting
/// over an empty scope, which is indistinguishable from agreement and stays
/// that way for as long as the refactor lasts.
#[test]
fn an_unfindable_declaration_fails_rather_than_passing() {
    let root = scratch_with_edit(
        "renamed",
        "src/host/mod.rs",
        "pub enum AuxKey {",
        "pub enum AuxKeyRenamed {",
    );
    assert_red(
        "AuxKey was renamed out from under the gate",
        &root,
        "EXTRACTOR FOUND NOTHING",
    );
}

/// `HostConfig`'s own top-level fields were compared by NOTHING.
///
/// Renaming the struct left the gate green, and so did a misspelt
/// `boundDeviceLayer`, because the arms that existed only asked what the app
/// could EXPRESS — which a wrong key trivially can.
#[test]
fn a_written_key_the_daemon_cannot_read_is_caught() {
    let root = scratch_with_edit(
        "emitted",
        "ui/Sources/AntiknobUI/Core/Models.swift",
        "try c.encode(boundDeviceLayers, forKey: .boundDeviceLayers)",
        "try c.encode(boundDeviceLayers, forKey: .boundDeviceLayer)",
    );
    assert_red(
        "the app writes boundDeviceLayer",
        &root,
        "config-keys-emitted",
    );
}

/// `case name, apps` declares TWO keys, and reading only the first made `apps`
/// look absent from the app's model — a false drift, in the other direction.
///
/// Removing `apps` from the app's declaration is caught by the express/read
/// arms; `variant-keys-emitted` stays green because the encoder still writes
/// `apps` and the daemon does have the field. Naming the arm that fires is the
/// point: a test that accepted any red exit could not tell this apart from a
/// crash.
#[test]
fn a_multi_name_codingkeys_case_is_fully_read() {
    let root = scratch_with_edit(
        "apps",
        "ui/Sources/AntiknobUI/Core/LayerVariant.swift",
        "        case name, apps\n",
        "        case name\n",
    );
    assert_red(
        "the app's LayerVariant lost `apps`",
        &root,
        "variant-keys-accepted",
    );
}

// ── the tables the app renders ──────────────────────────────────────────────

#[test]
fn a_swapped_led_mode_number_is_caught() {
    let root = scratch_with_edit(
        "ledorder",
        "src/firmware.rs",
        r#""off", "red", "green", "ripple", "rainbow", "rgb""#,
        r#""off", "red", "green", "ripple", "rgb", "rainbow""#,
    );
    assert_red("two LED modes swapped", &root, "led-modes");
}

#[test]
fn a_changed_vendor_palette_entry_is_caught() {
    let root = scratch_with_edit(
        "rainbow",
        "src/firmware.rs",
        "    (0xFF, 0x80, 0x30),\n",
        "    (0xFF, 0x80, 0x40),\n",
    );
    assert_red("the vendor rainbow changed", &root, "vendor-rainbow");
}

#[test]
fn an_aux_key_the_daemon_lacks_is_caught() {
    let root = scratch_with_edit(
        "aux",
        "ui/Sources/AntiknobUI/Core/Models.swift",
        "    case mute\n",
        "    case mute\n    case eject\n",
    );
    assert_red("the app encodes an unknown aux key", &root, "aux-keys");
}

#[test]
fn a_gesture_removed_from_one_side_is_caught() {
    let root = scratch_with_edit(
        "gesture",
        "src/host/gesture.rs",
        "        Gesture::HoldTwistR,\n",
        "",
    );
    assert_red("the daemon lost a gesture", &root, "gestures-count");
}

// ── ratchet, not rug ───────────────────────────────────────────────────────

/// A stale allowlist entry FAILS.
///
/// An allowlist nobody prunes becomes a permanent exemption, and the class it
/// was added to police goes back to being unpoliced while the list still reads
/// like coverage. The entry is seeded by rewriting the shipped `ALLOWLIST`
/// literal in a COPY of the gate rather than by adding an env knob to the real
/// one: this is a property of the gate's logic, so the test runs the real
/// logic.
#[test]
fn a_stale_allowlist_entry_fails() {
    let dir = scratch("allowlist");
    let gate_src = std::fs::read_to_string(repo_root().join(GATE)).expect("read the gate");
    let literal = "ALLOWLIST: dict[str, str] = {}";
    assert!(
        gate_src.contains(literal),
        "the gate's ALLOWLIST is no longer the empty literal this test rewrites, so the \
         stale-entry path is untested. Re-point the test; do not delete it."
    );
    let seeded = gate_src.replacen(
        literal,
        "ALLOWLIST: dict[str, str] = {\n    \"action-tags\": \"seeded by the calibration test\",\n}",
        1,
    );
    // The seeded copy needs its sibling too. The gate imports
    // `_parity_extract` from its own directory, so a copy in the scratch tree
    // cannot resolve it unless the helper is copied alongside — which is how
    // the two ship, so copying both reproduces the shipped arrangement rather
    // than inventing one.
    std::fs::copy(repo_root().join(HELPER), dir.join(HELPER_NAME))
        .expect("copy the gate's helper module beside the seeded gate");
    let seeded_path = dir.join("seeded_gate.py");
    std::fs::write(&seeded_path, seeded).expect("write the seeded gate");

    let out = Command::new("python3")
        .arg(&seeded_path)
        .env("ANTIKNOB_PARITY_ROOT", &dir)
        .output()
        .expect("run the seeded gate");
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "an allowlist entry matching nothing passed, so this allowlist is a rug.\n{text}"
    );
    assert!(
        text.contains("stale allowlist"),
        "the seeded entry did not fail as stale:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every model file the gate reads must exist and be non-trivially large.
///
/// A file that silently became empty or stubbed would leave the gate comparing
/// nothing — the same failure as an unfindable declaration, and harder to
/// notice because nothing is missing.
#[test]
fn the_gate_has_a_real_subject_on_both_sides() {
    for rel in MODEL_FILES {
        let path = repo_root().join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} is missing: {e}", path.display()));
        assert!(
            text.len() > 200,
            "{} is {len} bytes; the gate reads it for declarations, so an empty or stub \
             file means it is checking nothing",
            path.display(),
            len = text.len()
        );
    }
}
