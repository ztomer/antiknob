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

## Open

### 1. The Swift half of the repo has no gates at all

`tools/gate.sh --full` covers Rust thoroughly -- fmt, clippy, manifest lints,
no-`#[allow]`, tests, `cargo audit` -- and covers the ~2,900 lines under `ui/`
with nothing. No lint, no tests, no coverage floor, no file-length enforcement
beyond the structural gate. The suite still prints all-green, because it never
claimed to cover that half.

The blocker is that `ui/` is built by a hand-rolled `swiftc` invocation in
`ui/build.sh` with no SPM package or Xcode project, and `swift_gate.sh` needs
one (`GOH_SWIFT_MODE=xcode|spm`). Wrapping `ui/` in a `Package.swift` with a
test target is the real task; wiring the gate is the easy half that follows.
Until then, every Swift regression is caught by eye or not at all.

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
