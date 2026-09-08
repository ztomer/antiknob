# Vendor app UI -> wire map

What ANTICATER.app sends, and which control sends it. Written while driving
the app under `tools/hidsnoop/`, so every packet below was observed rather
than inferred -- this repo has been burned three times by reading a capture
for what it expected to find.

Method: the app runs re-signed under the `hid_write`/`hid_read_timeout`
interposer; System Events drives it over the accessibility API (Qt exposes a
full AX tree, which is why this is scriptable at all); each experiment
changes ONE thing, presses **Save settings**, and diffs the capture log.

Nothing reaches the device until **Save settings** is pressed. Every other
click is UI state only -- confirmed by watching the log stay silent.

## Window

`2336 x 1850` at `(5, 35)`. The layout needs most of a large display or
controls overlap and AX positions stop matching what is drawn.

## The five knob gestures

Five `AXCheckBox` elements laid out around the knob graphic. Their titles are
the CURRENT assignment, which is how the UI shows what is bound:

| # | AX title (as found)  | Position    | Size    | Gesture          |
|---|----------------------|-------------|---------|------------------|
| 1 | `Previous track`     | (742, 354)  | 136x136 | twist CCW        |
| 2 | `Volume-`            | (1052, 354) | 136x136 | twist CW         |
| 3 | `Next track`         | (852, 427)  | 227x227 | press            |
| 4 | `Previous track`     | (742, 600)  | 136x136 | hold + twist L   |
| 5 | `Volume+ Next track` | (1052, 600) | 136x136 | hold + twist R   |

Gesture 5's title is TWO actions in sequence. That is the chaining the
`0xFD` record's `n_groups` field encodes, seen in the vendor's own UI.

## Controls

| Element        | Position     | Size    | Role       |
|----------------|--------------|---------|------------|
| Device Connect | (5, 63)      | 2336x63 | AXButton   |
| clear          | (2043, 263)  | 256x127 | AXButton   |
| Clear All      | (2043, 409)  | 256x127 | AXButton   |
| View settings  | (2043, 554)  | 256x127 | AXButton   |
| Save settings  | (2043, 700)  | 256x127 | AXButton   |
| (text area)    | (1371, 354)  | 364x400 | AXTextArea |
| RGB LED group  | (19, 1111)   | 210x637 | AXGroup    |
| (large group)  | (229, 974)   | 2097x911| AXGroup    |

## LED mode picker

The large group at (229, 974) is six buttons, one per firmware mode:

| Button      | Position     | Size    |
|-------------|--------------|---------|
| `LED Mode0` | (249, 986)   | 278x847 |
| `LED Mode1` | (525, 986)   | 277x847 |
| `LED Mode2` | (800, 986)   | 278x847 |
| `LED Mode3` | (1076, 986)  | 277x847 |
| `LED Mode4` | (1351, 986)  | 278x847 |
| `LED Mode5` | (1627, 986)  | 277x847 |

Mode 5 is a first-class button in the vendor's own UI, which is a third
independent reason the "mode 5 crashes the firmware" claim was wrong.

## The device is held exclusively

While ANTICATER.app is running, `antiknob read-slots` fails with
`exclusive access and device already open`. So the slot table cannot be read
to confirm a save while the vendor app is up: the sequence has to be
drive -> save -> quit -> read. Restoring this repo's own layout afterwards
is `antiknob upload`, which cannot run until the vendor app is closed either.

## The action vocabulary

The left column (`19, 1111` down) is seven `AXRadioButton` categories. Picking
one refills the large panel at `(229, 974)` with that category's actions.
Clicking an action APPENDS it to the selected gesture's sequence.

| Category             | Position    | Items | Contents |
|----------------------|-------------|-------|----------|
| `BaseKeys`           | (19, 1111)  | 102   | letters, digits, punctuation, arrows, `Ctrl+` `Shift+` `Win+` `Alt+` prefixes |
| `Ctrl Shift Alt`     | (19, 1202)  | 53    | modifier COMBOS (`Ctrl+Alt+Shift+Win+`), `F13`-`F19`, `command` |
| `MutiMedia`          | (19, 1293)  | 21    | transport, volume, brightness, `WWW *`, `Calculator`, `E-mail`, `My Computer` |
| `RGB LED`            | (19, 1384)  | 6     | `LED Mode0` .. `LED Mode5` |
| `Mouse/Swipe screen` | (19, 1475)  | 16    | buttons, wheel, modifier+click, `Left/Right/Up/Down Swipe`, `Like` |
| `DelaySetting`       | (19, 1566)  | 36    | delays (titles are empty in AX; values still to be read) |
| `Procreate`          | (19, 1657)  | 31    | app-specific macros |

Three of these are capabilities this repo does not model at all:

* **`DelaySetting`** -- a delay is an ACTION in a sequence. This is almost
  certainly what the `0x32` (=50) bytes in the first captured `0xFD` record
  were: `03 fd 05 01 01 00 01 00 00 04 00 32 00 00 32 ...` is `a` followed by
  repeated 50s.
* **`RGB LED` as an action** -- a gesture can change the backlight mode. The
  LED is not just per-layer state.
* **`Procreate`** -- 31 named macros (`Increase Brush Size by 5%`,
  `Toggle Perspective Guide`, `Enter Transform Mode`, ...). Whatever the
  vendor does here is expressible as ordinary chords plus delays, so this is
  the worked example of "drive an application with the knob".

`MutiMedia` full list: Next track, Stop, Previous track, Play/Pause, Screen
brightness+/-, My Computer, Bass+/-, Treble+/-, WWW Pagerefresh/forward/back,
WWW home, Volume+/-, Mute, Multimedia, Calculator, E-mail.

`Mouse/Swipe screen` full list: Mouse LeftKey, Mouse Right, Mouse Middle,
Mouse Wheel+/-, Ctrl+Mouse Up/Down, Shift+Mouse Up/Down, Alt+Mouse Up/Down,
Like, Left/Right/Up/Down Swipe.

## How a binding is edited

1. Click a gesture checkbox -- selects it (the app draws it yellow).
2. Pick a category, then click actions in the panel. Each click APPENDS.
3. The gesture checkbox's TITLE becomes the sequence, space separated
   (`Volume+ Next track`), and the text area shows the same.
4. `clear` empties the selected gesture (its title becomes `NULL`).
5. `Save settings` writes to the device. Nothing before it does.

### AX caveat, found the hard way

`click` on a gesture `AXCheckBox` from AppleScript flips the AX value but
does NOT move the app's internal selection -- Qt's handler never runs. A
`clear` issued after such a click cleared a DIFFERENT gesture: the one the
human had last selected with a real mouse. Anything driving this app has to
use `perform action "AXPress"` (or real mouse events) and verify by reading
the titles back, never by assuming the click landed.

## EXPERIMENT 1 -- gesture to key id

Assigned one distinct MutiMedia action per zone, pressed **Save settings**,
read the log delta. Zone identity comes from the vendor's own icons: the top
two are rotation arrows, the centre is a press arrow, the bottom two repeat
the rotation arrows with an added underline (hold).

    03 fd 02 01 02 00 02 00 00 b7   <- Stop               -> top-left  (CCW)
    03 fd 03 01 02 00 02 00 00 6f   <- Screen brightness+ -> centre    (press)
    03 fd 04 01 02 00 02 00 00 cd   <- Play/Pause         -> top-right (CW)
    03 fd 05 01 02 00 02 00 00 70   <- Screen brightness- -> lower-left  (hold+twist L)
    03 fd 06 01 02 00 02 00 00 92   <- Calculator         -> lower-right (hold+twist R)
    03 fd fe ff                     <- commit, once, after all five

| Zone                | Gesture        | Key id |
|---------------------|----------------|--------|
| top-left            | twist CCW      | 2      |
| centre              | press          | 3      |
| top-right           | twist CW       | 4      |
| lower-left          | hold + twist L | 5      |
| lower-right         | hold + twist R | 6      |

**This is the mapping the first capture recorded, and this repo "corrected"
it away earlier the same night.** The correction was reasoned from a
read-back experiment -- writing `03 FD 02 ...` changes the record the `FA`
read reports as key 2, whose content matched what `config.yaml` calls button
2 -- and that reasoning assumed the layout in `config.yaml` (three buttons at
keys 1-3, knob at 4-6) was itself measured. It was not; it is a declaration
this repo wrote.

### The conflict this leaves, and the one test that settles it

Two mappings are now on the table for the SAME key ids:

* **Vendor**: keys 2-6 are the five knob gestures. No buttons anywhere in
  the vendor's UI for this device -- it draws five knob zones and nothing
  else.
* **This repo**: keys 1-3 are buttons, 4-6 are CCW/press/CW, from
  `key_id_for_knob(button_count = 3)`.

They cannot both be right, and they overlap at keys 4-6, so every knob
binding this tool has ever flashed may be on the wrong gesture.

Evidence is genuinely split. The calibrated `probe-gestures` run had its
control marker at key 4 and that marker fired on a CCW twist, which fits THIS
REPO's mapping. But both probe runs also captured `0x00B6` (key 2's content)
and never captured key 3's, which fits the vendor's mapping with the press
gesture not registering.

The test that settles it needs one gesture performed: the device now holds
five distinct usages at keys 2-6 (`b7 6f cd 70 92`), so

    antiknob listen --timeout-secs 20

and a single deliberate CCW twist names the key id outright. Until then,
NOTHING in the code changes on this point.

## Save writes only DIRTY records

The first save sent five records (all five gestures had changed) then one
commit. The next save, with one gesture changed, sent one record and one
commit. So the app tracks dirt per gesture; a full rewrite is not implied by
pressing Save.

## `View settings` is a device READ-BACK

Pressing it sends three reads and repopulates the UI from the replies:

    03 fa 19 00 01
    03 fa 19 00 02
    03 fa 19 00 03

Width `0x19` = 25, counters 1, 2, 3 -- one per LAYER, not one per slot. That
is the "groups 0x01..0x24 x counters 1-3 return the whole table" shape
PLAN.md records, and it is a whole config in three packets rather than this
repo's per-slot walk. Worth adopting: it is 3 reads instead of 18, and it is
what the vendor trusts to render its own UI.

This also gives a verification loop that does not need the app quit:
save -> View settings -> read the titles back.

## EXPERIMENT 2 -- chaining, and it did NOT round-trip

The record grows a length field but, as saved by these steps, only the LAST
action survived.

| Actions set in the UI            | Packet                                      |
|----------------------------------|---------------------------------------------|
| `Stop` (1)                       | `03 fd 02 01 02 00 02 00 00 b7`             |
| `Volume- Mute` (2)               | `03 fd 03 01 02 00 04 00 00 e2`             |
| `Volume+ Next track Mute` (3)    | `03 fd 02 01 02 00 06 00 00 e2`             |

Byte 6 is `2 x action count` -- a payload LENGTH in bytes, two per media
usage. Byte 9 carries one usage: always the LAST one clicked. Bytes 10
onward are zero.

`View settings` then reports the device holding `Mute` for both chained
gestures. So the chain really did not reach the device: this is not a
logging artefact.

**But chaining demonstrably works**, because the knob arrived with
`Volume+ Next track` bound to the lower-right zone, and the app rendered
that from a device read-back at launch. The user's own capture of it is:

    03 fd 06 01 02 01 04 00 00 b5      <- note byte 5 = 01, not 00

Every packet from MY saves has byte 5 = `00`. So byte 5 is the missing
piece -- most likely the index of the action within the sequence, with one
packet sent per action. If that is right, a two-action chain is two packets
(`byte5=00` then `byte5=01`) and my runs only ever produced the first.

Unresolved, and the next thing to test:

* does clicking a second action send a second packet on save, and my
  click sequence somehow collapsed the list?
* is byte 5 an index, and does the device append rather than replace?
* what does `clear` do to an existing multi-action record -- does the app
  need to rewrite index 0 before index 1 is accepted?

## THE RECORD FORMAT, decoded

    03 FD <key> <layer> <kind> <b5> <len> <entry 1> <entry 2> ... <entry 18>

    entry = <delay hi> <delay lo> <value>     (3 bytes, delay is 16-bit BIG-endian ms)

* `kind`  01 keyboard, 02 media, 03 mouse
* `len`   total payload BYTES, not entries: 1 per keyboard action, 2 per
          media action, 4 per mouse action
* 18 entries, matching the 18 spin boxes in `DelaySetting` exactly

Proven by typing into the delay boxes and reading the wire:

| delay boxes           | bytes 7..15                  |
|-----------------------|------------------------------|
| 123, 50, 50           | `00 7b 04  00 32 00  00 32 00` |
| 123, 50200, 509       | `00 7b 04  c4 18 00  01 fd 00` |

`50200 = 0xC418` and `509 = 0x01FD` -- the values arrived that way because a
triple-click failed to select the box contents and the typed digits appended
to the existing `50`. The accident is what proved the field is 16-bit
big-endian rather than one byte: a one-byte delay could not hold 50200.

The default `50` in every box from #2 on is why the very first captured
record was full of `00 32 00` groups. They were never padding; they are
seventeen empty slots each carrying a 50 ms default delay.

### Multi-byte actions span consecutive entries

A media usage is 16-bit and occupies TWO entries' value bytes, low first:

    Calculator (0x0192):  entry1.value = 92 , entry2.value = 01
    Stop       (0x00B7):  entry1.value = b7 , entry2.value = 00

So `len` counts bytes because an action is not one entry.

## Action encodings measured

### Keyboard (`kind 01`, 1 byte per action)

    BaseKeys "A"        -> value 0x04    (HID usage for `a`)
    Ctrl Shift Alt "F13"-> value 0x68    (HID usage for F13)

Both match this repo's `parse_keycode` table, so the keycodes are ordinary
USB HID usages and no translation is needed.

### Media (`kind 02`, 2 bytes per action)

16-bit consumer usage, low byte then high byte, across two entries.

### Mouse (`kind 03`, 4 bytes per action)

    Mouse LeftKey  -> byte 12 = 01
    Mouse Right    -> byte 12 = 02
    Mouse Middle   -> byte 12 = 04
    Mouse Wheel+   -> byte 21 = 01
    Mouse Wheel-   -> byte 21 = ff
    Like           -> kind 03, b5 = 04, byte 9 = 05

Note this is NOT the `0xFE` mouse layout, which puts the button at byte 12
and the wheel delta at byte 15. Under `0xFD` the wheel is at byte 21. That
difference is why `fd::build_packet` refusing mouse was right.

### RGB LED is NOT a gesture action

Clicking `LED Mode3` does not bind anything to the selected gesture -- it
sets the LAYER's backlight and sends the LED packets directly:

    03 fe b0 00 03      layer 0 <- mode 3
    03 fe b0 01 00      layers 1 and 2 always mode 0
    03 fe b0 02 00

The gesture's record came back `len = 00`, i.e. empty, and its title stayed
`NULL`. So the earlier note in this file calling "RGB LED as an action" a new
capability was wrong: it is the layer LED picker, sitting in the same panel.

### Swipes did not assign

`Left Swipe` clicked at its centre leaves the gesture `NULL` and produces an
empty record. `Like`, in the same panel, assigns normally. So the four swipe
entries are either not valid for a knob gesture or need something else. Open.

## Chaining DOES work -- for keyboard actions

Nineteen letters clicked in a row saved as one record:

    03 fd 02 01 01 00 13  00 00 04  00 00 05  00 00 07  00 00 08  00 00 09 ...
                      ^19    a         b         d         e         f

`len = 0x13 = 19`, and each entry carries one keycode. This is the capability
the whole exercise was after: a gesture can type a sequence.

**Media actions do NOT chain, and it is the DEVICE that refuses.** Clicking
two or three media actions leaves a title listing all of them but writes a
record the firmware truncates. Tested directly with this repo's own writer,
bypassing the app entirely:

    written : 03 FD 07 01 02 00 04  00 00 e9  00 00 00  00 00 92  00 00 01
    read    : 03 FA 07 01 02 00 02  00 00 e9  00 00 00  00 00 00

`len` came back 02 and the second action was dropped. The second usage was
Calculator (`0x0192`) precisely so its non-zero high byte ruled out
trailing-zero trimming. So a media slot holds exactly one action, and an
earlier note here guessing "the limit is the app, not the hardware" was
wrong. `fd::build_packet` now refuses a media sequence rather than letting
the firmware truncate one silently, which is what the vendor app allows.

### Capacity

The payload runs from byte 7 to byte 63: 57 bytes = **19 entries of 3 bytes**.
`len` counts value BYTES, and an action costs 1 byte (keyboard), 2 (media) or
4 (mouse). So one gesture holds at most:

| kind     | bytes/action | max actions | note                          |
|----------|--------------|-------------|-------------------------------|
| keyboard | 1            | 19          | the only kind that chains     |
| media    | 2            | 1           | firmware caps it, see below    |
| mouse    | 4            | 4 (untested)| encoding only partly measured |

### Modifiers are actions, not a bitmask

    F1 = Ctrl    F2 = Shift    F3 = Alt    F4 = Win/Cmd

They occupy an entry of their own and apply to what follows, so `Ctrl+C` is
the two-entry sequence `F1 06`. This is NOT the HID modifier bitmask the
`0xFE` path uses at byte 11, and it is why `Ctrl+` appears as a clickable
"key" in `BaseKeys` rather than as a checkbox.

Cross-check: Procreate's `Undo` encodes as `F4 1D` = Cmd+Z, which is the real
Procreate shortcut. So the Procreate category is nothing but named chords --
every one of its 31 macros is expressible with this vocabulary, and so is any
other application's shortcut set.

### Delays are per-gesture

Selecting a different gesture shows that gesture's own 18 delay boxes
(defaults `0, 50, 50, ...`). They are part of the record, not a global.

## What this unlocks

* **Type a string** -- 19 keystrokes per gesture, modifiers inline.
* **Drive an application** -- any chord sequence, with per-step delays, which
  is exactly how the vendor implements its Procreate menu.
* **Per-step timing** -- 16-bit ms delay on every entry, so a sequence can
  wait for a menu to open rather than racing it.

### The full modifier table

| Wire | Modifier   | HID bit | This repo's mask |
|------|------------|---------|------------------|
| `F1` | Ctrl       | 0       | `0x01`           |
| `F2` | Shift      | 1       | `0x02`           |
| `F3` | Alt        | 2       | `0x04`           |
| `F4` | Win / Cmd  | 3       | `0x08`           |
| `F5` | Right Ctrl | 4       | `0x10`           |
| `F6` | Right Shift| 5       | `0x20`           |
| `F7` | Right Alt  | 6       | `0x40`           |
| `F8` | Right Win  | 7       | `0x80`           |

`0xF1 + bit`, in HID modifier order. The `command` button is `F4`, the same
as `Win+`. So converting this repo's `Action::Key { modifiers, code }` into a
`0xFD` sequence is: one entry per set modifier bit, in bit order, then one
entry for the keycode. No new vocabulary is needed.

## Still open

* **Media chaining.** The app writes only the last media action. The device
  holds at least two (the knob arrived that way), so the limit is the app,
  not the hardware -- but this repo's own writer should be tested against the
  device directly before claiming media chains work.
* **Swipes.** `Left/Right/Up/Down Swipe` never assign to a knob gesture.
* **`Ctrl+Alt+` combo buttons** in the `Ctrl Shift Alt` category did not
  assign either, while the single modifiers in the same panel do.
* **Byte 5.** Mostly `00`; `01` when appending to an existing binding, `04`
  for `Like`. Not decoded, and nothing built here depends on it.
* **Layer.** Every packet observed used layer `01`. The vendor UI exposes no
  layer switch for this device.

## Verified with this repo's own writer

Written with `antiknob raw` and read back off the device:

    03 FD 02 01 01 00 03  00 00 04  00 00 05  00 00 06   -> a, b, c
    03 FD 03 01 01 00 02  00 00 04  01 f4 05             -> a, wait 500ms, b

Both echoed byte for byte. So the format is not merely observed in the
vendor's traffic, it is one this repo can produce.

### A write does NOT clear entries past `len`

Restoring a one-action binding over a three-action one left the third entry's
value in place, and a zero-filled 64-byte write did not remove it either --
the device updates only the first `len` bytes' worth of entries. Clearing
needs an explicit long write (`kind 01`, `len 18`, zero entries) BEFORE the
short one. Harmless in practice, because `len` bounds what the firmware
reads, but it means a slot dump can show bytes that no longer mean anything.

### Byte 5 is device-owned

It read `01` on every slot at session start and `00` after this work,
regardless of whether the last write was `0xFE` or `0xFD` and regardless of
what value was sent in that position. Nothing here writes or reads it.

## Device restored

`antiknob upload` reports 18/18 slots confirmed and every binding matches the
session baseline in kind, length and action.
