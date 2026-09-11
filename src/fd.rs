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
//! Mouse actions are refused. Their four bytes do not sit where the `0xFE`
//! layout puts them -- the button mask lands at byte 12 and the wheel delta
//! at byte 21 -- and only the buttons (1 left, 2 right, 4 middle) and the
//! wheel (`01` / `ff`) have been measured, not the remaining two bytes.

use crate::firmware::{
    FD_CMD, FD_ENTRY_LEN, FD_HEADER_LEN, FD_MAX_ENTRIES, FD_MOD_ENTRY_BASE, REPORT_ID, REPORT_LEN,
};
use crate::protocol::Action;
use anyhow::{anyhow, Result};

/// The modifier entry for HID modifier bit `n`, `FD_MOD_ENTRY_BASE + n`.
///
/// Measured one at a time: `Ctrl+` is `F1`, `Shift+` `F2`, `Alt+` `F3`,
/// `Win+` and `command` both `F4`, then `F5..F8` for the right-hand four.
pub fn modifier_entry(bit: u8) -> u8 {
    FD_MOD_ENTRY_BASE + bit
}

/// What a record declares itself to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyboard = 1,
    Media = 2,
}

impl Kind {
    fn of(action: &Action) -> Option<Self> {
        match action {
            Action::Key { .. } => Some(Self::Keyboard),
            Action::Media(_) => Some(Self::Media),
            Action::MouseClick { .. } | Action::MouseWheel { .. } => None,
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Media => "media",
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
        // Unreachable: `plan_kind` refuses mouse before we get here.
        Action::MouseClick { .. } | Action::MouseWheel { .. } => Vec::new(),
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
        let this = Kind::of(&step.action).ok_or_else(|| {
            anyhow!(
                "the 0xFD writer has no measured encoding for mouse actions; \
                 their bytes do not sit where the 0xFE layout puts them \
                 (button at byte 12, wheel at byte 21). Bind mouse actions \
                 with the 0xFE path instead"
            )
        })?;
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

/// Build the 64-byte `0xFD` record binding one slot to a sequence.
///
/// `layer` is 0-based here and 1-based on the wire, matching
/// `Action::to_packet` so callers never have to remember which is which.
pub fn build_packet(key_id: u8, layer: u8, steps: &[Step]) -> Result<Vec<u8>> {
    let kind = plan_kind(steps)?;

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

    if entries.len() > FD_MAX_ENTRIES {
        return Err(anyhow!(
            "this sequence needs {} entries but one slot holds {}; a truncated \
             sequence would flash as a success and run the wrong thing. Note a \
             chord costs one entry per modifier plus one for the key",
            entries.len(),
            FD_MAX_ENTRIES
        ));
    }

    let mut packet = vec![0u8; REPORT_LEN];
    packet[0] = REPORT_ID;
    packet[1] = FD_CMD;
    packet[2] = key_id;
    packet[3] = layer + 1;
    packet[4] = kind as u8;
    // Byte 5 is device-owned: it reads back the same whatever is sent here.
    packet[5] = 0;
    packet[6] = entries.len() as u8;

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
pub fn wipe_packet(key_id: u8, layer: u8) -> Vec<u8> {
    let mut packet = vec![0u8; REPORT_LEN];
    packet[0] = REPORT_ID;
    packet[1] = FD_CMD;
    packet[2] = key_id;
    packet[3] = layer + 1;
    packet[4] = Kind::Keyboard as u8;
    packet[6] = FD_MAX_ENTRIES as u8 - 1;
    packet
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
/// Byte 1 is skipped because it is `0xFD` out and `0xFA` back, and byte 5
/// because the device owns it -- it reads back the same regardless of what
/// was sent in that position.
pub fn record_matches(sent: &[u8], readback: &[u8]) -> bool {
    let Some(end) = significant_len(sent) else {
        return false;
    };
    if significant_len(readback) != Some(end) {
        return false;
    }
    sent.first() == readback.first()
        && sent[2..5] == readback[2..5]
        && sent[6..end] == readback[6..end]
}

/// Parse a list of action names into steps, all with no delay.
pub fn parse_sequence(specs: &[String]) -> Result<Vec<Step>> {
    specs
        .iter()
        .map(|s| Action::parse(s).map(Step::now))
        .collect()
}

#[cfg(test)]
#[path = "fd_tests.rs"]
mod fd_tests;
