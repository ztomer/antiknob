# Live checks: the things that need you at the knob

Everything in this session that could be settled without touching the
hardware has been. What is left needs a hand on the knob or an eye on its
LED. Each check below is one command, what to expect, and what the answer
means. They are ordered so a single sitting clears them all.

Nothing here is destructive. The two probes write to the device and put back
exactly what they found, verified by reading it again -- but if you stop one
midway, the last section says how to restore by hand.

Binaries are at `/Applications/Antiknob/bin/antiknob` (already on your PATH
as `antiknob` if `~/.local/bin` is in it).

---

## 1. Does the knob still do what you asked? (30 seconds)

The flash from earlier is confirmed on the wire, but you have not turned the
knob since the layout was re-flashed with the LED colours.

```bash
antiknob listen --device 514c:8850 --timeout-secs 20
```

Twist counter-clockwise, twist clockwise, press.

**Expect:** `consumer 0x00ea (volume down)`, `consumer 0x00e9 (volume up)`,
`consumer 0x00e2 (mute)`.

**If instead** every gesture shows the same code, the flash did not take --
send me the output.

---

## 2. Which slots are hold + twist? (2 minutes)

This is the one that unblocks the rest. `PLAN.md` had held that hold+twist
key IDs could not be found without the vendor app; that turned out to be
wrong -- slots 7 and 8 exist on every layer and carry only a factory
placeholder, which is what an unbound gesture looks like.

```bash
antiknob probe-gestures --device 514c:8850 --capture-secs 60
```

It writes a distinct marker to slots 7 and 8, confirms both landed, then
asks for gestures. When it prompts:

- **hold the knob down and twist LEFT** — five or six times
- **hold the knob down and twist RIGHT** — five or six times
- then leave it alone until the capture ends

**Expect** one of:

- `usage 0x00b7 -> slot 7 IS a gesture` and `usage 0x00cd -> slot 8 IS a
  gesture`. That settles it: 7 and 8 are hold+twist, and I can bind them.
- `slot 7 never fired` / `slot 8 never fired`. Then hold+twist lives
  somewhere else, and the next batch to try is
  `--candidates 9,10,11,12` (same command, that flag added).

It ends with `Candidate slots restored, and read back to confirm.` If it says
anything else, tell me before running it again.

**Why the markers are transport keys:** they have to be codes the knob does
not already emit, or an ordinary twist would look like a probe hit. Pressing
`stop`/`play` with nothing playing does nothing.

---

## 3. Is there a breathing LED mode? (1 minute, watch the knob)

You asked for breathing colours per layer. This firmware's mapped modes are
`off / backlight / shock / shock2 / press` — **none of them is documented as
a breathe**, and `led_mode_name` reports a mode 5 ("custom") that nothing in
the code can currently produce. Rather than pick one and call it breathing, I
left the third layer's LED unset and built this.

```bash
antiknob led-probe 0 --dwell-secs 4
```

**Watch the knob's light.** It holds each mode for four seconds and prints
which one it is showing.

**Tell me:** which mode number, if any, pulses/fades rather than sitting
steady. If none does, say so — then breathing is not available on this
firmware and we pick a different way to distinguish layer three (a distinct
colour, or `shock`, which reacts to input).

It restores the layer's original mode at the end.

---

## 4. Optional: try the virtual layer for real

The third layer's machinery is built and tested, but it can only *fire* if
the knob is in host-translate mode — which it is not, because you chose
standalone so the knob works on other machines. Only do this if you want to
see it working; it takes the knob out of standalone until you flash back.

```bash
antiknob bind-slots                      # knob now sends ⌃⌥F16/F17/F18
```

Then edit `~/Library/Application Support/antiknob/host.json` to give a layer
`variants` (I can write this for you — say the word and name a couple of
apps), and:

```bash
printf '{"jsonrpc":"2.0","id":1,"method":"get_virtual_layer","params":{}}\n' \
  | nc -U /tmp/antiknob.sock
```

**Expect** `"resolution":{"reason":"matched_app","variant":"..."}` changing as
you switch between the apps you named.

**To go back to standalone:**

```bash
antiknob upload
```

**Expect** `18/18 slot(s) confirmed.`

---

## If a probe is interrupted

Both probes restore what they found, and verify the restore by reading it
back. If you stop one with ctrl-C partway, the knob may be left with a probe
marker on slots 7/8 or a different LED mode. To put everything back:

```bash
antiknob upload          # re-flashes all 18 slots and the layer LEDs
antiknob led 0 backlight red
antiknob led 1 backlight green
```

`upload` reads the device back and tells you how many slots it confirmed;
`18/18` means the knob is exactly as configured.

---

## What I still owe you, once you've answered

- **Check 2** decides whether hold+twist becomes bindable. If slots 7/8 are
  it, `bind-slots` learns two more slots and the five-gesture set is
  complete.
- **Check 3** decides layer three's LED.
- The **device's active layer** is the last open question for the virtual
  layer roadmap — see `PLAN.md` item 3. A knob with no spare button cannot
  switch device layers, so either that query is found or Media/Navigate move
  host-side and lose their no-daemon property. My read-only sweep for it is
  recorded in `PLAN.md`; if it came up empty, the next step needs the vendor
  app running under Rosetta with a USB capture, which is a bigger job than
  anything here.
