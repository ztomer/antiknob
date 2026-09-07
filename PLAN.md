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

### 1. Swift coverage is a ratchet, not a bar

The Swift half is now under the house gate (`ui/` is an SPM package; see
`ui/.gatesrc`), but the coverage floor it enforces is 4%, which is the honest
measured figure rather than a quality bar. Of ~6,900 lines, ~6,000 are SwiftUI
view bodies no unit test executes; `Models.swift` -- the part with testable
logic -- sits at 64% and everything else at 0%.

Two ways to make the number mean something, in order of value:

* Split the package into a logic target and a views target so the floor can
  apply where coverage is meaningful. `StatusPresentation` and `ledModeNames`
  were already pulled out of `ConfigStore` for exactly this reason and are the
  start of that target.
* Put a seam under `SocketClient` (247 lines, 0%) so its JSON-RPC framing and
  error paths can be tested without a live daemon.

`ui/.swiftlint-baseline.json` is the companion ratchet: 11 entries, all
`cyclomatic_complexity` / `function_body_length` / `type_body_length` on view
bodies and the two exhaustive `Action` coding switches. It is shrink-only --
new violations fail, and a listed one that grows fails too, because the match
key includes the count in the reason string. Delete entries as the bodies get
split; do not re-record it to make a failure go away. It has been re-recorded
once, when splitting `LayerDetail.swift` moved three entries to a new file and
eliminated a fourth; that was verified as a pure move (zero new debt, ignoring
which file each violation lived in) before re-recording, which is the only
form of re-record the ratchet permits.

### 2. `rdev` drags in a crate a future rustc will reject

`cargo build` warns: `block v0.1.6` "contains code that will be rejected by a
future version of Rust". The chain is `antiknob -> rdev 0.5.3 -> cocoa 0.22 ->
block 0.1.6`. `rdev` is the input-tap engine, so this is not a version bump --
it is either an upstream fix, a fork, or replacing the tap with `objc2`
+ `CGEventTap` directly (the crate is already a dependency). Standing rule:
never build on APIs that are on a removal path. Track the warning with
`cargo report future-incompatibilities`.

### 3. No coverage floor on the Rust side

`tools/gate.sh --full` prints `no coverage floor -- set GOH_COV_FLOOR_RUST in
.gatesrc`. Measure the current figure first, set the floor just under it, then
ratchet; do not pick a round number and grind toward it.

### 4. Hold+twist firmware slots stay unbound

`bind-slots` binds three slots per layer (CCW / Press / CW) to
`ctrl-alt-F16..F18`. The hold+twist key IDs were never verified against the
vendor app, so they are deliberately left alone rather than guessed at --
a wrong ID writes a binding the firmware then reports back as something else.
Recover them with `antiknob read-slots --wide` against a vendor-app-configured
device, then extend `host::bind::binding_packets`.

### 5. Bluetooth transport detection is inferred, not confirmed

`device::list_devices` classifies a device as Bluetooth from
`dev.bus_type() == Bluetooth` or a product string containing
`anticater`/`vk01`/`vk-01`. That path has never run against real Bluetooth
hardware -- only USB and the 2.4GHz receiver are verified. The power
description it drives (`Bluetooth Wireless (Battery Powered)`) is likewise
unverified, and no vendor battery-level query has been found in the protocol
(the device reports bus power over USB). Probe before trusting either.
