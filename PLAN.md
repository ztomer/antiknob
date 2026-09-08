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
* The keyboard tap is a `CGEventTap` driven directly (`keytap.rs` in the
  daemon). It replaced `rdev`, whose `cocoa`/`block` chain carried a lint that
  a future rustc turns into a hard error. Modifiers arrive as `FlagsChanged`
  with no up/down bit; `keytap::decode` derives it from the device-dependent
  NX flag bits, so left and right are distinguished.
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
(`GOH_COV_FLOOR_RUST`, measured 61.92%) and Swift 4 (`GOH_SWIFT_COV_MIN`,
measured 4.8% over SOURCES -- the house checker stopped counting `Tests/` on
2026-09-07, because a test file is ~100% covered by definition and a floor set
on that can be met by tests that assert nothing).

The Swift number is low because ~6,000 of its ~7,300 lines are SwiftUI view
bodies no unit test executes. The logic that has a seam scores far higher --
`Models.swift` 64%, `SocketWire` near total. Two ways to make the number mean
more, in order of value:

* ~~Split the package into a logic target and a views target~~ **Done
  2026-09-07, and not by splitting the package.** The floor that matters is
  now on `Sources/AntiknobUI/Core/` at **46%**, measured 46.58%, beside the
  package floor of 4%.

  The old note said this was blocked on routing `swift_gate.sh` through
  `coverage_gate.sh`, which supports `--floors-json`. Measured, that reroute
  was the wrong move: `coverage_gate.sh` in Swift mode could not measure this
  package AT ALL -- it handed llvm-cov the `.dSYM` directory instead of the
  executable -- and once that was fixed it read 10.54% against the working
  checker's 4.78%, because it counted `Tests/` toward the floor. Rerouting
  would have raised every repo's measured Swift coverage and made every floor
  easier to pass. Both defects are fixed upstream (gates_of_heck `9ef8f1c`)
  and the scope rule now lives in one module both paths import.

  `check_swift_coverage.py` gained the floors instead (`1a13651`), with the
  key generalised from a target NAME to a path PREFIX. That is what makes a
  separate target unnecessary: `Core/` is a directory inside `AntiknobUI`, so
  the views and the types they are built from stay in one module and nothing
  gains a `public` annotation it would not otherwise have -- the property
  `Package.swift` deliberately records. Letting a coverage tool dictate a
  package's module structure is the wrong way round.

  `Core/` holds the six sources with no view declarations at all: the models,
  the socket client, the two parsers, the gesture enum and the presentation
  values. That boundary scores 46.58% where the views score 0.2% over 6,606
  lines, which is the whole reason the package number could never mean much.
* Keep putting seams under the logic that has none. `ConfigStore` is still
  the largest -- its `init()` loads config and starts a 2s poll timer, so it
  cannot be constructed in a test as it stands -- but the seam does not have
  to run through the store. Two decisions it made inline are now pure and
  covered: `CliDiscovery` (where the binary is, and what its JSON means) and
  `StatusParse` (what a status payload claims about the hardware). The second
  was written out TWICE in `refreshStatus`, once per source, differing only
  in the tap fields; one parser now serves both. That moved the measured
  figure 3.4% -> 4.8% and took the lint baseline from 8 entries to 5, because
  `ConfigStore` dropped under the type-body cap it had been over.

  The store's own remaining methods are one-shot RPCs that need the socket
  faked to test; that is the next seam, and it is a bigger one.

`ui/.swiftlint-baseline.json` is the companion ratchet: 5 entries, all
body-length and complexity on view bodies and the two exhaustive `Action`
coding switches. Shrink-only -- a new violation fails and so does a listed one
that GROWS, because the match key includes the measured count. It has been
re-recorded four times, every time verified file-agnostically as zero new
debt first, which is the only form of re-record it permits. The three
`ConfigStore` entries were deleted rather than re-listed: their violations
stopped firing, and a baseline entry that no longer bites is exactly the
rot the ratchet exists to prevent. The fourth re-record was a pure path
change -- the two `Models.swift` entries moved into `Core/` -- which the
ratchet correctly reported as new, because its match key is file plus rule
plus reason.

### 2. Knob slot mapping, and what is left of hold+twist

Resolved 2026-09-07 against real hardware (VK01 `514c:8850`, 3 buttons +
1 knob). Two defects made every knob binding this tool ever flashed a no-op,
and neither reported a failure:

1. `key_id_for_knob` hardcoded `BASE = 16`. Knob slots follow the buttons in
   the 1-based key-ID space, so a 3-button device reads its knob from 4/5/6
   and 16 is right only for a 15-key layout. Bindings went to 16/17/18, which
   the firmware stores and never reads. Now derived: `button_count + 1 +
   knob_index * 3 + offset`.
2. `Action::to_packet` wrote media usages to bytes 11/12 -- where *keyboard*
   actions put mods and keycode. The device stores media at byte 9. Keyboard
   and mouse bindings worked; every `volumeup`/`mute`/`play` was inert.

The notes below already recorded byte 9 as the media offset "matching
`protocol.rs`", and already recorded that the observed key IDs did not match
the `key_id_for_knob` scheme. Both were true observations filed as curiosities
rather than read as defects. A dump is not a diff: writing down what the
device holds does not check it against what the code sends.

`upload` now reads the slot table back and compares (`device::verify`), so a
write that lands nowhere reports as unconfirmed instead of "successfully
written". That check found defect 2 on its first run by isolating the failure
to media-only layers. Confirmed 17/18 slots after both fixes; the knob went
from emitting `E9` for every gesture to `EA` / `E9` / `E2` for CCW / CW /
press.

Record shape, request and reply alike (`FE` = write, `FA` = read):

    03 FE|FA <key_id> <layer> <kind> .. .. .. .. <media> .. <mods> <code>

`kind` 01 = keyboard (count at byte 10, mods 11, code 12), 02 = media
(16-bit usage LE at bytes 9-10), 03 = mouse. Layer is 1-based on the wire.
Groups `0x01..=0x24` x counters 1-3 return the whole table
(`device::slot_table_addresses`); everything else is silent.

**Hold+twist is real, and this repo said otherwise.** Corrected 2026-09-07
after capturing the vendor app binding it on this hardware.

The false claim came from two pieces of evidence that looked consistent and
proved nothing: the reference tool's `KnobAction` enum has three variants
(that is ITS model of the device, not the firmware's capability), and
`probe-gestures` wrote markers to slots 7/8 and saw neither fire (which rules
out those two slots, nothing more). A vendor screenshot showing five gesture
zones around the knob was on screen at the time and went unread. The claim
reached code, tests, the GUI, README, REFERENCES and the v0.9.0 tag before
the user corrected it.

The reason it was invisible: **the vendor uses a different command.** Writes
go out as `0xFD`, not the `0xFE` this build uses. The `0xFD` writer is built
and confirmed against the hardware (`src/fd.rs`, `antiknob bind-seq`); the
record layout is:

    03 FD <key_id> <layer> <kind> <n_groups> <group_kind> <group> <group> ...

    group = 00 <mods | usage_high> <keycode | usage_low>

`kind` and `group_kind` are both `01` for keyboard and `02` for media, layer
is 1-based on the wire, and the commit is `03 FD FE FF`. `n_groups` is why
this command matters: **one gesture can run a SEQUENCE of actions**, which
`0xFE` has no field for. Written and read back on real hardware for a single
chord, a media usage, and two- and three-action sequences; every write echoed
byte for byte.

**The capture notes this replaces were wrong twice, and both errors were the
same mistake as before -- a dump read as a diff.** The recorded frame put
`<n_mods>` before `<n_groups>`, which is one byte out; and the recorded key
map (2..6 for the five knob gestures) does not describe this device. Writing
`03 FD 02 ...` changes the slot the `FA` read reports as key 2, which on this
3-button layout is the `prev` BUTTON. There is only ONE slot table: `0xFD`
and `0xFE` address the same keys, 1-3 buttons and 4-6 knob. Nobody checked
the captured key numbers against the slot map this repo had already measured.

That single table is the useful part: a `0xFD` write is visible to the
ordinary `FA` read, so `bind-seq` confirms every write by reading it back and
comparing the binding bytes, not just the address.

`0xFE` **stays.** It is verified for keyboard, media and mouse, the firmware
fills in `n_groups`/`group_kind` for it, and `0xFD` has no measured mouse
encoding -- which `fd::build_packet` refuses rather than guesses. `0xFD` is
the path for anything needing a sequence.

Two things learned while reading the table that the notes had wrong:

* **Slots 7 and up are not evidence of hold+twist.** They exist and answer a
  read, but so do 8..12, and every one of them holds the same generic factory
  placeholder (`key N -> the Nth letter`, `0a` through `0f`). That is a table
  sized for a larger family member, not a gesture map.
* **The slot table is walked, not addressed.** A lone query for counter 7
  never answers; the same query as step 7 of a walk from counter 1 answers
  every time, and the state is per-handle -- a counter-1 read in another
  process does not unlock it.

  An earlier note here said this voided the 512-query sweep's negative
  results. It does not, and that was an overclaim recorded on 2026-09-07 and
  corrected the same day. The sweep queried `FA <cmd> 00 00` and
  `FA <cmd> 00 01`, and counter 1 DOES answer standalone -- verified against
  the hardware for widths `01`, `06`, `0f`, `19` and `24`, and `FA B0`
  answers standalone too. So the sweep could see both families that are
  known to answer: it was calibrated in fact, not merely in claim.

  What the walk finding narrows is the sweep's blindness, not its validity.
  On top of the limits already recorded -- an argument outside bytes 3-4, a
  prefix other than `FA` -- it is also blind to a command that answers only
  as step N>1 of an in-process walk. Re-running it in the standalone form
  would find nothing new.

Still to do: **find out which slot hold+twist actually drives.** It cannot be
settled from the desk -- it needs the gesture performed while a distinct
marker sits in each candidate slot. That is now one command:

    antiknob probe-gestures

It arms slots 7-11, captures for 45s, and puts every slot back afterwards
(confirmed by read-back). The missing step is a human twisting the knob.

`probe-gestures` now runs a **positive control**: slot 4, the knob's CCW
gesture, carries a marker of its own, and a run whose control never fires
reports INCONCLUSIVE instead of listing candidates as silent. That is the
whole reason the original claim was wrong -- a negative result from an
instrument nobody had shown could produce a positive one. The old default
of `7,8` was itself the same mistake in miniature: it came from reading
"exists and looks empty" as "is an unbound gesture", when 7 through 12 all
hold the same generic factory placeholder.

See kriomant/ch57x-keyboard-tool#191 and #174.

### 3. Three layers, the third one virtual and daemon-driven

Requested 2026-09-07. Today both the firmware and the host config carry
layers, and they are separate things that happen to share a word: the
firmware's three are fixed keymaps, the host's are what the daemon
translates slot chords into. The shape wanted is:

* **Media** and **Navigate** stay hardware layers, flashed to the device and
  working with no daemon at all. User-renamable.
* **Layer 3 is virtual** -- it has no fixed keymap. The daemon owns it, and
  its bindings hot-swap from a user-defined set. The obvious driver is the
  frontmost application, so the knob means one thing in a browser and
  another in an editor, but the mechanism should not be hard-wired to that.
* **The MCP server exposes the swap**, so an agent can set or define the
  active virtual layer the same way a user can.

LED identity per layer: **by effect, not colour.** This knob has a single
fixed colour -- mode 1 was set with blue, red and green in turn and stayed
red every time -- so "breathing red / breathing green / multicoloured
breathing" is not achievable as asked. The shipped layout uses
`static` / `ripple` / `rainbow`, and mode 4 (rainbow) is the multicoloured
effect the device arrives in. See item 5 for the protocol.

**Decided 2026-09-07: the virtual layer lives on the current device layer.**
Media and Navigate stay hardware layers and keep their no-daemon property;
the virtual layer is carried by whichever device layer is bound to slot
chords, which is what `bind-slots --layer N` already flashes. No device-layer
switching is attempted, because this knob cannot do it (below) -- the user
picks the layer once and that is the one the daemon drives.

Built 2026-09-07. `host.json` carries `boundDeviceLayer`, `bind-slots`
records the layer it flashed (and records NOTHING when every layer is bound,
because there is then no single answer), and `host::device_binding` turns the
record into something a person can read. `get_knob_mode` and
`get_virtual_layer` both report it, and the GUI's reachability notice states
it -- including when nothing is bound, which is the failure this closes:
a virtual layer configured on an unbound device is perfect and fires never,
and the only previous symptom was a knob that did nothing.

An out-of-range recorded layer is reported, not clamped, and an unparseable
`host.json` is refused rather than replaced with defaults carrying one new
field.

**Not verified against the hardware.** Running `bind-slots` for real converts
this knob out of the standalone media mode it is in, which is a change to the
user's working device rather than a test. The pure half is covered (10 tests)
and the API shape is asserted through `execute_command`; what is unproven is
the flash-then-record sequence on real hardware.

The reasoning that was open before that decision:

* The virtual layer only fires when the knob sends slot chords, so it
  presupposes host-translate mode -- and this hardware currently cannot be
  in both modes at once (see item 2). Either the device layer carrying the
  virtual layer is bound to slot chords while the other two stay standalone
  -- which needs a way to switch device layers on a knob with no spare
  button -- or the whole device goes host-translate and Media/Navigate are
  re-implemented host-side, losing their no-daemon property.
* Layer switching on this VK01 is unsolved, and now bounded. The HOST layer
  `double_tap_switch` moves is a different thing from the device layer the
  firmware is on. A read-only sweep of 512 queries -- `FA <cmd> 00 00` and
  `FA <cmd> 00 01` for every `cmd` -- found exactly two that answer:

      FA B0 <layer>          the layer's LED mode
      FA <width> 00 <ctr>    the slot table (see item 2)

  Nothing reports the active device layer. The sweep is calibrated rather
  than merely negative: `FA B0` answers, so an answering command IS
  detectable by it. It does NOT cover a query whose argument lives outside
  bytes 3-4, or one outside the `FA` prefix. Going further means capturing
  the vendor app under Rosetta over USB -- a larger job than anything else
  here.

  The practical consequence: a knob with no spare button cannot switch device
  layers, so the virtual layer either lives on whichever layer the device is
  already on, or Media/Navigate move host-side and lose their no-daemon
  property. That is a design decision, not a discovery.

Progress 2026-09-07: the host side is built and covered. `virtual_layer`
resolves a variant from the frontmost app or an explicit pin,
`host::frontmost` is the OS seam, and `get_virtual_layer` /
`set_virtual_variant` expose it over the socket and MCP. LED identity is set
for Media (steady red) and Navigate (steady green); the third is unset on
purpose, because this firmware has no breathing mode among off / backlight /
shock / shock2 / press, and `led-probe` exists to find out whether the
unmapped mode 5 is one. `probe-gestures` makes hold+twist a one-command
question -- slots 7 and 8 exist and carry only a factory placeholder, so the
old "needs the vendor app" note was wrong.

### 4. The LED protocol, and what this knob can actually do

Resolved 2026-09-07 by capturing ANTICATER.app's HID traffic
(`tools/hidsnoop/`) and reading kriomant/ch57x-keyboard-tool#173 and #175.

Packet: `03 FE B0 <layer> <mode>`, base colour at bytes 5-7, sixteen per-key
RGB triples from byte 8. Layers are 0-based. No commit follows.

Three things this build had wrong, each of which produced a write the device
accepted and ignored:

1. **`03 FB FB FB` is required first.** Without it the firmware stores the
   mode, reads it back correctly, and changes nothing. That false read-back
   is why this took so long to find: it confirmed every write and said
   nothing about the effect. All LED writes now go through
   `device::send_led`.
2. **Mode 5 crashes the firmware.** Sending it wedged this knob's LED
   renderer until it was power-cycled -- on a model with no power switch,
   which stays lit on battery when unplugged. It is refused, in the CLI and
   in the GUI, with tests that fail if it returns.
3. **The mode table was another family's.** For a `514c:8850` it is
   `0 off, 1 static, 2 reactive, 3 ripple, 4 rainbow`. `cmds.rs` carried a
   second copy of the old table, so `led-read` kept printing the wrong names
   after the shared one was fixed.

**This knob ignores the colour bytes.** Measured with three distinct colours.
They are still sent, correctly placed, because the 16-key device sharing this
product id does honour them -- but nothing in this repo may promise colour on
the knob, which is why the GUI's swatches now carry a notice.

Known firmware bug (#175): LEDs freeze after 2s-2m and only a replug
recovers. If a mode change appears to do nothing, replug before debugging.

The lesson worth keeping: the answers were in the reference project's ISSUE
TRACKER, tested on this exact hardware, while hours went into inferring them
from a disassembly. The source was fetched early; the issues were not opened
until much later.

### 5. Bluetooth transport is inferred, and cannot be confirmed here

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
