# Workplan: closing out the 2026-09-07 knob session

**Transient.** This is an execution checklist, not a second backlog. When every
item is `DONE`, the durable findings fold into `PLAN.md` and this file is
deleted (repo rule: one forward-looking backlog file, completed plans pruned to
git history). Delete condition: all items DONE and `PLAN.md` updated.

Context: two defects made every knob binding a no-op (`key_id_for_knob`
hardcoded `BASE = 16`; media usages written to bytes 11/12 instead of 9/10),
and `upload` reported success on `hid_write` not erroring, which hid both.
Fixed and confirmed on hardware. What follows is everything those fixes
exposed or left behind.

Every item states how it is tested, and where a test is the point, how it is
proved capable of failing. Nothing is `DONE` on a green run alone.

---

## Phase 1 — hazards the key-ID fix created or exposed

### 1.1 `bind-slots --buttons` defaults to a guessed 3 — DONE

> Found: the count now comes from `DeviceConfig::button_count()` (rows x
> columns), with `--buttons` as an explicit override and a hard error when
> neither is available. The API path lost its `unwrap_or(3)` and now requires
> `buttons`. Dry run prints 4/5/6 for this VK01 without being told.

**Problem.** `key_id_for_knob` now takes a button count, and `bind-slots` gets
it from `--buttons`, which defaults to `3`. That is the same shape as the bug
just fixed: a constant standing in for the device's real layout, silently
writing to the wrong slots when it is wrong. The MCP/socket path has the same
`unwrap_or(3)`.

**Fix.** Take the layout from the config file the rest of the tool already
reads (`rows * columns`), same default resolution as `upload`. Keep
`--buttons` as an explicit override for a device with no config. No bare
numeric default anywhere in the path.

**Test.**
- Unit: a config with `rows: 1, columns: 3` yields knob IDs 4/5/6; `rows: 3,
  columns: 5` yields 16/17/18; `rows: 0` yields 1/2/3.
- Prove-it-fails: revert to the constant and watch the 3x5 case go red.
- Hardware: `bind-slots --dry-run` prints key IDs 4/5/6 for this VK01.

**Done when.** No `unwrap_or(3)` or `default_value = "3"` remains in the
knob-slot path, and the dry run prints 4/5/6 without being told the count.

---

### 1.2 A wrong layout in a config silently rebinds the wrong slots — DONE

> Found: two guards, both proved failable. `validate()` refuses a layer
> listing more buttons than the grid declares (the extra one lands in the
> knob's first slot). `upload` refuses a zero-button layout unless
> `--knob-only` confirms the device has no keys — `config_knob_only.yaml` is
> now correctly refused on this 3-button VK01. One branch I wrote turned out
> unreachable and was deleted rather than left as decoration. Splitting
> `cmds.rs` (543 lines) at the RE/diagnostic seam into `diag.rs` was needed
> to stay under the file-length cap.

**Problem.** `config_knob_only.yaml` declares `rows: 0, columns: 0`. Under the
corrected formula that puts the knob at key IDs 1/2/3 — which on this VK01 are
the three physical buttons. Flashing it would silently overwrite the buttons
with knob actions. Before the fix this file was harmless because everything
went to 16/17/18; the fix made it dangerous.

**Fix.** Two parts.
1. `upload` refuses when the config's declared layout would place a knob slot
   on top of a button slot, naming both. A collision is always a config error.
2. Reconcile `config_knob_only.yaml` with reality: either delete it (this
   hardware has buttons) or gate it behind an explicit "knob-only device"
   confirmation. Decide from the device, not from the filename.

**Test.**
- Unit: a config declaring 0 buttons but 1 knob and 3 button entries is
  rejected with a message naming the colliding key ID.
- Prove-it-fails: remove the guard, watch the collision test go red.
- Hardware: `upload config_knob_only.yaml` on the VK01 is refused, or flashed
  only after the explicit confirmation, and the buttons still work afterwards.

**Done when.** No config in the repo can silently rebind buttons as knob
slots, and the refusal names the collision.

---

### 1.3 One slot reads back `NotSeen` — DONE

> Found: not a hole in the device, a truncated reader. `read_slot`'s first
> parameter is not an address at all — it is how many slots wide a layer is,
> and `counter` then walks the table layer-major. The original loop capped
> counters at 3 for no reason beyond how `read-slots` had been written, so
> 108 reads still missed a slot. Addressing it properly is 18 reads and
> complete: a full flash now confirms **18/18**, and the read-back showed the
> whole device matching config.yaml exactly.

**Problem.** `upload` confirms 17/18. The 18th never appears in the table
walk. `NotSeen` is honest (it says nothing about the write) but it is a hole
in the instrument: a real failure could hide in it.

**Fix.** Establish whether the walk is incomplete or the slot genuinely is not
stored. Sweep `(group, counter)` beyond `0x01..=0x24` x `1..=3` — the current
range came from one observation and counters above 3 were never tried. Widen
`slot_table_addresses` to whatever actually answers.

**Test.**
- Hardware: sweep counters 1..=8 across groups 0x00..=0x40, record which pairs
  answer, and identify the missing slot by address.
- Regression: `slot_table_addresses` covers every `(key_id, layer)` pair that
  a 3-layer flash writes; assert the count.
- Prove-it-fails: shrink the range by one group, watch coverage drop.

**Done when.** A full flash reports 18/18, or the missing slot is identified
and `NotSeen` for it is explained in a comment with evidence.

---

## Phase 2 — usability defects found along the way

### 2.1 `config.yaml` resolves relative to the working directory — DONE

> Found: `validate`, `upload` and `bind-slots` now resolve to
> `~/Library/Application Support/antiknob/config.yaml`, seeded from a
> compiled-in starter on first use and never overwritten thereafter. An
> explicit path still wins. `install.sh` migrates an existing
> `/Applications` copy to the real location and removes it. Verified by
> running `antiknob validate` with no argument from `/tmp`.

**Problem.** `upload` and `validate` default to the literal path
`"config.yaml"`, so they work only from the right directory. The installer
drops a copy in `/Applications/Antiknob/`, which is not a config location and
is unreachable unless the user happens to `cd` there.

**Fix.** Default to `~/Library/Application Support/antiknob/config.yaml`
(where `host.json` and the socket already live), seeded from the packaged
sample on first use. Stop installing the `/Applications` copy; migrate an
existing one if present. An explicit path argument still wins.

**Test.**
- Unit: path resolution against a temp `HOME` — missing file is seeded,
  existing file is never overwritten, explicit argument overrides.
- Prove-it-fails: make the seeder overwrite, watch the "never overwrite" test
  go red.
- Live: `antiknob validate` succeeds from `/tmp`.

**Done when.** Both commands work from any directory and no config lives under
`/Applications`.

---

### 2.2 `listen` labels are unreadable — DONE

> Found: per-line prefix is now `vid:pid` alone; the named collection set
> (`kbd mouse pointer consumer`, `vendor:0xff00/0x01`) prints once in the
> header. Unknown pairs keep their hex rather than being dropped, so a device
> that grows a collection we have no name for stays visible.

**Problem.** Deduplicating snoop handles was right, but the label now prints
every collection as raw hex:
`514c:8850 [0x0001/0x06 0x0001/0x02 0x0001/0x01 0x000c/0x01]`, repeated on
every line of a capture.

**Fix.** Name the collections (`kbd mouse consumer vendor`) and print the full
set once in the header, not per line. Per-line prefix is `vid:pid` alone.

**Test.**
- Unit: collection-set formatting, including an unknown usage falling back to
  hex rather than being dropped.
- Live: a real capture is legible and still identifies the device.

**Done when.** A capture line fits comfortably and the header carries the
collection detail.

---

### 2.3 The app presents host layers as live when the hardware cannot reach them — DONE

> Found: the daemon now answers `get_knob_mode` by reading the slot table
> (`device::mode::classify`), and the layer view renders a banner whenever the
> layers cannot fire. Verified end to end: the live daemon reports
> `{"mode":"standalone","host_layers_can_fire":false,"slots_read":18}`, and the
> Media layer shows "These layers are inactive..." with the way out. `unknown`
> is its own state so the app never claims a mode it has not read. The mode is
> deliberately off the 2-second status poll — it walks the device's slot table
> and only changes on a reflash.
>
> One trap worth recording: the first screenshot showed no banner because
> `open` on an already-running app just focuses the stale process. The binary
> had the code; the process was an hour old. Check process start time, not
> just the build.

**Problem.** The root of "the app shows a different binding than the one I
assigned". The UI edits `host.json` and renders those layers as the knob's
behaviour, but they only fire if the firmware sends `ctrl+alt+F16..F18`. On a
standalone-flashed knob they can never fire, and nothing on screen says so.
This is the same class as the tap reporting bug: a UI stating an intention as
though it were a fact.

**Fix.** The layer view states which mode the device is in and what that
implies. Determine mode by reading the slot table (slot chords present =
host-translate; media/mouse actions = standalone) rather than by asking the
user. When standalone, the host-layer editor says plainly that these bindings
are inactive until slots are bound, and offers the action that changes it.

**Test.**
- Unit (Swift): mode -> banner-state mapping, all three cases including
  "device not connected / unknown", pinned in `AntiknobUITests`.
- Unit (Rust): slot-table -> mode classification from real captured records —
  a standalone table, a slot-chord table, and an empty one.
- Prove-it-fails: feed the classifier a slot-chord table and assert it does
  not report standalone.
- User-POV: run the app against this knob in its current standalone state and
  confirm the layer view says so.

**Done when.** The app cannot show a host layer as live while the device
cannot produce it.

---

## Phase 3 — reverse engineering, hardware in the loop

### 3.1 Map hold+twist — PARKED (needs the user at the device)

> Ready to run, blocked only on gestures. The probe is: write a distinctive
> media usage (`stop` = 0xB7, `play` = 0xCD, ...) to each candidate key ID
> beyond 6, confirm each write landed with the read-back that now covers the
> whole table, then one capture where the user performs hold+twist left and
> right tells us which ID owns which gesture. `verify` removes the ambiguity
> that blocked this before: a null result now means "not that gesture"
> rather than "the write may have failed".
>
> **The ask:** about two minutes at the knob — hold and twist left several
> times, then hold and twist right, while a capture runs.

**Problem.** `PLAN.md` held that hold+twist key IDs are unknowable without a
vendor-app reference. That is not established: the wide dump shows populated
slots at key IDs 19 and 21 carrying `prev`/`next` on layers 2 and 3, well past
the six that CCW/press/CW occupy. The read-back harness now makes probing
cheap, which it was not when that note was written.

**Method.** Binary search by writing, not by reading. For each candidate key
ID, write a *distinctive* media usage nothing else uses (e.g. `stop` = `0xB7`),
flash, and observe which gesture emits it. One write per candidate, one
gesture to check. `verify` confirms the write landed before any gesture is
attributed to it, so a null result means "this ID is not that gesture" rather
than "the write failed" — which is exactly the ambiguity that blocked this
before.

**Requires the user.** Each candidate needs a hold+twist performed at the
device. Batch candidates so one session of gestures tests several IDs: write a
different distinctive usage to each candidate, then a single capture
distinguishes them by code.

**Test.**
- Hardware: hold+twist left and right each emit their assigned distinctive
  usage, reproducibly, across two flashes with different assignments (so a
  coincidence cannot pass).
- Regression: the discovered IDs go into `key_id_for_knob`'s scheme with a
  test pinning them, and `bind-slots` stops printing "hold+twist slots
  unchanged (key IDs unverified; use the vendor app)".

**Done when.** Hold+twist gestures are bindable, or the dump proves they are
not stored in this table and that is recorded with evidence.

---

## Phase 4 — land it

### 4.1 Commits — TODO

Four, in dependency order, each green on `tools/gate.sh --full`:

1. **daemon reaches the keyboard again** — login item starts the app bundle,
   stable signing identity + `tools/make_signing_cert.sh`, `KeepAlive`
   honouring a clean exit, the grant message, tray "open Accessibility",
   installer changes, README.
2. **`listen` tells the truth** — device filter, one handle per physical
   device, decoding against declared collections, label formatting.
3. **knob bindings actually land** — `key_id_for_knob` from layout, media
   offset, `device::verify` + `upload` read-back, corrected pinned tests,
   layout-collision guard, config path resolution.
4. **docs** — `PLAN.md` findings, delete `WORKPLAN.md`.

**Done when.** `git status` is clean and every commit builds and passes the
gate on its own.

---

### 4.2 Roadmap item added 2026-09-07 — RECORDED

> Three layers with the third virtual and daemon-driven, hot-swappable,
> exposed over MCP, with per-layer LED identity. Written up as `PLAN.md`
> item 3, including the two open questions that have to be answered before
> it can be flashed: it presupposes host-translate mode, and nothing found
> so far reads or sets the device's active layer on a knob with no spare
> button.

---

## Running rules for this loop

- No item is `DONE` on a green test alone. Anything touching the device is
  confirmed against the device; anything visual is confirmed on screen.
- Every new test is proved capable of failing before it is trusted.
- When an item turns out to be wrong or unnecessary, say so and strike it —
  do not implement it to close a checkbox.
- Hardware-in-the-loop items (3.1) stall on the user, not on the loop. Park
  them with a precise ask rather than guessing, and keep going elsewhere.
