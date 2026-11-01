//! The knob's backlight, over the wire: one write, one framed reply.
//!
//! Split from `mod.rs` for the file-length cap, along the seam these two
//! share and share with nothing else -- an LED write is only meaningful
//! paired with the read that confirms it, and the whole reason this file
//! exists as a unit is that the two disagreed. `read_led_mode` returned byte
//! 2 of whatever report arrived next, and after a write that is the device
//! answering the LED init packet, whose byte 2 is zero. So a successful
//! write to mode 1 read back as "off" -- a verifier reporting failure for a
//! success, measured on a `514c:8850` on 2026-09-08.

use super::{send_report, HidDevice};
use crate::firmware::{
    LED_INIT_SETTLE_MS, LED_MAX_SKIPPED_REPORTS, LED_QUERY_CMD, LED_QUERY_SUB, LED_READ_TIMEOUT_MS,
    LED_REPLY_TAG, REPORT_LEN,
};
use anyhow::{Context, Result};

/// The LED mode a report carries, or `None` when it is not a reply to the
/// vendor query at all.
///
/// Pure, so the one thing that actually went wrong here is testable without
/// a knob: the device's answer to the LED init packet (`03 FB 00 01 0B ...`)
/// was being read as a mode, and its byte 2 is zero.
fn led_mode_of(report: &[u8]) -> Option<u8> {
    (report.len() >= 3 && report[1] == LED_REPLY_TAG).then(|| report[2])
}

/// LED state read-back, mirroring the vendor app's
/// `Widget::Read_RgbLed_DataDsp`: query `[FA B0 layer]` (report 0x03). The
/// reply is framed `03 FA <mode> ...`, and byte 2 is the layer's current
/// mode. Read-only.
///
/// The reply is MATCHED, not merely awaited. This used to return byte 2 of
/// whatever report arrived next, which is only the answer when the handle's
/// input queue happens to be empty. Read back on the handle that just wrote
/// an LED packet and the queue is not empty: the device answers the init
/// packet with `03 FB 00 01 0B ...` first, whose byte 2 is 0x00 -- so a
/// perfectly successful write to mode 1 read back as "off", every time,
/// reproducibly, on real hardware.
///
/// That is the worst shape a bug can take: a verifier that reports failure
/// for a success. A caller doing write-then-verify would have concluded the
/// firmware ignores writes, which is exactly the wrong conclusion this
/// device has already caused once.
pub fn read_led_mode(dev: &HidDevice, layer: u8) -> Result<u8> {
    let mut payload = [0u8; REPORT_LEN];
    payload[0] = LED_QUERY_CMD;
    payload[1] = LED_QUERY_SUB;
    payload[2] = layer;
    send_report(dev, &payload)?;

    for _ in 0..LED_MAX_SKIPPED_REPORTS {
        let mut buf = [0u8; REPORT_LEN];
        let n = dev
            .read_timeout(&mut buf, LED_READ_TIMEOUT_MS as i32)
            .context("Timed out reading LED state from device")?;
        if n < 3 {
            anyhow::bail!("Short LED reply ({} bytes)", n);
        }
        if let Some(mode) = led_mode_of(&buf[..n]) {
            return Ok(mode);
        }
    }
    anyhow::bail!(
        "No LED reply from the device after {} report(s)",
        LED_MAX_SKIPPED_REPORTS
    )
}

/// Send an LED packet, preceded by the init this firmware requires.
///
/// Without `03 FB FB FB` first, a `514c:8850` ACCEPTS the LED write, stores
/// the mode, reads it back correctly, and changes nothing. A read-back that
/// confirms a write and says nothing about its effect is the worst kind of
/// instrument, and it cost most of a debugging session before the vendor
/// app's own traffic showed the init going out on connect.
///
/// Documented from hardware testing in kriomant/ch57x-keyboard-tool#173, and
/// confirmed here: mode 0 turned this knob's light off only once the init
/// preceded it, having done nothing for hours before.
///
/// No commit follows. The vendor app sends none after LED writes, and the
/// keymap commit (`FD FE FF`) is a different instruction.
fn send_led(dev: &HidDevice, packet: &[u8]) -> Result<()> {
    send_report(dev, &crate::led::led_init_packet())?;
    std::thread::sleep(std::time::Duration::from_millis(LED_INIT_SETTLE_MS));
    send_report(dev, packet)?;
    Ok(())
}

/// What an LED write's read-back found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedVerdict {
    /// The layer reads back the mode that was sent.
    Confirmed { layer: u8, mode: u8 },
    /// The layer holds a different mode from the one sent.
    Mismatched { layer: u8, sent: u8, stored: u8 },
    /// The read-back itself failed, so nothing is known about the write.
    Unread { layer: u8, sent: u8, reason: String },
}

impl LedVerdict {
    /// One line for a person: what the device holds, never "OK".
    pub fn describe(&self) -> String {
        match self {
            Self::Confirmed { layer, mode } => {
                format!("layer {layer} LED reads back mode {mode}, as sent")
            }
            Self::Mismatched {
                layer,
                sent,
                stored,
            } => format!("layer {layer} LED was sent mode {sent} but reads back mode {stored}"),
            Self::Unread {
                layer,
                sent,
                reason,
            } => format!("layer {layer} LED was sent mode {sent}; the read-back failed: {reason}"),
        }
    }

    /// The mode the layer holds, when the read-back answered.
    pub fn stored(&self) -> Option<u8> {
        match self {
            Self::Confirmed { mode, .. } => Some(*mode),
            Self::Mismatched { stored, .. } => Some(*stored),
            Self::Unread { .. } => None,
        }
    }
}

/// Byte offsets of an LED write packet (`led::build_led_packet`).
const LED_PACKET_LAYER: usize = 3;
const LED_PACKET_MODE: usize = 4;

/// Compare what was sent with what the layer read back. Pure.
pub fn led_verdict(packet: &[u8], read: &Result<u8>) -> LedVerdict {
    let layer = packet.get(LED_PACKET_LAYER).copied().unwrap_or(0);
    let sent = packet.get(LED_PACKET_MODE).copied().unwrap_or(0);
    match read {
        Ok(stored) if *stored == sent => LedVerdict::Confirmed { layer, mode: sent },
        Ok(stored) => LedVerdict::Mismatched {
            layer,
            sent,
            stored: *stored,
        },
        Err(e) => LedVerdict::Unread {
            layer,
            sent,
            reason: format!("{e:#}"),
        },
    }
}

/// THE LED writer: init, write, then read the layer's mode back on the same
/// handle and compare. `send_led` is private so no path can write an LED
/// mode and report nothing about it (`tests/led_write_confirmed.rs`).
pub fn write_led(dev: &HidDevice, packet: &[u8]) -> Result<LedVerdict> {
    if packet.len() <= LED_PACKET_MODE {
        anyhow::bail!("LED packet too short ({} bytes)", packet.len());
    }
    send_led(dev, packet)?;
    let read = read_led_mode(dev, packet[LED_PACKET_LAYER]);
    Ok(led_verdict(packet, &read))
}

/// Fail on any LED write that did not read back as sent, naming each one.
/// Shared by `upload` and the daemon's keymap flash, so the two cannot
/// disagree about what an unconfirmed LED write is.
pub fn ensure_leds_confirmed(leds: &[LedVerdict]) -> Result<()> {
    let bad: Vec<String> = leds
        .iter()
        .filter(|v| !matches!(v, LedVerdict::Confirmed { .. }))
        .map(LedVerdict::describe)
        .collect();
    if bad.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("LED write not confirmed: {}", bad.join("; "))
    }
}

#[cfg(test)]
mod led_verdict_tests {
    use super::*;

    fn packet(layer: u8, mode: u8) -> Vec<u8> {
        let mut p = vec![0u8; REPORT_LEN];
        p[LED_PACKET_LAYER] = layer;
        p[LED_PACKET_MODE] = mode;
        p
    }

    /// The fake device's three answers: the sent mode, another mode, none.
    #[test]
    fn a_read_back_confirms_names_a_mismatch_or_says_it_could_not_read() {
        assert_eq!(
            led_verdict(&packet(1, 4), &Ok(4)),
            LedVerdict::Confirmed { layer: 1, mode: 4 }
        );
        let wrong = led_verdict(&packet(2, 1), &Ok(0));
        assert_eq!(
            wrong,
            LedVerdict::Mismatched {
                layer: 2,
                sent: 1,
                stored: 0
            }
        );
        assert!(
            wrong.describe().contains("reads back mode 0"),
            "{}",
            wrong.describe()
        );
        let unread = led_verdict(&packet(0, 2), &Err(anyhow::anyhow!("timed out")));
        assert_eq!(unread.stored(), None);
        assert!(
            unread.describe().contains("timed out"),
            "{}",
            unread.describe()
        );
    }
}

#[cfg(test)]
mod led_reply_tests {
    use super::*;

    /// The exact bytes read off a `514c:8850` on 2026-09-08, in the order
    /// the handle offered them after an LED write.
    ///
    /// `03 FB ...` is the device answering the LED INIT packet; `03 FA ...`
    /// is the answer to the mode query. `read_led_mode` took byte 2 of
    /// whichever arrived first, so a write of mode 2 read back as 0 -- a
    /// verifier reporting failure for a success, which is worse than no
    /// verifier at all.
    const INIT_ECHO: [u8; 16] = [
        0x03, 0xFB, 0x00, 0x01, 0x0B, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0x80, 0x30, 0xFF, 0xFF, 0x30,
        0x00,
    ];
    const MODE_REPLY_GREEN: [u8; 16] = [
        0x03, 0xFA, 0x02, 0xFF, 0x00, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0x00, 0x00,
        0xFF,
    ];

    #[test]
    fn the_init_echo_is_not_a_mode() {
        assert_eq!(led_mode_of(&INIT_ECHO), None);
    }

    #[test]
    fn the_query_reply_carries_the_mode() {
        assert_eq!(led_mode_of(&MODE_REPLY_GREEN), Some(2));
    }

    /// Draining the queue must reach the reply rather than stop at the echo.
    #[test]
    fn the_reply_is_found_past_the_echo() {
        let queued: Vec<&[u8]> = vec![&INIT_ECHO, &MODE_REPLY_GREEN];
        let found = queued.iter().find_map(|r| led_mode_of(r));
        assert_eq!(found, Some(2));
    }

    /// Mode 0 is a real mode -- "off" -- and it is also the value the bug
    /// produced. A reply saying 0 must still read as 0, or the fix would
    /// only work for the modes that are not the failure's own answer.
    #[test]
    fn off_is_a_mode_and_not_a_failure() {
        let mut off = MODE_REPLY_GREEN;
        off[2] = 0;
        assert_eq!(led_mode_of(&off), Some(0));
    }

    #[test]
    fn a_truncated_report_carries_nothing() {
        assert_eq!(led_mode_of(&[0x03, 0xFA]), None);
        assert_eq!(led_mode_of(&[]), None);
    }
}
