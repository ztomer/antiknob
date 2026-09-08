# Antiknob Plan (forward-looking)

Shipped work lives in git history and in the release notes on each tag; this
file carries only what is still open. `git log --oneline` and
`git show <tag>` are the record of what landed when. The protocol itself is
in [VENDOR_UI_MAP.md](VENDOR_UI_MAP.md), which is measurement rather than
plan and does not get pruned.

## Context

* Hardware: Anticater VK01 knob, VID `0x514C` (LQKJ), PID `0x8850`
  (serial `EB60121120051103`); vendor usage page `0xFF00`, report `0x03`.
  2.4GHz receivers and Bluetooth are detected too (see `SUPPORTED_DEVICES`).
* Zero-sudo IOHIDManager access. MIT OR Apache-2.0, native arm64, macOS 26+.
* **The knob is five gestures at keys 2-6** -- CCW, press, CW, hold+twist
  left, hold+twist right -- and this device has at most ONE button, at key 1.
  Measured with `antiknob probe-gestures --map`; see `key_id_for_knob`.
* Architecture: the GUI and CLI stay TCC-free (device flashing, presets, LED,
  YAML). The daemon alone needs Accessibility / Input Monitoring for host-side
  translation. `host.json` is the contract between them, and `set_config`
  keeps five rotated backups (`host.json.1..5`) because a bad writer used to
  take the file with it.
* **The backlight follows the ACTIVE HOST LAYER**, and that is something the
  daemon does rather than something the firmware offers: the firmware stores
  one mode per DEVICE layer and has never heard of host layers, so
  `host::led_sync` writes the active layer's mode to the bound device layer
  on every switch, off the caller's thread. A layer with no mode leaves the
  light alone.
* All hidapi work re-enumerates the bus before every job. `HidApi::new`
  snapshots it once, so a daemon that never refreshed answered from that
  snapshot forever -- one replug and every open failed against a dead device
  path while `get_status` still reported "connected".
* The settings app is four tabs: General, Layers, Hardware, Services. Layers
  live behind one tab because the strip is in the titlebar and has a fixed
  budget; lighting lives inside a layer because it is per host layer.
* Every command is declared once in `src/api/registry.rs`; the CLI's clap tree
  and the MCP tool list are both generated from it. A command absent from a
  surface states why, in the table.
* All hidapi calls are marshalled onto one dedicated thread by
  `device::with_hid` / `device::with_device`; nothing outside `src/device/`
  may reach the `hidapi` crate. See `tests/hid_thread_affinity.rs` for why.
* The Swift app is an SPM package (`ui/Package.swift`): `AntiknobUI` holds the
  views, models and store, with `Core/` the half that has no view
  declarations and carries its own coverage floor; `Antiknob` holds only
  `@main`, because `@main` cannot live in a library.

## Open

### 1. Coverage, where it is worth measuring

Two floors, both the measured figure with the decimal shaved so they can only
be met by keeping tests:

| floor | value | measured | what it covers |
|-------|-------|----------|----------------|
| `GOH_COV_FLOOR_RUST` | 61 | 61.45% | the Rust crate |
| `GOH_SWIFT_COV_MIN` | 7 | 7.53% | the whole Swift package |
| `coverage-floors.json` | 54 | 54.84% | `Sources/AntiknobUI/Core/` |

The package figure is low because ~6,400 of its ~6,900 lines are SwiftUI view
bodies no unit test executes. `Core/` is the half with no view declarations at
all, and its floor is the one that means something.

`ConfigStore` has a seam now: `ConfigStore(inMemory:)` builds a store that
loads nothing, polls nothing and writes nothing. That was overdue for a
reason worse than coverage -- the plain `init()` loads the real `host.json`
and pushes every `cfg` assignment back to the running daemon, so a test that
built one and assigned a fixture EDITED THE USER'S CONFIGURATION. One did,
and destroyed a working two-layer setup plus two restores of it.

Still worth doing: **fake the socket.** The seam above covers construction
and the config, but `ConfigStore`'s one-shot RPCs still reach
`SocketClient.shared` directly, so nothing exercises their success and
failure paths. That is the next seam.

`ui/.swiftlint-baseline.json` is the companion ratchet: 5 entries, shrink-only
-- a new violation fails and so does a listed one that GROWS, because the
match key includes the measured count. Re-record only after verifying zero new
debt, and DELETE entries whose violations stop firing rather than re-listing
them.

### 2. Three layers, the third virtual and daemon-driven

The firmware's three layers and the host's three are different things that
share a word. The firmware's are fixed keymaps; the host's are what the daemon
does with slot chords.

**Decided: the virtual layer rides whichever device layer is bound to slot
chords.** Media and Navigate stay hardware layers with their no-daemon
property intact. No device-layer switching is attempted, because this knob has
no spare button and a calibrated 512-query sweep found nothing that reports or
sets the active layer -- only `FA B0 <layer>` (the LED mode) and the slot
table answer.

Built: `host.json` carries `boundDeviceLayer`, `bind-slots` records the layer
it flashed, `host::device_binding` turns that into a sentence, and
`get_knob_mode` / `get_virtual_layer` / the GUI all state it -- including the
unbound case, where a virtual layer is configured perfectly and fires never.

Still to do:

* **Flash the slot bindings on hardware.** Everything downstream of the flash
  is now built and, where it could be, verified: `bind-slots` writes all five
  chords (`ctrl-alt-F16..F20`, keys 2-6 on this device), the daemon drives the
  backlight from the active host layer, and the settings app reports the
  state honestly. What has never been run is the flash itself, because it
  converts the knob out of standalone mode -- a change to a working device
  rather than a test. Until it runs, the host layers do nothing and the app
  says so.

      antiknob bind-slots --dry-run    # shows the plan, touches nothing
      antiknob bind-slots              # the real thing

  The per-layer backlight was verified end to end WITHOUT flashing, by
  recording a bound layer in `host.json` and switching layers over the
  socket: red, green, red on the knob, matching the layers. That proves
  `led_sync`; it does not prove the flash.
* **Decide what the virtual layer should DO.** The mechanism exists and the
  frontmost-app driver works; nothing has been designed for it to mean.

### 3. Bluetooth transport is inferred, and cannot be confirmed here

`device::classify_device` decides a device is ours, and over which transport,
from its VID/PID, its bus type and its product string. Only USB and the 2.4GHz
receiver have ever been exercised against real hardware.

The decision is a pure function pinned by 7 tests, which is the only way the
Bluetooth branches can be exercised without the hardware. That extraction
found a real defect: the old inline code raised its `is_bt` flag from the
PRODUCT STRING as well as the bus, so an Anticater with an unlisted PID
plugged into USB was classified as Bluetooth -- and since transport drives the
power readout, the UI would have called a bus-powered device battery-powered.
Fixed: the bus is ground truth, the name only decides whether the device is
ours.

What remains needs the hardware: no Anticater has been paired over Bluetooth
here, so the branch is pinned by construction and unconfirmed in life. The
power description it drives (`Bluetooth Wireless (Battery Powered)`) is
likewise unverified, and no vendor battery-level query has been found -- over
USB the device reports bus power and nothing else. See
[BATTERY_RESEARCH.md](BATTERY_RESEARCH.md).

### 4. Protocol corners still unmeasured

From [VENDOR_UI_MAP.md](VENDOR_UI_MAP.md), in rough order of value:

* **Mouse under `0xFD`.** Its four bytes do not sit where the `0xFE` layout
  puts them -- button at byte 12, wheel at byte 21 -- and only the buttons
  (1/2/4) and wheel (`01`/`ff`) are measured. `fd::build_packet` refuses mouse
  rather than guessing the rest, so a mouse action cannot be part of a
  sequence.
* **Swipes.** `Left/Right/Up/Down Swipe` never assign to a knob gesture in the
  vendor UI, and neither do the `Ctrl+Alt+` combo buttons, while single
  modifiers in the same panel do.
* **Byte 5.** Device-owned: it reads back the same whatever is written there,
  and was `01` on every slot at session start and `00` afterwards. Nothing
  depends on it.
* **LED mode 3 ("ripple").** The one mode whose behaviour is still taken from
  kriomant/ch57x-keyboard-tool#173 rather than watched on this knob. The
  other five have been seen.
* **`upload --verify` still walks slot by slot.** The burst read
  (`read_full_table`) is three queries instead of eighteen and strictly more
  complete, but swapping the verification gate onto new code is a deliberate
  change, not a drive-by one.
