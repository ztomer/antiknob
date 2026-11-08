// AppConstants.swift — every cross-boundary value in one place.
//
// Two maps, one file: firmware facts mirrored from `src/firmware.rs` and
// process budgets mirrored from `src/policy.rs`. Each entry cites its Rust
// source so a changed budget can be audited across the language boundary --
// true single-sourcing is not possible here (Swift cannot include a Rust
// module), so agreement by citation plus matching tests is the honest
// version. Values nothing else must agree with (layout metrics, glyphs)
// stay where they are used: centralizing those would imply a contract that
// does not exist.

import Foundation

enum AppConstants {
    /// Firmware facts. Source of truth: `src/firmware.rs`.
    enum Firmware {
        /// The vendor configuration endpoint's usage page. Commands go to
        /// the interface carrying this page (`VENDOR_USAGE_PAGE`).
        static let vendorUsagePage = 0xFF00
    }

    /// Where the daemon lives. Source of truth: `api::default_socket_path`
    /// in `src/api/socket.rs`; this is the Swift side's one spelling of it
    /// (`tests/socket_path_one_source.rs` holds both to that).
    enum Socket {
        /// The daemon's socket, relative to home. There is no `/tmp`
        /// symlink any more.
        static let appSupportSubpath = "Library/Application Support/antiknob/antiknob.sock"
        /// The host config, relative to home. Read when the daemon is down.
        static let hostConfigSubpath = "Library/Application Support/antiknob/host.json"
    }

    /// Per-call socket timeouts, in seconds. Source of truth for the
    /// intervals: `policy.rs` (`SOCKET_*_TIMEOUT_SECS`); the per-call
    /// split below is this client's own judgment (a knob-mode walk costs
    /// more than a status probe).
    enum RpcTimeout {
        static let standard = 2
        static let getStatus = 1
        static let listCommands = 3
        static let knobMode = 5
    }

    /// Status poll cadence, in seconds. Faster polls re-read the HID bus;
    /// the knob-mode walk stays off it (see `ConfigStore.knobModeRaw`).
    enum Polling {
        static let statusInterval = 2.0
    }

    /// Application metadata.
    enum AppInfo {
        /// Application version string (e.g. "0.15.3").
        static var version: String {
            if let ver = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String, !ver.isEmpty {
                return ver
            }
            return "0.15.3"
        }

        /// Application build number string (e.g. "45").
        static var build: String {
            if let b = Bundle.main.infoDictionary?["CFBundleVersion"] as? String, !b.isEmpty {
                return b
            }
            return ""
        }

        /// Combined version presentation string, e.g. "v0.14.0 (45)" or "v0.14.0".
        static var versionDisplay: String {
            let b = build
            return b.isEmpty ? "v\(version)" : "v\(version) (\(b))"
        }
    }
}
