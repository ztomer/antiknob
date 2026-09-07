# Workplan: the virtual layer, and everything reachable without the knob

**Transient.** Execution checklist, not a second backlog. When every item is
`DONE` or resolved, the durable findings fold into `PLAN.md` and this file is
deleted. Delete condition: all items DONE or struck, and `PLAN.md` updated.

The previous round -- daemon TCC identity, knob key IDs, the media offset,
flash verification, `listen`, the UI reachability notice -- is committed and
closed; its findings live in `PLAN.md` item 2 and in the commit messages.
What follows is `PLAN.md` item 3, the virtual third layer, plus the parts of
the hardware questions that can be built before anyone touches the knob.

**The constraint that shapes this round.** Everything below is built and
tested without the device. Anything that can only be settled by turning the
knob is *prepared*, not guessed: it becomes a command the user runs in one
step, with a stated expected result. `PARKED` means exactly that, never
"assumed to work".

---

## Phase 5 — the virtual layer, host side

### 5.1 A layer can be virtual, and say what drives it — DONE

> `HostLayer.variants`, skipped when empty so a config written before this
> serialises identically. Pinned against the real `host.json` from this
> machine rather than a fixture.

**Problem.** `HostLayer` has a name and five gestures. A virtual layer needs
more: a set of named binding-sets it can swap between, and a rule for which
one is active. Nothing in the config can express that.

**Fix.** A layer may carry `variants` and a `selector`. A variant is a name
plus the five gestures; the selector says how one is chosen (explicitly set,
or matched against the frontmost application's bundle id). With neither, a
layer behaves exactly as it does now.

**Test.**
- Unit: the `host.json` already on this machine round-trips unchanged.
- Unit: a layer with variants resolves the right one for a bundle id, falls
  back to its base bindings when nothing matches, and is deterministic when
  two rules could match (first wins, documented).
- Prove-it-fails: make the fallback return the last variant instead of the
  base; the no-match test must go red.

**Done when.** The schema expresses a virtual layer and the live config still
loads byte-identically.

---

### 5.2 Resolve the active variant from the frontmost app — DONE

> `host::frontmost` is the seam (one trait, a fixed fake and a settable
> fake); `virtual_layer::resolve` is pure. Override beats app, app beats
> base, and an override naming nothing resolves to base rather than to a
> neighbouring variant.

**Problem.** The obvious driver is the frontmost application, but the
mechanism must not be hard-wired to it -- `PLAN.md` says so, and an agent
setting the variant over MCP is the other driver already requested.

**Fix.** A pure resolver: `(selector, context) -> variant`, where context
carries the frontmost bundle id and any explicit override. The override wins.
Asking macOS what is frontmost sits behind a narrow seam with a hand-written
fake, so the resolver tests need no AppKit.

**Test.**
- Unit: override beats app match, app match beats base, unknown app falls
  back, an empty selector matches nothing.
- Unit: switching apps changes the variant and switching back restores it --
  no latching.
- Prove-it-fails: drop the override precedence; the ordering test must fail.

**Done when.** The resolver is pure and covered, and the OS query is faked.

---

### 5.3 Wire the resolver into the daemon — DONE

> Four call sites indexed the layer directly; they now funnel through
> `Engine::action_for`, so a fifth added later cannot forget variants. The
> app is read at fire time, with a test that fails if the read is hoisted.
> `engine.rs` hit the 500-line cap and its virtual-layer tests moved out.

**Problem.** Resolution has to happen at gesture time. Reading the frontmost
app when the knob turns is one call; polling for it is a standing cost for a
value that only matters at that instant.

**Fix.** The engine consults the resolver when a slot chord fires, using the
frontmost app read at that moment (the repo's own rule 8 -- resolve user
intent at interaction time). The tray shows the active variant.

**Test.**
- Unit: a gesture on a virtual layer runs the variant's action, not the
  base's, given a fake frontmost-app source.
- Unit: the same gesture under a different frontmost app runs a different
  action.
- Prove-it-fails: pin the resolver to index 0; both must go red.

**Done when.** Gestures resolve through the selector, with the OS read taken
at fire time.

---

### 5.4 Expose the swap over the socket and MCP — DONE

> `get_virtual_layer` / `set_virtual_variant`, both in `all_tools`. The
> report carries the resolution *reason*; an unknown pin is refused naming
> the real variants. Verified against the running daemon.

**Fix.** `get_virtual_layer` reports the variants, which is active, and *why*
(override / matched app / base). `set_virtual_variant` sets or clears the
override. Both in `all_tools`, described by what they do.

**Test.**
- Unit: dispatch round-trips both against a config in a temp dir.
- Unit: set an override then clear it and app-driven resolution resumes --
  the clear must not latch.
- Prove-it-fails: make clear a no-op; that test must fail.
- Live: `nc -U` both against the running daemon.

**Done when.** Both work over the socket and appear in the tool list.

---

### 5.5 LED identity per layer — DONE (third colour PARKED)

> Found: **there is no breathing mode.** The firmware's mapped modes are
> off / backlight / shock / shock2 / press, and `led_mode_name` reports a
> mode 5 ("custom") that `build_led_packet` cannot produce. Media is steady
> red and Navigate steady green; the third layer's LED is deliberately left
> unset, with a test that fails if anyone sets it, because writing a steady
> colour and calling it "breathing" would be the same unearned claim this
> session keeps removing. `antiknob led-probe` walks the modes so the
> question can be answered by looking.

**Problem.** Requested: Media breathing red, Navigate breathing green,
virtual multicoloured breathing. `config.yaml` already has an optional
per-layer `led` string that `upload` flashes.

**Fix.** Set the first two in the shipped starter layout -- configuration,
not new protocol. The multicoloured breathe is NOT assumed: the vendor mode
table is only partly mapped, so build a probe that walks the modes and
reports what `led-read` says each one is, and park the choice on that.

**Test.**
- Unit: the starter layout's LED specs produce the expected packets.
- Prove-it-fails: change a colour; the packet assertion must fail.
- `PARKED`: which mode is "multicoloured breathing" needs eyes on the device.

**Done when.** Red and green are configured and tested; the third is one
command plus a look.

---

## Phase 6 — prepare the hardware questions

### 6.1 Make the hold+twist probe one command — DONE (running it PARKED)

> Found: slots 7 and 8 exist on every layer, answer the read, and carry only
> a factory placeholder (kind 1, byte 9 = key_id + 3, no keycode). So
> `PLAN.md`'s "needs the vendor app to diff against" was wrong -- the
> question is answerable by experiment. `antiknob probe-gestures` arms
> distinct markers, confirms they landed *before* asking for a gesture, and
> restores afterwards. The dry run found two defects in my own code: the
> first restore after a capture always fails (macOS still holding the
> snoop's handles), and restore claimed success on a write returning Ok
> without reading anything back. Both fixed; slots verified byte-identical
> to a pre-probe dump.

**Fix.** `antiknob probe-gestures` writes a distinctive media usage to each
candidate key ID beyond the six known ones, verifies each landed via the
read-back, captures, decodes, and reports which key ID emitted what. Restores
the previous bindings afterwards and says so.

**Test.**
- Unit: the plan assigns a *distinct* usage per candidate -- a repeat would
  make two gestures indistinguishable, which is the whole point.
- Unit: the restore plan is exactly the table read before the probe.
- Prove-it-fails: assign the same usage twice; the distinctness test must
  fail.
- `PARKED`: running it needs the user.

**Done when.** One command does write, verify, capture and restore.

---

### 6.2 The device's active layer — TODO

**Problem.** Unsolved, and it gates the roadmap item: nothing found reads or
sets which device layer the firmware is on, and this knob has no spare button
to switch it.

**Fix.** Read before write. The slot-table query is understood now, so sweep
for a *read* that reports the active layer -- the vendor app must know it. If
a read is found, look for the matching write. If neither is, record what was
tried with its bounds rather than leaving "unsolved".

**Test.** Read-only sweep (`read_slot` changes no state, proven earlier).
Whatever is found gets a decoder and a test against captured bytes.

**Done when.** The query is identified, or the search is recorded so nobody
repeats it blindly.

---

## Phase 7 — hand back

### 7.1 Instructions for the live checks — TODO

One file listing every parked check as a command with its expected result,
ordered so a single session at the knob clears them all.

---

## Running rules for this loop

- Nothing is `DONE` on a green test alone; anything hardware-facing is
  `PARKED` with a command, never assumed.
- Every new test is proved capable of failing before it is trusted.
- The `host.json` on this machine is the user's live config, not a fixture.
  It must keep working untouched at every step.
- Strike an item that turns out to be wrong rather than implementing it to
  close a checkbox.
