# Antiknob Plan (forward-looking)

Shipped work lives in git history and in the release notes on each tag; this
file carries only what is still open. The previous version of this file,
with the narrative of how each open item got where it is, is
`git show 061e1b0:PLAN.md`. The protocol itself is in
[VENDOR_UI_MAP.md](VENDOR_UI_MAP.md), which is measurement rather than plan
and does not get pruned.

Reference docs (not plans, not pruned): `VENDOR_UI_MAP.md` (protocol
measurements), `REFERENCES.md` (where facts come from), `FIRMWARE_REVIEW.md`
(2026-09-11 wire audit, all eight findings resolved the same day; its one
residual question is item H.4), `BATTERY_RESEARCH.md` (research; its open
questions are item D.4), `FINDINGS.md` (historical feasibility study, marked
as such at its top).

## Context

* Hardware: Anticater VK01 knob, VID `0x514C` (LQKJ), PID `0x8850`
  (serial `EB60121120051103`); vendor usage page `0xFF00`, report `0x03`.
  2.4GHz receivers (`0x25A7:0xFA11`) and Bluetooth are detected too (see
  `SUPPORTED_DEVICES`). Device writes need the USB-C wired link; `antiknob
  raw` refuses over the receiver.
* Zero-sudo IOHIDManager access. MIT OR Apache-2.0, native arm64, macOS 26+.
* **The knob is five gestures at keys 2-6** -- CCW, press, CW, hold+twist
  left, hold+twist right -- and this device has at most ONE button, at key 1.
  Measured with `antiknob probe-gestures --map`; see `key_id_for_knob`.
* **Slot chords are `ctrl-alt-shift-F16..F20`**, written by `bind-slots` as
  `0xFD` chains (one entry per modifier, then the keycode). Flashed on all
  three device layers and working on hardware since 2026-09-08.
* Architecture: the GUI and CLI stay TCC-free (device flashing, presets, LED,
  YAML). The daemon alone needs Accessibility / Input Monitoring for host-side
  translation. `host.json` is the contract between them, and `set_config`
  keeps five rotated backups (`host.json.1..5`).
* **The backlight follows the ACTIVE HOST LAYER**, done by the daemon
  (`host::led_sync`), not offered by the firmware, which stores one mode per
  DEVICE layer. A layer with no mode leaves the light alone.
* **An `FA` read-back proves the device STORED bytes, never that it ACTS on
  them**, and an instrument that cannot fail is worse than none. Before
  believing a negative result, make the instrument produce a positive one
  (type a key, write a mode you can see). Anything a command PRINTS about
  what it did must be derived from what it did.
* All hidapi work re-enumerates the bus before every job, and all hidapi
  calls are marshalled onto one thread by `device::with_hid` /
  `device::with_device`; nothing outside `src/device/` may reach `hidapi`
  (`tests/hid_thread_affinity.rs`).
* Every command is declared once in `src/api/registry.rs`; the CLI's clap
  tree and the MCP tool list are both generated from it.
* The Swift app is an SPM package (`ui/Package.swift`): `AntiknobUI` holds
  views, models and store, with `Core/` the half with no view declarations
  and its own coverage floor; `Antiknob` holds only `@main`. Settings app is
  four tabs: General, Layers, Hardware, Services.
* Coverage floors live in `.gatesrc` (`GOH_COV_FLOOR_RUST`), `ui/.gatesrc`
  (`GOH_SWIFT_COV_MIN`) and `ui/coverage-floors.json` (`AntiknobUI/Core`).
  They are ratchets set just under the measured figure; this file does not
  restate the numbers. `ConfigStore(inMemory:)` is the store's test seam --
  the plain `init()` loads and WRITES the user's real `host.json`, and a test
  that used it once destroyed a working configuration.

## Rulings

* **The virtual layer rides whichever device layer is bound to slot
  chords.** Media and Navigate stay hardware layers with their no-daemon
  property. No device-layer switching: the knob has no spare button and a
  calibrated 512-query sweep found nothing that reports or sets the active
  layer.
* Device writes (flash, LED) on the owner's knob need the owner's go-ahead
  in the session; approval is per session (granted 2026-09-07 for one).

## Phases

Size: S = under one session, M = one session, L = several.

### Phase 0 -- wire in goh 0.27.0 -- closed 2026-10-14

`tools/gate.sh --full` green at `6cc8c15`, pushed through the pre-push gate.

* **W.1** RUSTSEC-2024-0449: YAML on saphyr 0.2.1 via `src/yaml.rs`, `cargo tree -i unsafe-libyaml` empty: `948466b`
* **W.2** Scanner floors (12 keys, calibrated red): `6cc8c15`
* **W.3** Push (`a883863`..`6cc8c15`): `6cc8c15`

### Phase 0b and 1 -- defects, stale claims, house-style and test seams -- closed 2026-10-24

Software only. `tools/gate.sh --full` green at the push of this commit.

* **K.1** `get_knob_mode` read the dead `0xFE` layout (mods/code at bytes
  11/12), so a knob flashed by `bind-slots` (`0xFD` chains) classified as
  Standalone. `fd::decode_chain` / `chord_of` are the one reader of the
  stored layout, the classifier matches `host::bind::slot_chords()` (derived
  from `SLOT_SPECS`), `SLOT_CHORD_MODS` and `SLOT_KEYCODES` are deleted, and
  the fixtures are the flasher's own packets re-framed as `FA` replies. Red
  first (`what_bind_slots_writes_reads_as_host_translate` returned
  Standalone). Sibling of the class: `upload` derived its pre-write wipes
  through `parse_record`, which skips `0xFD`, so no keyboard or media slot was
  ever wiped; `fd::wipes_for` fixes it (red first). `3faf599`
* **S.4** `upload --verify` and `get_knob_mode` read `read_full_table`;
  `tests/slot_read_burst.rs`. `783541b`
* **K.2, K.3** stale floor comments, the media-chaining bullet, the pre-shift
  chord comments. `4ccc1fa`
* **K.4** lockfile refreshed (12 packages); `cargo tree -d` unchanged;
  `unsafe-libyaml` stays out of the lock (W.1). `fb7d862`
* **S.1** 113 hand-rolled markers onto `goh-tui` / `tui/lib.sh`, no seed,
  `goh status-markers` in `tools/repo_gates.sh` (calibrated red), CI links
  `../gates_of_heck` to `./goh`. `0956d2e`
* **S.1 follow-up, workaround removed** -- owner's words (umbrella section 0b):
  "the export's missing `gates_of_heck` sibling for consumers with a `goh-tui`
  path dependency (antiknob's workaround)". goh `fb3d0a7` makes the pre-push
  export link the sibling, so the `ln -s` block in `tools/gate.sh` (`801ef17`) is
  deleted; the push gate runs green without it and no longer names a write
  outside its tree. CI is not that export, so its own link step is back in
  `tests.yml`.
* **S.6** CI on Python 3.15. `d1bfb28`
* **S.7** `GOH_RUST_LINT_CONFIGS='--target aarch64-apple-darwin:--release'`
  (calibrated red through a `cfg(not(debug_assertions))` error). `4e02fad`
* **S.2** `DaemonRPC` seam, `OfflineRPC` default, `FakeDaemonRPC`, tests for
  every RPC's success and failure. `aafa748`
* **S.3** the five baseline entries fixed and the baseline deleted; floors
  raised to the measurement (package 16, Core 63). `6d799c0`

### Phase 1b -- sibling readers (software only) -- size S

* **S.8 Per-slot walks left in the hardware tooling.** `bind-seq`'s
  read-back (`binding.rs`) and the `probe-gestures` family (`probe.rs`, five
  call sites) still call `read_slot_table` at a layout width, which cannot see
  a key past it; `read_full_table` can. They write to the device, so the
  change is exercised in the H.x session. *Acceptance:* no `read_slot_table`
  caller outside `read-slots` (the exploratory walk), proven on the knob.

### Phase 2 -- one wired hardware session -- size M, needs the knob on USB-C

Goal: every question that only the device can answer, batched into one
sitting. Exit: each item below has a recorded measurement in VENDOR_UI_MAP
(positive control included) or a stated reason it could not be taken.

* **H.1 Confirm K.1 on the knob:** after `bind-slots`, `get_knob_mode`
  (MCP / socket) and the GUI report host-translate. Also confirm the
  `upload` wipes (`fd::wipes_for`, new for keyboard and media slots): flash a
  long sequence, then a short one over it, and read the slot back.
* **H.2 Do the chords need shift?** They moved to `ctrl-alt-shift` when
  `ctrl-alt-F18..F20` seemed to collide with another app; the collision was
  probably the since-fixed modifier-inheritance bug. One flash of
  `ctrl-alt-F16..F20` settles it. *Acceptance:* daemon FIRED lines for all
  five gestures and no other app reacting, then a ruling recorded here.
  Changing the chord touches `SLOT_SPECS`, `host::gesture::SLOT_MODS` and
  their pinning tests together.
* **H.3 Fresh wired capture** with `tools/hidsnoop/snoop-vendor-app.sh`,
  frames added to `tests/fixtures/vendor_capture.json` (no invented frames).
  Targets, by value: mouse under `0xFD` (only buttons at byte 12 and wheel at
  byte 21 measured; `fd::build_packet` refuses mouse until then, so no mouse
  step in a sequence), byte 5 of a chained record (factory frame shows `01`
  where we write `00`), why swipes and the `Ctrl+Alt+` combo buttons never
  assign in the vendor UI.
* **H.4 LED:** watch mode 3 ("ripple", still taken from
  kriomant/ch57x-keyboard-tool#173) and whether a mode stored on device
  layers 3-15 ever renders (FIRMWARE_REVIEW #7; storage measured, render
  not). If they never render, cap `SET_LED_LAYER_MAX` at the firmware's
  three layers.

### Phase 3 -- design and hardware we do not have -- parked

* **D.1 What the virtual layer should DO.** The mechanism and the
  frontmost-app driver work; nothing has been designed for it to mean.
  Blocked on an owner design decision; size M once decided.
* **D.2 Bluetooth transport, unconfirmed in life.** `classify_device` is a
  pure function pinned by 7 tests; no Anticater has been paired over BT
  here, so the BT branch and its power text are unverified and no battery
  query is known. *Unblock:* pair the knob over BT and run `antiknob status
  --json`.
* **D.3 The 0x1189 CH57x family.** The gesture-to-key-id mapping is applied
  to every VID/PID in `SUPPORTED_DEVICES`, but no CH57x capture exists.
  *Unblock:* a CH57x device on the bench, or narrow `SUPPORTED_DEVICES` to
  what has been measured -- an owner call.
* **D.4 Battery questions** (BATTERY_RESEARCH.md section 6): sleep current
  and delay per mode, whether wired mode powers down RF, whether the vendor
  `DelaySetting` persists and is worth exposing. Needs a USB power meter;
  only DelaySetting has a software deliverable.

## Sources consolidated (2026-10-12)

* `PLAN.md` @ 061e1b0 -- rephased into this file; narrative pruned to
  `git show 061e1b0:PLAN.md`. Open sections 1-4 carried as S.2, S.3, S.4,
  H.2, H.3, H.4, D.1, D.2; "Next" (0x1189 mapping, byte 5) carried as D.3,
  H.3.
* `.opencode/opencode-loop/ses_f86117f01ffeYDpXlEyY2Eelpy` (goal "implement
  the plan", 2026-09-07, blocked on the owner) -- superseded: every owner
  step it waited on landed (device writes approved and flashed 2026-09-07,
  twist-to-action verified on hardware 2026-09-08, installs/releases since
  through v0.15.1). Nothing carried.
* `.opencode/opencode-loop/ses_f773ebc95ffef48NGRrbXqMEfm` (goal "implement
  and verify the fix", 2026-09-10, completed) -- superseded: the Flash Knob
  fix is in the tree (`resolve_bind_layers` in `src/api/device_cmds.rs`).
* `.opencode/opencode-loop/ses_f8600a124ffewjUvyduoCc0ynY` -- empty job
  list; nothing to carry.
* `FIRMWARE_REVIEW.md` -- reference; findings 1-8 verified resolved in code
  (`superseded()` in `led_sync.rs`, `send_report` refuses oversize,
  `absurd_counts_refuse_rather_than_wrap`); #7's render question -> H.4.
* `BATTERY_RESEARCH.md` section 6 -> D.4. `VENDOR_UI_MAP.md` "Still open"
  -> H.3, H.4, K.3.
* Claude CLI memory, Claude desktop sessions, Antigravity (agent and IDE),
  Gemini CLI, opencode db: no antiknob entries (the only grep hits were an
  `/Applications` listing that names `ANTICATER.app`, not this project).
  `ROADMAP-consolidated.md` and `RUSTIFICATION_ROADMAP.md` do not mention
  antiknob.
* git: one branch (`main`), one worktree, no stash.
* gates_of_heck v0.27.0 Downstream (2026-10-14): hooks reinstalled
  (`f90ce18`); audit -> W.1; floors -> W.2; status-markers and `goh-tui`
  -> S.1; tap_bump -> S.5 (closed 2026-10-17, `4e6092b`); Python 3.15 -> S.6.
