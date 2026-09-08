//! Reading the slot table back off the device.
//!
//! Split from `device/mod.rs` for the file-length gate, along a real seam:
//! everything here asks the device what it holds and nothing here changes
//! it. Two shapes, and the difference matters:
//!
//! * `read_slot` walks one slot at a time at a given width. Proven, and what
//!   `upload --verify` uses.
//! * `read_full_table` uses the vendor's BURST: one query answers with many
//!   records, so three queries return the whole 75-slot table. Faster, and
//!   it sees keys the walk cannot reach.

use super::{send_report, HidDevice};
use anyhow::{Context, Result};

/// Slot-table read query, reverse-engineered from the vendor app's
/// `Widget::read_Hidkey_Data`: write `[FA group 00 counter]` (report 0x03)
/// and read back the 64-byte slot dump. Read-only; changes no device state.
/// `group` selects the slot bank (`0x0F` / `0x19` observed), `counter`
/// walks entries 1..=3 within the bank.
pub fn read_slot(dev: &HidDevice, group: u8, counter: u8) -> Result<Vec<u8>> {
    let mut payload = [0u8; 64];
    payload[0] = 0xFA;
    payload[1] = group;
    payload[2] = 0x00;
    payload[3] = counter;
    send_report(dev, &payload)?;

    let mut buf = [0u8; 64];
    let n = dev
        .read_timeout(&mut buf, 500)
        .context("Timed out reading slot dump from device")?;
    Ok(buf[..n].to_vec())
}

/// Slots one burst query returns, and the widest the firmware answers.
pub const BURST_WIDTH: u8 = 25;

/// How many queries cover the whole table. The vendor sends exactly three.
pub const BURST_QUERIES: u8 = 3;

/// Read the WHOLE slot table: every key, every layer, in three queries.
///
/// `FA <width> 00 <n>` does not answer with one record, it answers with a
/// BURST of `width` of them. Reading once after the query -- which is what
/// every probe here used to do -- makes a burst look like a single record
/// and is why this was mistaken for a per-slot read for so long.
///
/// Three queries at width 25 return 75 distinct `(key, layer)` records,
/// which is the complete table. That is both faster than the per-slot walk
/// (3 queries instead of 18) and STRICTLY more complete: the walk at a
/// layout's own width never looks past key 6, so the media bindings an old
/// version of this tool wrote to keys 16-18 sat there unnoticed. They are
/// still there, and this read finds them.
///
/// Records are deduplicated by address, keeping the first seen, because the
/// three bursts overlap.
pub fn read_full_table(dev: &HidDevice) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut seen: Vec<(u8, u8)> = Vec::new();
    for counter in 1..=BURST_QUERIES {
        let mut payload = [0u8; 64];
        payload[0] = 0xFA;
        payload[1] = BURST_WIDTH;
        payload[3] = counter;
        if send_report(dev, &payload).is_err() {
            continue;
        }
        let mut buf = [0u8; 64];
        for _ in 0..BURST_WIDTH {
            match dev.read_timeout(&mut buf, 500) {
                Ok(n) if n > 0 => {
                    let record = buf[..n].to_vec();
                    if record.len() > 3 {
                        let addr = (record[2], record[3]);
                        if !seen.contains(&addr) {
                            seen.push(addr);
                            out.push(record);
                        }
                    }
                }
                // A short burst ends the query rather than failing it: the
                // count is what the device chooses to send, not a promise.
                _ => break,
            }
        }
    }
    out
}
