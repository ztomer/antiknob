# References

Where the facts in this repo come from. Each entry says what it is good for
and, where it matters, what it is NOT good for -- several wrong turns this
session came from reaching for the nearest reference rather than the one that
answers the question.

---

## kriomant/ch57x-keyboard-tool — the protocol, for key bindings

<https://github.com/kriomant/ch57x-keyboard-tool>

Open-source programmer for CH57x macro keyboards. **Ground truth for the key
binding protocol.** Read it via `gh api repos/kriomant/ch57x-keyboard-tool/
contents/<path>` rather than cloning.

What it settled:

* **`KnobAction { RotateCCW, Press, RotateCW }` is THIS TOOL's model of the
  knob, not the firmware's capability.** Reading it as the latter produced
  the worst wrong claim of this session -- that hold+twist does not exist.
  It does: the vendor binds all five gestures, at key IDs 2..6 in the `0xFD`
  command space this project's `k884x` driver does not use. A probe finding
  nothing in the `0xFE` space was treated as proof of absence.
* **Our device is in its table**: `(Ch57x_3, 0x514c, 0x8850, 0x04)`.
* **Key ids are per-model constants there**, e.g. `MAX_NUMBER_OF_BUTTONS + 1
  + 3 * knob + action`, with the constant 12, 15 or 16 depending on model.
  That does NOT match this hardware, which reads its knob from slots 4/5/6
  with three buttons -- measured, not inferred. This build derives the base
  from the declared layout instead, and verifies every flash by read-back.

What it is NOT good for: **the LED protocol.** Its driver for `514c:8850`
refuses LED commands outright and asks for help at
<https://github.com/kriomant/ch57x-keyboard-tool/issues/60>. Concluding from
that that this device has no working backlight was wrong -- it has one, the
project simply has not reverse-engineered it.

---

## ANTICATER.app — the vendor app, and the only LED ground truth

`/Applications/ANTICATER.app` (x86_64, Qt 5.12, hardened runtime).

The one reference that actually answers LED questions, because it drives the
hardware. Two ways in, both used:

**Static.** `otool -tV` on the binary. `Widget::SetRgb_Led_Key`,
`Widget::Read_RgbLed_DataDsp`, and the globals `_RGB_LED_Md`,
`_KeyBoard_KeyLed`, `_RgbLED_Change_Flag`. This gives the packet layout and
the mode bound (`cmpq $0x5; ja invalid`, so modes are 0..=5).

**Dynamic — capture the traffic.** Hardened runtime blocks lldb and
`DYLD_INSERT_LIBRARIES`, so work on a COPY:

```bash
ditto /Applications/ANTICATER.app /tmp/AC.app
# the two libhidapi symlinks fail to copy; recreate them
(cd /tmp/AC.app/Contents/Frameworks && \
   ln -sf libhidapi.0.12.0.dylib libhidapi.0.dylib && \
   ln -sf libhidapi.0.12.0.dylib libhidapi.dylib)
# build an x86_64 interposer over hid_write / hid_read_timeout, then
codesign -s - -f --deep --entitlements ent.plist /tmp/AC.app   # get-task-allow
                                                               # + disable-library-validation
DYLD_INSERT_LIBRARIES=/tmp/hidlog.dylib HIDLOG=/tmp/hidlog.txt \
  /tmp/AC.app/Contents/MacOS/ANTICATER
```

`hid_write` is dynamically linked from the bundled `libhidapi.0.dylib`, so
interposing works. The interposer source is in this session's scratch; it is
30 lines and logs only, forwarding every call unchanged.

What the capture established:

* `03 FE B0 <layer> <mode>`, then a base colour at bytes 5-7 and sixteen
  per-key RGB triples from byte 8.
* Selecting a mode in the UI sends nothing. Only **Save settings** transmits,
  and it sends no commit afterwards.
* Only layer 0 receives the selected mode; layers 1 and 2 are always written
  mode 0.
* The `03 FB FB FB` handshake replies with device state including the live
  palette -- a read path that returns a colour, unlike `FA B0` which returns
  only a mode byte.

**Read the issue tracker, not just the source.** Everything below was already
written down in `kriomant/ch57x-keyboard-tool` issues #173 and #175, tested
on this exact hardware, while this session spent hours inferring it from a
disassembly and a packet capture. The source was fetched; the issues were
not opened until much later.

* **`03 FB FB FB` is REQUIRED before any LED write.** Without it the device
  accepts the write, stores the mode, reads it back correctly, and changes
  nothing. Confirmed here: mode 0 turned the light off only once the init
  preceded it.
* **Mode table for this device**: `0 off, 1 static, 2 reactive, 3 ripple,
  4 rainbow`. Mode 4 is the multicoloured effect the knob ships in.
* **Mode 5 CRASHES the firmware.** It wedged this knob's LED renderer until
  the device was power-cycled -- and this model has no power switch and stays
  lit on battery when unplugged.
* **The 3-button knob ignores the colour bytes.** Mode 1 was set with blue,
  red and green in turn and stayed red every time. The 16-key device sharing
  this product id does honour them, which is why they are still sent.
* Known firmware bug: LEDs freeze after 2s-2m and only a replug recovers.

An earlier version of this file said the LED packet layout was "correct as
written" and that mode 5 carried a palette. Both were wrong, and both were
written confidently off partial evidence.

---

## vk01-anticater — the host-side predecessor

`/Users/ztomer/src/vk01-anticater` (local).

The Swift daemon this project was ported from: event tap, layers, slot
chords, synthetic output. Useful for behaviour parity on the HOST side --
`port-parity` questions like what a gesture should do.

Not useful for hardware: it contains no HID writes and no LED code at all.

---

## jaspercurry/JTS — engineering practice

<https://github.com/jaspercurry/JTS>, cloned to `/Users/ztomer/src/JTS`.

A Raspberry Pi voice speaker. **No HID, knob or LED-protocol content** -- its
`led`/`rgb` matches are incidental, so it cannot answer a device question
here. Recorded for its documentation and verification practice: ADRs under
`docs/adr`, `DEEP-AUDIT-PLAYBOOK.md`, a `doc-map.toml`, bring-up docs, and
tests named for the property they defend rather than the function they call
(e.g. `test_crossover_v2_conductor_honesty_gates.py`).

---

## The device itself

The reference of last resort and first authority. `antiknob read-slots`
changes no state, and `upload` reads back what it wrote. Where a document and
the device disagree, the device is right -- that is how the knob's real key
ids (4/5/6, not 16/17/18) and the media byte offset (9, not 11) were found,
both of which contradicted the code and one of which contradicted this
repo's own notes.
