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

### Phase 0b -- defects and stale claims (software only) -- size M

Goal: nothing the tree says about itself false. Exit: K.1 green with its
calibration recorded, `rg -n '46%|46.58|4%\)' ui/ ui/.gatesrc` empty, the
stale VENDOR_UI_MAP bullet gone.

* **K.1 `get_knob_mode` cannot report host-translate on a flashed knob
  (suspected live defect, found 2026-10-12 by reading the code).**
  `device::mode::classify` recognises a chord by `action.mods ==
  SLOT_CHORD_MODS` (`0x01|0x04`, ctrl+alt) read through `parse_record`,
  which takes mods/code from bytes 11/12 -- the dead `0xFE` layout. But
  `bind-slots` writes `ctrl-alt-shift` as a `0xFD` chain (`len 4`, entries
  `f1 f2 f3 6b` from byte 7, per `bind-slots --dry-run`), and VENDOR_UI_MAP
  records that an `FA` read returns that chain layout unchanged. So bytes
  9/11/12 read `f1/00/f2`, no record matches, and every flashed knob
  classifies as `Standalone`. The classifier's tests build their fixtures
  from `SLOT_CHORD_MODS` itself, so they are tautological.
  *Class:* readers of `FA` replies that assume the `0xFE` layout.
  *Acceptance:* a test that feeds `bind::binding_packets` output, re-framed
  as an `FA` reply, through `classify` and requires `HostTranslate`; proven
  red on today's code first. Delete `SLOT_CHORD_MODS` (it encodes the
  pre-shift chord) and derive the match from `SLOT_SPECS`. Grep every
  `parse_record` caller for the same assumption. Hardware confirmation is
  H.1. *Risk:* the GUI's "Not flashed" / mode text is driven by this, so
  users have likely been told the wrong thing since v0.12.0 (803170e,
  2026-09-08), when the chords gained shift and moved to `0xFD`.
* **K.2 Stale coverage comments.** `ui/Package.swift:23-24` says the Core floor is
  46% and the package floor 4%; `ui/.gatesrc:31` says Core measures 46.58%. The
  configured floors are 54 and 7. Correct the sentences, not append.
  *Acceptance:* the grep in the exit criterion is empty.
* **K.3 VENDOR_UI_MAP "Still open" contradicts itself.** Its "Media
  chaining" bullet says the limit is the app; the same file later measures
  that the DEVICE truncates media chains. Correct the bullet (measurement
  wins, both readings visible). Also the stale comments in
  `src/device/mode.rs` / `src/firmware.rs` calling the chords
  "ctrl-alt-F19" (fixed with K.1).
* **K.4 Lockfile hygiene.** `cargo update --dry-run` (2026-10-14): `cc`
  1.6.0 -> 1.8.0, `deranged` 0.5.8 -> 0.5.9, `equivalent` 1.0.2 -> 1.0.3,
  `serde_json` 1.0.151 -> 1.0.154, `syn` 3.0.6 -> 3.0.9, `unicode-ident`
  1.0.26 -> 1.0.27; no majors behind. Not done with W.1, which kept to
  the YAML swap.
  *Acceptance:* refreshed lock, `cargo tree -d` shows no new duplicates,
  gate green. Size S.

### Phase 1 -- house-style and test seams (software only) -- size M

Goal: the remaining house-rule gaps closed and the store's RPC paths
testable. Exit: `status-markers` gated with no seed, a socket fake in use,
the `Core/` floor raised to the new measured figure, S.6-S.7 closed.

* **S.1 Adopt `goh status-markers`, then print through `goh-tui`.**
  `$GOH_DIR/gates/goh.sh status-markers` reads 126 files and finds 113
  hand-rolled markers in 9 (2026-10-14): `src/bin/antiknob/cmds.rs` 29,
  `probe.rs` 19, `diag.rs` 18, `binding.rs` 10, `src/bin/antiknob-daemon/run.rs`
  11, `main.rs` 10, `install.sh` 9, `ui/build.sh` 6,
  `src/host/config_backup.rs` 1; 43 of them are `[ ==> ]` in `src/bin/`.
  The gate does not run it yet. Step 1: call it from `tools/repo_gates.sh`,
  fix the shell ones (`install.sh`, `ui/build.sh` source `$GOH_DIR/tui/lib.sh`)
  and `--seed` the rest. Step 2: the Rust helper EXISTS --
  `crates/goh-tui` in gates_of_heck (its BACKLOG 4.14, `a74bbff`; the
  2026-10-12 version of this item said goh had none, which was wrong). Add
  `goh-tui = { path = "../gates_of_heck/crates/goh-tui" }` (Cargo reads no
  env var in a path, so the sibling checkout IS `$GOH_DIR`), print
  `goh_tui::outln!("{OK} ...")` in place of each `[ Ok  ]`, and re-seed
  smaller. *Risk:* CI checks goh out at `./goh` inside the workspace
  (`tests.yml:65-71`), where `../gates_of_heck` does not resolve and
  `actions/checkout` will not write outside the workspace; CI needs a
  step that puts it there (a symlink) before the build, proven by CI green. *Acceptance:* the step runs in
  `tools/gate.sh --full`, the seed reaches 0 and is deleted, and a
  reintroduced `[ ==> ]` turns it red; output honours `NO_COLOR` / non-tty.
* **S.2 Fake the socket.** `ConfigStore`'s one-shot RPCs still reach
  `SocketClient.shared` directly (13 references in `ConfigStore.swift`,
  `ConfigStore+Commands.swift`, `ServicesCapabilities.swift`), so their
  success and failure paths are unexercised. Inject a narrow protocol with a
  hand-written fake. *Acceptance:* tests for each RPC's success AND failure
  path; `ui/coverage-floors.json` raised to the new measured figure.
* **S.3 Shrink `ui/.swiftlint-baseline.json`** (5 entries, shrink-only
  ratchet). *Acceptance:* entries fixed and deleted; a listed violation that
  stops firing is deleted, never re-recorded.
* **S.4 Verification and `get_knob_mode` onto the burst read.** `upload
  --verify` and `get_knob_mode` (`read_slot_table`) still walk slot by slot
  at the declared layout's width, so they cannot see keys past it.
  `read_full_table` is three queries instead of eighteen and strictly more
  complete. Deliberate change, its own commit. *Acceptance:* both paths
  call `read_full_table`; a fixture test with a record above the layout
  width is seen. *Depends on:* K.1 (same reader).
* **S.6 CI pins Python 3.14** (`.github/workflows/tests.yml:75`) for the job
  that runs the gates; the house moved to 3.15 on 2026-10-10. Green today,
  so not Phase 0. *Acceptance:* `python-version: "3.15"`, CI green. Size S.
* **S.7 Only this machine's cfg is linted.** The rust gate warns
  `only this machine's cfg was linted — set GOH_RUST_LINT_CONFIGS in
  .gatesrc` (2026-10-14). Declare every target/feature set the crate ships
  (arm64 macOS only; the CLI and daemon features). *Acceptance:* the
  warning gone, and a cfg-gated lint error in a non-default config goes
  red. Size S.

### Phase 2 -- one wired hardware session -- size M, needs the knob on USB-C

Goal: every question that only the device can answer, batched into one
sitting. Exit: each item below has a recorded measurement in VENDOR_UI_MAP
(positive control included) or a stated reason it could not be taken.

* **H.1 Confirm K.1 on the knob:** after `bind-slots`, `get_knob_mode`
  (MCP / socket) and the GUI report host-translate.
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
