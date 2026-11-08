//! The one way a slot is written: wipe, write, commit, read back, compare.
//!
//! `hid_write` returning says the packet was well-formed, never that the
//! slot holds it -- knob bindings went to slots the firmware never reads for
//! months with a success line each time. `bind-seq` and `upload` learned to
//! read back; `bind-slots`, the daemon's `bind_slots` and its keymap flash
//! did not, and each printed a count of packets sent as though it were a
//! count of slots changed. Every slot write now goes through here, so the
//! read-back is not a choice a caller can forget (`tests/slot_write_confirmed.rs`
//! fails on a commit sent from anywhere else).

use super::verify::{summarize, verify, SlotAddr, SlotVerdict};
use super::{read_full_table, send_commit, send_report, HidDevice};
use crate::firmware::{SLOT_VERIFY_SETTLE_MS, SLOT_WRITE_GAP_MS};
use anyhow::{anyhow, Result};
use std::thread::sleep;
use std::time::Duration;

/// What a write left on the device, judged against what was sent.
#[derive(Debug, Clone)]
pub struct SlotWrite {
    /// The whole table as read after the write.
    pub table: Vec<Vec<u8>>,
    /// One verdict per record written, in write order.
    pub verdicts: Vec<(SlotAddr, SlotVerdict)>,
}

impl SlotWrite {
    /// How many written records read back exactly as sent.
    #[must_use]
    pub fn confirmed(&self) -> usize {
        self.verdicts
            .iter()
            .filter(|(_, v)| *v == SlotVerdict::Confirmed)
            .count()
    }

    /// Any slot that read back as something else.
    #[must_use]
    pub fn any_mismatched(&self) -> bool {
        self.verdicts
            .iter()
            .any(|(_, v)| *v == SlotVerdict::Mismatched)
    }

    /// `Ok` only when every written record read back as sent.
    ///
    /// # Errors
    ///
    /// Fails when any written record did not read back as sent.
    pub fn ensure_confirmed(&self) -> Result<()> {
        match summarize(&self.verdicts) {
            None => Ok(()),
            Some(problem) => Err(anyhow!(
                "{problem} ({}/{} confirmed)",
                self.confirmed(),
                self.verdicts.len()
            )),
        }
    }
}

/// Write `packets`, commit them, and read the whole table back.
///
/// Each slot is wiped first: a media write updates only its first `len`
/// entries, so a short record over a long one keeps the old tail, and the
/// whole-record comparison would then (rightly) call it a mismatch. The
/// wipes are committed before the writes, as the vendor app does.
///
/// Returns the verdicts rather than failing on a mismatch, so a caller that
/// must clean up (the gesture probe restoring its markers) still can; call
/// `ensure_confirmed` to turn them into an error.
///
/// # Errors
///
/// Fails when a packet cannot be sent; a record that reads back differently is a verdict, not an error.
pub fn write_slots(dev: &HidDevice, packets: &[Vec<u8>]) -> Result<SlotWrite> {
    let wipes = crate::fd::wipes_for(packets);
    for wipe in &wipes {
        send_report(dev, wipe)?;
        sleep(Duration::from_millis(SLOT_WRITE_GAP_MS));
    }
    if !wipes.is_empty() {
        send_commit(dev)?;
    }
    for packet in packets {
        send_report(dev, packet)?;
        sleep(Duration::from_millis(SLOT_WRITE_GAP_MS));
    }
    if !packets.is_empty() {
        send_commit(dev)?;
    }
    sleep(Duration::from_millis(SLOT_VERIFY_SETTLE_MS));
    let table = read_full_table(dev);
    let verdicts = verify(packets, &table);
    Ok(SlotWrite { table, verdicts })
}
