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

    /// Where the daemon lives. Sources of truth: `policy.rs`
    /// (`TMP_SOCKET_PATH`) and the Application Support layout both sides
    /// hard-code.
    enum Socket {
        /// Convenience symlink the daemon creates when it can.
        static let tmpPath = "/tmp/antiknob.sock"
        /// The real socket, relative to home.
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
}
