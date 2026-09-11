# Firmware-engineer review: host↔VK01 wire paths (2026-09-11)

Point-in-time audit of every byte this tree puts on the wire and every
timing it depends on. Read the code, not the comments; checked each claim
against the implementation. Severity-ranked. Nothing here was changed --
fixes are follow-up work, not this file.

Conventions: `firmware.rs` = the hardware map, `policy.rs` = process
budgets. All paths `src/...` unless noted.

## What holds (checked, not assumed)

- **Report framing.** Every builder emits `REPORT_LEN` bytes behind
  `REPORT_ID`; `send_report` strips a redundant prefix so the command byte
  lands at wire byte 1. Burst math is exact (3 x 25 = 75 records) and
  `FD_MAX_ENTRIES` derives instead of restating.
- **Layer numbering is consistent across encodings.** Slot records are
  1-based on the wire (`layer + 1` in every builder, `layer == 0` rejected
  in every parser); LED traffic is 0-based (`03 FE B0 00 05` per the vendor
  capture). Both directions agree in each subsystem.
- **Init-before-every-write.** All LED paths go through `send_led`
  (init + 20ms + packet), including the probe's restore path as of this
  week. No commit follows LED writes, matching the vendor capture.
- **Reply discrimination.** `led_mode_of` matches framed `FA` replies and
  skips the init packet's `FB` echo. Cross-job confusion is impossible:
  `open_device_on` opens a fresh handle per `with_device` job, so input
  queues are isolated per job and only intra-job echoes exist.
- **Chord/decoder agreement.** `SLOT_SPECS`, `SLOT_KEYCODES`,
  `SLOT_CHORD_MODS` and the gesture decoder pin each other by test.
- **Retries are all bounded.** Socket connect (10 x 25ms), probe restore
  (5 x backoff), bootout poll (50 x 100ms), LED reply skips (8). No
  unbounded retry loop in the tree.
- **JSON int handling.** Negative/overflowing `layer` values fail
  deserialization into a clean RPC error, not a wrapped byte.

## Findings

### 1. Supersede check races the write phase (high -- new code)

`sync_led`'s worker takes a ticket, paces, checks `latest_ticket` ONCE,
then spends ~1s reading stored modes and writing layers one by one. A
newer sync arriving during the read or between layer writes is never
noticed: the stale job keeps writing older modes over (or interleaved
with) the newer job's. Under rapid A->B->A switching the final per-layer
state can mix intents, and the loser is silent.

Fix: re-check the ticket after the read and between layer writes; drop
on supersede at each point, not just the first.
`src/host/led_sync.rs`, worker body.

### 2. Overlong payloads truncate silently (medium)

`send_report` clamps with `.min(REPORT_LEN)`. A 70-byte packet goes out
as 64 bytes with success reported. Packet builders all emit exactly
`REPORT_LEN`, so this can only fire on hand-built input (`send_raw`
caps at 64 already) -- but a silent clamp is exactly the shape that
produced this repo's stored-but-wrong history.

Fix: bail on `payload.len() > REPORT_LEN` instead of clamping.
`src/device/mod.rs`, `send_report`.

### 3. Slot addressing arithmetic is unchecked (medium)

`key_id_for_knob`: `(button_count as u8) + 1 + knob_index * 5 + offset`
wraps in release, panics in debug. `buttons` arrives from JSON as an
unbounded `usize`; `button_count` from a layout file is equally
unvalidated. A wrapped key id writes a well-formed packet to the wrong
slot -- the defect class this repo is built around. Same family:
`GetKnobMode` saturates `buttons + 5` to `u8::MAX`, issuing a 255-wide
table walk at the firmware.

Fix: checked arithmetic returning an error at both sites; reject
absurd `buttons`/`button_count` early with the value quoted back.
`src/protocol.rs` (`key_id_for_knob`), `src/api/dispatch.rs`
(`GetKnobMode`), `src/config.rs` (`slots_per_layer` path).

### 4. Read-before-write compares mode number only (low)

`pending_writes` skips when the stored NUMBER matches, ignoring the
palette in the spec string. Harmless on this VK01 (it ignores colour
bytes), stale on the 16-key variant sharing the product id, which
honours per-key RGB.

Fix: compare the full normalized spec, or document the VK01-only
assumption at the filter. `src/host/led_sync.rs`, `pending_writes`.

### 5. Socket RPCs head-of-line-block (low, pre-existing)

The accept loop runs `handle_client` inline, and one connection stays
open per RPC only because current clients close after one line. A paced
`set_led` (~500ms+) or a slot-table walk (seconds) stalls every other
RPC meanwhile. This predates the pacer; the pacer lengthens the stall
by a bounded amount.

Fix (deliberate, not drive-by): per-connection thread or a bounded
worker pool. Do not "fix" by removing the pacing.
`src/api/socket.rs`, accept loop.

### 6. The pacer is process-local (note, not a defect)

Daemon jobs pace against each other; the CLI paces nothing. A CLI
write landing inside a daemon sync burst still bursts at the firmware.
Cross-process pacing needs a lockfile, which is more machinery than a
one-shot CLI justifies -- but the residual risk is real, which is why
the CLI prints the replug remedy on every write. No change proposed;
recorded so the next wedge during mixed CLI/daemon use is diagnosed
in minutes, not hours.

### 7. `set_led` accepts layers 0-15; the firmware holds 3 (question)

Writes to layers 3-15 validate, store (presumably), and read back, with
unknown render effect. Either the firmware aliases them somewhere
visible -- worth one probe -- or the cap should be the firmware's
layer count, not the protocol field width.
`src/api/dispatch.rs` (`SET_LED_LAYER_MAX`), `src/policy.rs`.

### 8. `led-probe` should state the wedge risk up front (suggestion)

The probe exists to be stared at, and it performs six consecutive mode
changes -- the highest single-command freeze risk in the tree, now that
everything else paces. One output line before the walk ("if a mode
stops rendering, unplug/replug; the rest of the walk is still valid")
costs nothing.
`src/bin/antiknob/diag.rs`, `run_led_probe`.

## Calibration ledger (timings and their evidence)

| Budget | Value | Standing on |
|---|---|---|
| LED init settle | 20ms | worked 2026-09-07 after hours of failure without it; lower never probed |
| Inter-layer settle | 150ms | probe's settle, the only measured number; adopted by sync + uploads (uploads were 20ms) |
| Job interval | 500ms | reasoned, not measured; retune if a wedge recurs under paced writes |
| Slot write gap | 15ms | long-standing, never bisected |
| Upload/commit gaps | 10/50ms | vendor-mirroring, unchanged for months |
| Query timeouts | 250/500ms | patience, not firmware; failures are clean errors |

## Out of scope, deliberately

macOS API codes (CG/NX/AX), host vocabularies, test fixtures (they pin
the maps with independent literals -- converting them would make the
tests tautological), user config values, and anything quoted in a doc
comment rather than executed.

## Resolutions (2026-09-11, same session)

1. Supersede re-checked after the read and between layer writes
   (`sync_led` worker); `superseded()` helper, lock never held across
   HID work.
2. `send_report` refuses oversize payloads instead of clamping.
3. `key_id_for_button` / `key_id_for_knob` return `Result` with checked
   arithmetic; `GetKnobMode` errors instead of saturating to a 255-wide
   walk; `DeviceConfig::validate` refuses layouts past the slot space
   (checked math, values quoted); `--buttons` and socket `buttons`
   validated; `layer_idx as u8` replaced at both upload sites. A new
   test (`absurd_counts_refuse_rather_than_wrap`) caught a real
   `usize::MAX + 1` overflow in the fix itself before it shipped.
4. Skip only on exact canonical spec match; aliases and paletted specs
   always write. Pinned by test.
5. Accept loop spawns a named thread per connection (bounded by the
   existing 16-connection cap); context behind a per-request mutex with
   poison recovery. Pinned by an 8-client concurrency test.
6. No change (process-local by nature); CLI prints the remedy.
7. Answered by measurement, not changed: layers 3-15 store without
   aliasing 0-2 (green stored at 9, 0-2 untouched, restored after), so
   no clamp -- but `set_led`/`led` now state "stores, render unmeasured"
   past layer 2. Whether 3-15 render at all still needs eyes.
8. Probe prints the wedge warning and remedy before its walk.
