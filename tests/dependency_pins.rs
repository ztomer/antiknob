//! The house TUI is a PINNED git dependency on the gates_of_heck repo (OW.40).
//!
//! antiknob used to reach goh-tui by `path = "../gates_of_heck/crates/goh-tui"`,
//! which made every build depend on a sibling checkout that CI, the push gate's
//! export and a fresh clone did not have. Two things keep that from coming
//! back: no manifest entry may point into a gates_of_heck checkout, and the
//! goh-tui dependency must name a `git` remote and a full 40-hex `rev`.
//!
//! Both read the manifest as TEXT rather than through a TOML parser: this test
//! must fail with the offending LINE in its message, and the cases it refuses
//! (a path into a sibling repo) are one-line facts.

use std::path::PathBuf;

fn manifest() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The line declaring the `goh-tui` dependency (there is exactly one).
fn goh_tui_dependency_line(manifest: &str) -> String {
    manifest
        .lines()
        .find(|line| {
            let line = line.trim_start();
            line.starts_with("goh-tui") && line.contains('=')
        })
        .expect("Cargo.toml declares no goh-tui dependency")
        .trim()
        .to_string()
}

#[test]
fn no_dependency_points_into_a_gates_of_heck_checkout() {
    let manifest = manifest();
    let offenders: Vec<&str> = manifest
        .lines()
        .filter(|line| {
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            compact.starts_with("goh-tui")
                || compact.contains("path=\"../gates_of_heck")
                || compact.contains("path=\"../goh")
        })
        .filter(|line| line.contains("path"))
        .collect();
    assert!(
        offenders.is_empty(),
        "a dependency reaches into a gates_of_heck checkout again; antiknob must build without one (OW.40):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn goh_tui_is_a_git_dependency_pinned_to_a_full_rev() {
    let line = goh_tui_dependency_line(&manifest());
    assert!(
        line.contains("git = \"https://github.com/ztomer/gates_of_heck"),
        "goh-tui must come from the gates_of_heck git remote, not a checkout: {line}"
    );
    let rev = line
        .split("rev = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("goh-tui names no `rev` (a branch or tag can move): {line}"));
    assert!(
        rev.len() == 40 && rev.chars().all(|c| c.is_ascii_hexdigit()),
        "goh-tui's `rev` must be a full 40-hex commit sha, got {rev:?}: {line}"
    );
}
