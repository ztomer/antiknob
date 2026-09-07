//! Naming the switch a user has to flip, given what macOS grants.
//!
//! A CGEventTap that will not create is almost always a missing TCC grant,
//! and the daemon used to say so by naming both candidate grants and neither
//! item: "grant Accessibility (and Input Monitoring) to this binary". That
//! is a sentence with no noun in it -- System Settings has two panes and a
//! list of apps, and "this binary" is on none of them.
//!
//! macOS answers both halves precisely, so the daemon asks it (see the
//! binary's `permissions` module) and this module turns the answer into one
//! line. Pure on purpose: the reporting is the part that was wrong, and
//! withholding a real grant to test it means System Settings and a reboot.

use std::path::{Path, PathBuf};

/// What macOS says a process may do with the keyboard.
///
/// A tap created with `CGEventTapOptions::Default` (the active-mode grab)
/// needs Accessibility; a listen-only tap needs Input Monitoring. Both modes
/// exist in this daemon, so both are worth reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grants {
    pub accessibility: bool,
    pub input_monitoring: bool,
}

/// The item the Privacy & Security panes list, and the path to add when
/// they do not list it yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantItem {
    pub name: String,
    pub path: PathBuf,
}

/// Name the switch the way System Settings names it.
///
/// TCC identifies a process by its code signature, so an executable inside
/// an app bundle IS the bundle as far as the pane is concerned: it shows the
/// app's name and its icon, and its `+` button will not accept the
/// executable buried under `Contents/MacOS`. Walk up to the bundle whenever
/// there is one, and fall back to the bare executable when there is not.
pub fn grant_item(exe: &Path) -> GrantItem {
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")));
    let path = bundle.unwrap_or(exe);
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    GrantItem {
        name,
        path: path.to_path_buf(),
    }
}

/// The one line to print when the tap will not create.
pub fn tap_failure_line(grants: Grants, item: &GrantItem) -> String {
    let panes = match (grants.accessibility, grants.input_monitoring) {
        (false, false) => "Accessibility and Input Monitoring",
        (false, true) => "Accessibility",
        (true, false) => "Input Monitoring",
        // Both held and the tap still refused: not a grant problem, so do
        // not send the user to a switch that is already on.
        (true, true) => {
            return format!(
                "the window server refused the keyboard tap, though {} already holds \
                 Accessibility -- restarting the daemon usually clears it",
                item.name
            );
        }
    };
    format!(
        "knob gestures are off: switch on {} in System Settings > Privacy & Security > {} \
         (not listed? click + and add {})",
        item.name,
        panes,
        item.path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLE_EXE: &str =
        "/Applications/Antiknob/AntiknobDaemon.app/Contents/MacOS/AntiknobDaemon";

    #[test]
    fn an_executable_in_a_bundle_is_named_by_its_bundle() {
        let item = grant_item(Path::new(BUNDLE_EXE));
        assert_eq!(item.name, "AntiknobDaemon");
        assert_eq!(
            item.path,
            PathBuf::from("/Applications/Antiknob/AntiknobDaemon.app")
        );
    }

    #[test]
    fn a_loose_executable_is_named_by_itself() {
        let item = grant_item(Path::new("/Applications/Antiknob/bin/antiknob-daemon"));
        assert_eq!(item.name, "antiknob-daemon");
        assert_eq!(
            item.path,
            PathBuf::from("/Applications/Antiknob/bin/antiknob-daemon")
        );
    }

    #[test]
    fn a_nameless_path_still_produces_a_message() {
        let item = grant_item(Path::new("/"));
        assert_eq!(item.name, "/");
    }

    fn line(accessibility: bool, input_monitoring: bool) -> String {
        tap_failure_line(
            Grants {
                accessibility,
                input_monitoring,
            },
            &grant_item(Path::new(BUNDLE_EXE)),
        )
    }

    #[test]
    fn the_message_names_only_the_pane_that_is_actually_missing() {
        let ax = line(false, true);
        assert!(ax.contains("Accessibility"), "{ax}");
        assert!(!ax.contains("Input Monitoring"), "{ax}");

        let hid = line(true, false);
        assert!(hid.contains("Input Monitoring"), "{hid}");
        // "Privacy & Security > Input Monitoring" must not drag the other
        // pane along: naming both when one is granted is the old bug.
        assert!(!hid.contains("> Accessibility"), "{hid}");

        assert!(line(false, false).contains("Accessibility and Input Monitoring"));
    }

    #[test]
    fn the_message_names_the_switch_and_the_thing_to_add() {
        let msg = line(false, true);
        assert!(msg.contains("switch on AntiknobDaemon"), "{msg}");
        assert!(
            msg.contains("/Applications/Antiknob/AntiknobDaemon.app"),
            "{msg}"
        );
        assert_eq!(msg.lines().count(), 1, "one line, printed once: {msg}");
    }

    #[test]
    fn holding_every_grant_does_not_send_the_user_to_a_switch() {
        let msg = line(true, true);
        assert!(!msg.contains("System Settings"), "{msg}");
        assert!(msg.contains("AntiknobDaemon"), "{msg}");
    }
}
