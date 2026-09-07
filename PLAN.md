# Antiknob Plan (forward-looking)

Shipped work lives in git history and in the release notes on each tag; this
file carries only what is still open. `git log --oneline` and
`git show <tag>` are the record of what landed when.

## Context

* Hardware: Anticater VK01 knob, VID `0x514C` (LQKJ), PID `0x8850`
  (serial `EB60121120051103`); vendor usage page `0xFF00`, report `0x03`.
  2.4GHz receivers and Bluetooth are detected too (see `SUPPORTED_DEVICES`).
* Zero-sudo IOHIDManager access. MIT OR Apache-2.0, native arm64, macOS 26+.
* Architecture: the GUI and CLI stay TCC-free (device flashing, presets, LED,
  YAML). The daemon alone needs Accessibility / Input Monitoring for host-side
  translation. `host.json` is the contract between them.
* All hidapi calls are marshalled onto one dedicated thread by
  `device::with_hid` / `device::with_device`; nothing outside `src/device/`
  may reach the `hidapi` crate. See `tests/hid_thread_affinity.rs` for why.
* The Swift app is an SPM package (`ui/Package.swift`): `AntiknobUI` holds the
  views, models and store; `Antiknob` holds only `@main`, because `@main`
  cannot live in a library. Tests reach the library with `@testable import`,
  so nothing needs a `public` annotation it would not otherwise have.

## Open

### 1. Coverage floors are ratchets, not bars

Both floors are the measured figure with the decimal shaved off, set so they
can only be met by keeping tests, never by picking a number: Rust 61
(`GOH_COV_FLOOR_RUST`, measured 61.92%) and Swift 3 (`GOH_SWIFT_COV_MIN`,
measured 3.4% over SOURCES -- the house checker stopped counting `Tests/` on
2026-09-07, because a test file is ~100% covered by definition and a floor set
on that can be met by tests that assert nothing).

The Swift number is low because ~6,000 of its ~7,300 lines are SwiftUI view
bodies no unit test executes. The logic that has a seam scores far higher --
`Models.swift` 64%, `SocketWire` near total. Two ways to make the number mean
more, in order of value:

* Split the package into a logic target and a views target so a floor can
  apply where coverage is meaningful. Blocked on the house side, not here:
  `swift_gate.sh` calls `check_swift_coverage.py --min N` directly and never
  reaches `coverage_gate.sh`, which is the script that supports per-target
  floors (`--floors-json`). Routing Swift through it would fix this for every
  repo.
* Keep putting seams under the logic that has none. `ConfigStore` (659 lines,
  0%) is the largest remaining one; its `init()` loads config and starts a
  2s poll timer, so it cannot be constructed in a test as it stands.

`ui/.swiftlint-baseline.json` is the companion ratchet: 8 entries, all
body-length and complexity on view bodies and the two exhaustive `Action`
coding switches. Shrink-only -- a new violation fails and so does a listed one
that GROWS, because the match key includes the measured count. It has been
re-recorded twice, both times verified file-agnostically as zero new debt
first, which is the only form of re-record it permits.

### 2. `rdev` drags in a crate a future rustc will reject

`cargo build` warns that `block v0.1.6` "contains code that will be rejected by
a future version of Rust". The lint is `static of uninhabited type`
(rust-lang/rust#74840) on `_NSConcreteStackBlock`, and it becomes a hard error.

Investigated 2026-09-07, so the next session need not repeat it:

* The chain is `antiknob -> rdev 0.5.3 -> cocoa 0.22.0 -> block 0.1.6`.
* **0.5.3 is the newest rdev**; there is no version to bump to, and `block` is
  unmaintained, so waiting is not a plan.
* The surface is small: `Event`, `EventType`, `Key`, `rdev::listen` and
  `rdev::grab`, at exactly two call sites (`src/host/tap.rs:240`,
  `src/bin/antiknob-daemon/run.rs:375`).
* `tap.rs` already speaks CG keycodes internally and holds its own mirror of
  rdev's macOS mapping, so the translation layer that would normally dominate
  such a port is already written.
* The hard part is `grab`: swallowing an event needs a `CGEventTap` that
  returns NULL for consumed events. `objc2-core-graphics` is already a
  dependency, so nothing new is added.

Not attempted here on purpose: this is the code path that can swallow the
user's keystrokes, and getting it wrong leaves the keyboard misbehaving. It
wants its own session with the daemon running in observe mode first.

### 3. Hold+twist firmware slots stay unbound

`bind-slots` binds three slots per layer (CCW / Press / CW) to
`ctrl-alt-F16..F18`. The hold+twist key IDs were never verified against the
vendor app, so they are left alone rather than guessed at -- a wrong ID writes
a binding the firmware then reports back as something else.

Progress 2026-09-07: `antiknob read-slots --wide` dumps decode further than
before. Records have the shape

    03 FA <key_id> <layer> <kind> .. .. .. .. <code>

with `kind` 01 = keyboard (mods at byte 11, code at byte 12), 02 = media (code
at byte 9, e.g. `E9` = volume up, `B5` = next track, matching `protocol.rs`),
03 = mouse. Group `0x0F` holds the three bound slots; `0x19` and the low
groups hold records whose key IDs (0x09, 0x0E, 0x13...) do not match the
`key_id_for_knob` scheme.

Still blocked on the same thing: a device whose hold+twist gestures have been
configured **by the vendor app**, to diff against. Without that reference the
dump shows what is there, not which byte means hold+twist.

### 4. Bluetooth transport is inferred, and cannot be confirmed here

`device::classify_device` decides a device is ours, and over which transport,
from its VID/PID, its bus type and its product string. Only USB and the 2.4GHz
receiver have ever been exercised against real hardware.

Progress 2026-09-07: the decision was pulled out of `list_devices_on` into a
pure function and pinned by 7 tests, which is the only way the Bluetooth
branches can be exercised at all without the hardware. That extraction found a
real defect: the old inline code raised its `is_bt` flag from the PRODUCT
STRING as well as the bus, so an Anticater with an unlisted PID plugged into
USB was classified as Bluetooth -- and since transport drives the power
readout, the UI would have called a bus-powered device battery-powered. Fixed:
the bus is ground truth, the name only decides whether the device is ours.

What remains needs the hardware: no Anticater has been paired over Bluetooth
here, so the branch is pinned by construction and unconfirmed in life. The
power description it drives (`Bluetooth Wireless (Battery Powered)`) is
likewise unverified, and no vendor battery-level query has been found in the
protocol -- over USB the device reports bus power and nothing else.
