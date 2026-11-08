//! The `0xFD` slot writer -- the vendor's own keymap command.
//!
//! `Action::to_packet` writes slots with `0xFE`, one action per slot. The
//! vendor uses `0xFD`, whose record is a SEQUENCE with per-step timing:
//!
//! ```text
//! 03 FD <key_id> <layer> <kind> <b5> <len> <entry> <entry> ... 19 entries
//!
//! entry = <delay hi> <delay lo> <value>     delay is 16-bit BIG-endian ms
//! ```
//!
//! `len` counts value BYTES, not entries, because one action can span
//! several: a keycode is 1 byte, a media usage is 2 (low then high), a mouse
//! action is 4. The payload runs from byte 7 to byte 63, so a slot holds 19
//! entries -- 19 keystrokes.
//!
//! Only KEYBOARD records actually chain. A media record is capped at one
//! action by the firmware: writing two reads back as `len = 02` with the
//! second dropped, even when its high byte is non-zero so trailing-zero
//! trimming cannot explain it. That is a device limit, not a vendor bug --
//! the vendor app happily shows three media actions over a slot holding one.
//!
//! **Modifiers are entries, not a bitmask.** `0xF1..=0xF8` are Ctrl, Shift,
//! Alt, Win and their right-hand twins, in HID modifier bit order, and each
//! occupies an entry of its own before the key it applies to. `Ctrl+C` is
//! `F1 06`. That is why the vendor's UI lists `Ctrl+` as a clickable key
//! rather than a checkbox, and it differs from the `0xFE` path, which puts a
//! modifier BITMASK at byte 11.
//!
//! All of this was measured by driving ANTICATER.app under the
//! `tools/hidsnoop/` interposer, one change per save; the derivation is in
//! `VENDOR_UI_MAP.md`. An earlier version of this module GUESSED the layout
//! as `<kind> <n_groups> <group_kind>` with groups of `<00> <mods> <code>`.
//! It round-tripped for a single action by coincidence while writing the
//! modifier bitmask into the delay's high byte, so `ctrl-alt-f16` would have
//! typed F16 after a 5 ms pause with no modifiers held at all.
//!
//! **Mouse** follows the vendor's own builder, read statically out of
//! ANTICATER.app (`Widget::SetMousePage(int)` @0x1000477f0, records in
//! `tests/fixtures/vendor_static.json`): kind `03`, byte 5 `01` (a
//! mouse-report action; `04` is a touch gesture), `len` `04`, the button
//! mask at byte 12 (1 left, 2 right, 4 middle) and the wheel at byte 21
//! (`01` / `ff`). Those are the bytes the wire capture saw too. The vendor
//! clears the record before every mouse action, so a mouse record holds ONE
//! action: a mouse step inside a sequence is refused, as is any button or
//! wheel value the vendor never writes (X/Y movement at bytes 15/18 is not
//! written by anything in the vendor binary).
//!
//! **Touch gestures** are the same page and the same record with byte 5 =
//! `04` and the gesture at byte 9 (1 left, 2 right, 3 up, 4 down, 5 like):
//! `swipe-left`, `swipe-right`, `swipe-up`, `swipe-down`, `like`. One per
//! slot, no delay, any other code refused. The vendor app's own swipe
//! buttons never reach this record (a label mismatch drops the click), so
//! the swipe records are read from its code, not seen on the wire; `like`
//! is on the wire too. Whether the knob RUNS them is the owner's live
//! check (PLAN OW.78).

use crate::firmware::{
    FD_CMD, FD_ENTRY_LEN, FD_HEADER_LEN, FD_KIND_MOUSE, FD_MAX_ENTRIES, FD_MOD_ENTRY_BASE,
    MOUSE_CLASS_REPORT, MOUSE_CLASS_TOUCH, REPORT_ID, REPORT_LEN, TOUCH_GESTURES,
};
use crate::protocol::Action;
use anyhow::{anyhow, Result};

/// The modifier entry for HID modifier bit `n`, `FD_MOD_ENTRY_BASE + n`.
///
/// Measured one at a time: `Ctrl+` is `F1`, `Shift+` `F2`, `Alt+` `F3`,
/// `Win+` and `command` both `F4`, then `F5..F8` for the right-hand four.
#[must_use]
pub fn modifier_entry(bit: u8) -> u8 {
    FD_MOD_ENTRY_BASE + bit
}

/// What a record declares itself to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyboard = 1,
    Media = 2,
    Mouse = FD_KIND_MOUSE as isize,
}

impl Kind {
    fn of(action: &Action) -> Self {
        match action {
            Action::Key { .. } => Self::Keyboard,
            Action::Media(_) => Self::Media,
            Action::MouseClick { .. } | Action::MouseWheel { .. } | Action::Touch { .. } => {
                Self::Mouse
            }
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Media => "media",
            Self::Mouse => "mouse",
        }
    }
}

/// One action and how long to wait before running it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub action: Action,
    /// Milliseconds to wait before this step. The vendor defaults every step
    /// after the first to 50, which is what the `0x32` bytes filling its
    /// records are -- seventeen unused entries, not padding.
    pub delay_ms: u16,
}

impl Step {
    #[must_use]
    pub fn now(action: Action) -> Self {
        Self {
            action,
            delay_ms: 0,
        }
    }
}

/// The value bytes one action occupies, in wire order.
///
/// A chord becomes one entry per held modifier followed by the key, which is
/// how the vendor writes it and the only reason `Ctrl+` appears in its key
/// list at all.
fn value_bytes(action: &Action) -> Vec<u8> {
    match action {
        Action::Key { modifiers, code } => {
            let mut out: Vec<u8> = (0..8)
                .filter(|bit| modifiers & (1 << bit) != 0)
                .map(modifier_entry)
                .collect();
            out.push(*code);
            out
        }
        Action::Media(usage) => {
            let [low, high] = usage.to_le_bytes();
            vec![low, high]
        }
        // Never flattened: `build_packet` writes a mouse record whole.
        Action::MouseClick { .. } | Action::MouseWheel { .. } | Action::Touch { .. } => Vec::new(),
    }
}

/// The single record kind a sequence can be written as.
///
/// One record has one `kind` byte, so a sequence mixing keyboard and media
/// is not expressible. Refusing is the only honest answer: picking one would
/// write the other action's bytes into fields read under the wrong
/// interpretation, and the device would accept it.
fn plan_kind(steps: &[Step]) -> Result<Kind> {
    let mut kind: Option<Kind> = None;
    for step in steps {
        let this = Kind::of(&step.action);
        match kind {
            None => kind = Some(this),
            Some(first) if first == this => {}
            Some(first) => {
                return Err(anyhow!(
                    "one slot record holds one kind of action, but this sequence \
                     mixes {} and {}; split it across gestures",
                    first.describe(),
                    this.describe()
                ))
            }
        }
    }
    kind.ok_or_else(|| anyhow!("an empty sequence binds nothing; give at least one action"))
}

/// The `len` the vendor declares on every mouse record.
const MOUSE_LEN: u8 = 4;
/// Where the vendor puts the button mask and the wheel delta.
const MOUSE_BUTTON_AT: usize = 12;
const MOUSE_WHEEL_AT: usize = 21;
/// Where it puts a touch gesture's code.
const TOUCH_GESTURE_AT: usize = 9;

/// The one mouse record for `steps`, exactly as the vendor writes it
/// (`Widget::SetMousePage` @0x1000477f0, PHY2 cases `a`..`e`).
fn mouse_packet(key_id: u8, layer: u8, steps: &[Step]) -> Result<Vec<u8>> {
    let [step] = steps else {
        return Err(anyhow!(
            "a mouse action fills its slot alone: the vendor clears the record \
             before every mouse action, so a {}-step sequence has no record to \
             be written as. Split it across gestures",
            steps.len()
        ));
    };
    if step.delay_ms != 0 {
        return Err(anyhow!(
            "a mouse record carries no measured delay; bind it with no delay"
        ));
    }
    let (class, at, value) = match step.action {
        Action::MouseClick { button } if matches!(button, 1 | 2 | 4) => {
            (MOUSE_CLASS_REPORT, MOUSE_BUTTON_AT, button)
        }
        Action::MouseWheel { delta } if matches!(delta, 1 | -1) => {
            (MOUSE_CLASS_REPORT, MOUSE_WHEEL_AT, delta.to_le_bytes()[0])
        }
        Action::Touch { gesture } if TOUCH_GESTURES.contains(&gesture) => {
            (MOUSE_CLASS_TOUCH, TOUCH_GESTURE_AT, gesture)
        }
        ref other => {
            return Err(anyhow!(
                "{other:?} is not a mouse record the vendor writes (buttons 1, 2, 4 \
                 alone; wheel +1 or -1; touch gestures 1..=5), so its encoding is unknown"
            ))
        }
    };
    let mut packet = vec![0u8; REPORT_LEN];
    packet[0] = REPORT_ID;
    packet[1] = FD_CMD;
    packet[2] = key_id;
    packet[3] = layer + 1;
    packet[4] = FD_KIND_MOUSE;
    packet[5] = class;
    packet[6] = MOUSE_LEN;
    packet[at] = value;
    Ok(packet)
}

/// Build the 64-byte `0xFD` record binding one slot to a sequence.
///
/// `layer` is 0-based here and 1-based on the wire, matching
/// `Action::to_packet` so callers never have to remember which is which.
///
/// # Errors
///
/// Fails when the steps cannot be encoded: an empty or mixed sequence, or one longer than the packet holds.
pub fn build_packet(key_id: u8, layer: u8, steps: &[Step]) -> Result<Vec<u8>> {
    let kind = plan_kind(steps)?;
    if kind == Kind::Mouse {
        return mouse_packet(key_id, layer, steps);
    }

    // Flatten to entries. A chord contributes one per modifier plus one for
    // the key, and the step's delay belongs to the FIRST of them: the wait
    // happens before the chord, not between its modifiers.
    let mut entries: Vec<(u16, u8)> = Vec::new();
    for step in steps {
        for (i, value) in value_bytes(&step.action).into_iter().enumerate() {
            entries.push((if i == 0 { step.delay_ms } else { 0 }, value));
        }
    }

    // The device caps a media record at ONE action. Writing two, with the
    // second's high byte deliberately non-zero so trailing-zero trimming
    // could not explain it, read back as `len = 02` with the second dropped.
    // So a media chain is refused rather than silently truncated by the
    // firmware -- which is exactly what the vendor app lets happen, showing
    // three actions in its UI over a slot holding one.
    if kind == Kind::Media && steps.len() > 1 {
        return Err(anyhow!(
            "this device stores one media action per slot: writing {} reads \
             back as one, with the rest dropped. Use keyboard actions for a \
             sequence, or split these across gestures",
            steps.len()
        ));
    }

    let entry_count = u8::try_from(entries.len())
        .ok()
        .filter(|n| usize::from(*n) <= FD_MAX_ENTRIES);
    let Some(entry_count) = entry_count else {
        return Err(anyhow!(
            "this sequence needs {} entries but one slot holds {}; a truncated \
             sequence would flash as a success and run the wrong thing. Note a \
             chord costs one entry per modifier plus one for the key",
            entries.len(),
            FD_MAX_ENTRIES
        ));
    };

    let mut packet = vec![0u8; REPORT_LEN];
    packet[0] = REPORT_ID;
    packet[1] = FD_CMD;
    packet[2] = key_id;
    packet[3] = layer + 1;
    packet[4] = kind as u8;
    // Byte 5 is stored as written (measured 2026-11-01). The vendor app
    // writes `01` when appending and `04` for `Like`; this writer never
    // appends, so it writes `00`, which is what its records read back with.
    packet[5] = 0;
    packet[6] = entry_count;

    for (i, (delay, value)) in entries.iter().enumerate() {
        let at = FD_HEADER_LEN + i * FD_ENTRY_LEN;
        packet[at] = (delay >> 8) as u8;
        packet[at + 1] = (delay & 0xFF) as u8;
        packet[at + 2] = *value;
    }

    Ok(packet)
}

/// A record that zeroes every entry, to be sent BEFORE a shorter one.
///
/// The device updates only the first `len` bytes' worth of entries and
/// leaves the rest, so replacing a long sequence with a short one strands
/// the tail of the old one in the slot. `len` bounds what the firmware
/// reads, so the residue is inert -- but a slot dump then shows bytes that
/// mean nothing, which is exactly the sort of thing this repo has misread
/// before.
#[must_use]
pub fn wipe_packet(key_id: u8, layer: u8) -> Vec<u8> {
    let mut packet = vec![0u8; REPORT_LEN];
    packet[0] = REPORT_ID;
    packet[1] = FD_CMD;
    packet[2] = key_id;
    packet[3] = layer + 1;
    packet[4] = Kind::Keyboard as u8;
    packet[6] = u8::try_from(FD_MAX_ENTRIES - 1).unwrap_or(u8::MAX);
    packet
}

/// The wipe that must precede each write in `packets`: one per distinct
/// slot, for whichever command wrote it.
///
/// `upload` used to derive these through `parse_record`, which reads only
/// the `0xFE` layout and so skipped every keyboard and media write -- now
/// `0xFD` -- leaving a short binding written over a long one with the old
/// tail stranded in the slot. Addressing is the one thing all commands
/// agree on, so it goes through `record_addr`.
#[must_use]
pub fn wipes_for(packets: &[Vec<u8>]) -> Vec<Vec<u8>> {
    packets
        .iter()
        .filter_map(|p| crate::device::verify::record_addr(p))
        .map(|addr| wipe_packet(addr.key_id, addr.layer.saturating_sub(1)))
        .collect()
}

/// How many bytes of a record carry the binding.
fn significant_len(record: &[u8]) -> Option<usize> {
    let entries = *record.get(6)? as usize;
    let end = FD_HEADER_LEN + entries * FD_ENTRY_LEN;
    (end <= record.len()).then_some(end)
}

/// Does the record read back off the device carry the binding we sent?
///
/// Address-only verification is not verification. A check that asks "did a
/// record for key 7 come back?" answers yes whatever the device stored,
/// which is the same false confidence as the `hid_write` return value that
/// let knob bindings flash into dead slots for months.
///
/// The WHOLE record is compared, every byte but byte 1 (the command: `0xFD`
/// or `0xFE` out, `0xFA` back). Byte 5 was once skipped as "device-owned"
/// and is stored as written (`VENDOR_UI_MAP`, "Byte 5 is stored as written").
/// The tail past `len` was skipped as inert, and it is where a media write
/// leaves the previous record's entries (`VENDOR_UI_MAP`, "A write does NOT
/// clear entries past `len`"): a slot that kept a stale tail is a slot
/// holding something other than what was sent, and a comparison that stops
/// at `len` calls it confirmed. Any command's record goes through here, so
/// one rule decides what "read back as written" means.
#[must_use]
pub fn record_matches(sent: &[u8], readback: &[u8]) -> bool {
    if sent.len() != REPORT_LEN || readback.len() != REPORT_LEN {
        return false;
    }
    // A `0xFD` record must declare a count that fits; one that does not was
    // never a record, and two copies of the same garbage are not a match.
    if sent[1] == FD_CMD && significant_len(sent).is_none() {
        return false;
    }
    sent[0] == readback[0] && sent[2..] == readback[2..]
}

/// The write that puts a record read off the device back exactly as it was.
///
/// An `FA` record is the STORED layout -- `len` at byte 6, entries from byte
/// 7 -- and that is the `0xFD` layout, so a restore is the record replayed
/// with `0xFD` and every other byte unchanged. The gesture probe's restore
/// used `0xFE`, the command whose single-action records the firmware stores
/// and has been seen not to RUN (`VENDOR_UI_MAP`, "The `0xFE` single-action
/// record was never executable"). The read-back cannot tell the two apart:
/// measured 2026-11-01, the same record written with either command reads
/// back byte for byte the same. So the command is decided here, once, by
/// what the firmware runs, and no read-back is asked to settle it.
#[must_use]
pub fn restore_packet(record: &[u8]) -> Vec<u8> {
    let mut p = record.to_vec();
    p.resize(REPORT_LEN, 0);
    p[1] = FD_CMD;
    p
}

/// One slot record read the way the firmware stores it: a kind and the
/// value bytes of its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    pub key_id: u8,
    pub layer: u8,
    pub kind: u8,
    pub values: Vec<u8>,
}

/// Read the entry chain out of a record: the `0xFD` write or the `0xFA`
/// reply that echoes it.
///
/// The one reader of the stored layout. The `0xFE` layout put a modifier
/// bitmask at byte 11 and a keycode at byte 12; under `0xFD` those bytes
/// are delays and values, so any reader that took them from there saw a
/// chord as noise. `len` (byte 6) counts the entries, and a record whose
/// count runs past the report is not a record.
#[must_use]
pub fn decode_chain(record: &[u8]) -> Option<Chain> {
    let addr = crate::device::verify::record_addr(record)?;
    let end = significant_len(record)?;
    let values = record[FD_HEADER_LEN..end]
        .as_chunks::<FD_ENTRY_LEN>()
        .0
        .iter()
        .map(|entry| entry[FD_ENTRY_LEN - 1])
        .collect();
    Some(Chain {
        key_id: addr.key_id,
        layer: addr.layer,
        kind: record[4],
        values,
    })
}

/// A chord as `(modifier bitmask, keycode)`, read out of a keyboard
/// chain's values: modifier markers `F1..F8` then the key, the inverse of
/// the chord encoding in `value_bytes`. `None` for anything else.
#[must_use]
pub fn chord_of(values: &[u8]) -> Option<(u8, u8)> {
    let (code, markers) = values.split_last()?;
    let mut mods = 0u8;
    for marker in markers {
        let bit = marker.checked_sub(FD_MOD_ENTRY_BASE).filter(|b| *b < 8)?;
        mods |= 1 << bit;
    }
    Some((mods, *code))
}

/// Parse a list of action names into steps, all with no delay.
///
/// # Errors
///
/// Fails when a name does not parse as an action.
pub fn parse_sequence(specs: &[String]) -> Result<Vec<Step>> {
    specs
        .iter()
        .map(|s| Action::parse(s).map(Step::now))
        .collect()
}

#[cfg(test)]
#[path = "fd_tests.rs"]
mod fd_tests;
