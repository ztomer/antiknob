//! The hardware map: every firmware address and value in one place.
//!
//! A knob publishes several HID interfaces and answers a small binary
//! protocol; the bytes, pages, IDs, offsets, counts, and exchange timings
//! below are that protocol as measured against real hardware (plus the
//! vendor's own captures and kriomant/ch57x-keyboard-tool#173/#175). They
//! used to live beside the code that sends them, which meant one value
//! could be restated in two places and a new hardware revision was a
//! treasure hunt. Now there is one table to audit against a capture and
//! one diff when the firmware moves.
//!
//! The rule, so this does not rot back: a constant that describes the
//! HARDWARE or the wire goes here, with a doc line naming the exchange it
//! belongs to. Process timings (poll ticks, reconnect backoffs, socket
//! timeouts), host-side vocabularies (macOS keycodes, action names), and
//! test fixtures stay where they are -- centralizing those would imply
//! relationships that do not exist.
//!
//! Timings are per-exchange budgets, not one global settle: the 20ms the
//! LED init needs has nothing to do with the 15ms a slot write needs, and
//! sharing a constant would say otherwise.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Reports: every exchange is one 64-byte report behind ID 0x03.
// ---------------------------------------------------------------------------

/// Report ID on every packet this tool sends and every reply it matches.
pub const REPORT_ID: u8 = 0x03;

/// Payload bytes per report. Packet builders size from this; the HID write
/// itself adds the report ID on top (`REPORT_LEN + 1` on the wire).
pub const REPORT_LEN: usize = 64;

// ---------------------------------------------------------------------------
// Identity: which VID/PID pairs are ours, and the endpoint commands go to.
// ---------------------------------------------------------------------------

/// How a supported device is attached. Decided per device in
/// `device::classify`, not per interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    #[serde(rename = "usb")]
    Usb,
    #[serde(rename = "wireless_2_4g")]
    Wireless24G,
    #[serde(rename = "bluetooth")]
    Bluetooth,
}

impl TransportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Usb => "usb",
            Self::Wireless24G => "wireless_2_4g",
            Self::Bluetooth => "bluetooth",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Usb => "USB (Wired)",
            Self::Wireless24G => "2.4GHz Wireless",
            Self::Bluetooth => "Bluetooth Wireless",
        }
    }
}

/// Every VID/PID this tool drives: `(vendor, product, name, transport)`.
///
/// A knob publishes several HID interfaces (keyboard, consumer, mouse,
/// vendor); this table matches physical devices, and the vendor endpoint
/// below picks the interface commands go to.
pub const SUPPORTED_DEVICES: &[(u16, u16, &str, TransportType)] = &[
    (
        0x514C,
        0x8850,
        "Anticater / LQKJ VK01 (0x514c:0x8850)",
        TransportType::Usb,
    ),
    (
        0x514C,
        0x8851,
        "Anticater / LQKJ 2.4G (0x514c:0x8851)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8840,
        "Anticater / CH57x (0x1189:0x8840)",
        TransportType::Usb,
    ),
    (
        0x1189,
        0x8842,
        "Anticater / CH57x (0x1189:0x8842)",
        TransportType::Usb,
    ),
    (
        0x1189,
        0x8850,
        "Anticater / CH57x (0x1189:0x8850)",
        TransportType::Usb,
    ),
    (
        0x1189,
        0x8851,
        "Anticater / CH57x 2.4G (0x1189:0x8851)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8890,
        "Anticater / CH57x (0x1189:0x8890)",
        TransportType::Usb,
    ),
    (
        0x1189,
        0x8830,
        "Anticater / CH57x 2.4G (0x1189:0x8830)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8831,
        "Anticater / LQKJ 2.4G (0x1189:0x8831)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8832,
        "Anticater / LQKJ 2.4G (0x1189:0x8832)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8833,
        "Anticater / LQKJ 2.4G (0x1189:0x8833)",
        TransportType::Wireless24G,
    ),
    (
        0x514C,
        0x8830,
        "Anticater / LQKJ 2.4G (0x514c:0x8830)",
        TransportType::Wireless24G,
    ),
    (
        0x514C,
        0x8831,
        "Anticater / LQKJ 2.4G (0x514c:0x8831)",
        TransportType::Wireless24G,
    ),
    (
        0x514C,
        0x8832,
        "Anticater / LQKJ 2.4G (0x514c:0x8832)",
        TransportType::Wireless24G,
    ),
    (
        0x514C,
        0x8833,
        "Anticater / LQKJ 2.4G (0x514c:0x8833)",
        TransportType::Wireless24G,
    ),
    (
        0x25A7,
        0xFA11,
        "Anticater 2.4G Receiver (0x25a7:0xfa11)",
        TransportType::Wireless24G,
    ),
];

/// Usage page of the vendor configuration endpoint -- the one interface
/// that accepts the commands this tool sends.
pub const VENDOR_USAGE_PAGE: u16 = 0xFF00;

// ---------------------------------------------------------------------------
// Knob model: what the firmware holds.
// ---------------------------------------------------------------------------

/// Gestures per knob: twist-left, press, twist-right, hold+twist left,
/// hold+twist right. Sizes every slot plan and every classifier walk.
pub const GESTURES_PER_KNOB: usize = 5;

/// Device layers the firmware holds. A different axis from the host layers:
/// only the bound ones can carry slot chords the daemon hears.
pub const DEVICE_LAYERS: u8 = 3;

/// Device layers a default `bind-slots` flash targets: all of them.
pub const BIND_LAYERS: [u8; 3] = [0, 1, 2];

/// HID keycodes for F16..F20, the five slot chords `bind-slots` writes.
pub const SLOT_KEYCODES: [u8; GESTURES_PER_KNOB] = [0x6B, 0x6C, 0x6D, 0x6E, 0x6F];

/// Control + Alt, the modifier pair those chords carry.
///
/// Named for the chords, not the slots: `host::gesture` has its own
/// `SLOT_MODS` (the ctrl/alt/shift WORDS a binding wants), and the two
/// must never be mistaken for each other.
pub const SLOT_CHORD_MODS: u8 = 0x01 | 0x04;

// ---------------------------------------------------------------------------
// Slot records (`0xFD` writes, `0xFA` reads).
// ---------------------------------------------------------------------------

/// The sequence-macro write command byte. `0xFE` is the chord writer.
pub const FD_CMD: u8 = 0xFD;

/// Bytes before the first entry: report, command, key, layer, kind, b5, len.
pub const FD_HEADER_LEN: usize = 7;

/// Delay hi, delay lo, value.
pub const FD_ENTRY_LEN: usize = 3;

/// How many entries fit in one report. Derived, never restated.
pub const FD_MAX_ENTRIES: usize = (REPORT_LEN - FD_HEADER_LEN) / FD_ENTRY_LEN;

/// The modifier entry for HID modifier bit `n`, `FD_MOD_ENTRY_BASE + n`.
///
/// Measured one at a time: `Ctrl+` is `F1`, `Shift+` `F2`, `Alt+` `F3`,
/// `Win+` and `command` both `F4`, then `F5..F8` for the right-hand four.
pub const FD_MOD_ENTRY_BASE: u8 = 0xF1;

/// Record kind for keyboard actions in slot-table dumps.
pub const FD_KIND_KEYBOARD: u8 = 1;

/// The chord/mouse write command byte. `FD_CMD` is the sequence writer;
/// this one carries single chords and the mouse records whose `0xFD`
/// layout was never measured.
pub const FE_CHORD_CMD: u8 = 0xFE;

/// Record kind for mouse actions in slot-table dumps.
pub const FD_KIND_MOUSE: u8 = 3;

/// Commit staged key writes, mirroring the vendor app's `HID_write` tail.
/// Without it the firmware may ignore flashed packets. Key writes only --
/// never LED updates.
pub const SLOT_COMMIT_PREFIX: [u8; 3] = [0xFD, 0xFE, 0xFF];

/// The largest key id any supported layout reaches: a 4x4 grid plus four
/// knobs. Anything above it in byte 2 is a subcommand, not a slot.
pub const SLOT_MAX_KEY_ID: u8 = 40;

/// Burst read: slots covered per query. Three queries walk the whole table.
pub const SLOT_BURST_WIDTH: u8 = 25;
pub const SLOT_BURST_QUERIES: u8 = 3;

/// The slot-table query command: `[FA group 00 counter]`. Shares its byte
/// with the LED query above -- the vendor reuses `0xFA` across
/// sub-protocols -- but names this direction's use of it.
pub const SLOT_TABLE_QUERY_CMD: u8 = 0xFA;

/// The bank the normal read path walks (`0x0F` / `0x19` observed on the
/// wire). The exploratory sweep range is our own choice and lives in the
/// policy map instead.
pub const SLOT_DEFAULT_GROUP: u8 = 0x0F;

/// How long a slot-table query waits for its dump.
pub const SLOT_QUERY_TIMEOUT_MS: u64 = 500;

// ---------------------------------------------------------------------------
// Backlight (`0xFE` writes behind a `0xFB` init, `0xFA 0xB0` reads).
// ---------------------------------------------------------------------------

/// The backlight write command byte.
pub const LED_CMD: u8 = 0xFE;

/// Sub-command of the backlight write: `[FE B0 layer mode]` after the
/// report ID. Same byte as the query sub-command below, other direction.
pub const LED_WRITE_SUB: u8 = 0xB0;

/// The init that must precede any LED write. Without it the firmware
/// accepts the write, stores the mode, reads it back correctly, and
/// changes nothing.
pub const LED_INIT_CMD: u8 = 0xFB;

/// The mode query: `[FA B0 layer]`, answered `03 FA <mode> ...`.
pub const LED_QUERY_CMD: u8 = 0xFA;
pub const LED_QUERY_SUB: u8 = 0xB0;

/// The command byte a vendor QUERY reply carries. Shares its value with
/// the query command above -- the vendor reuses `0xFA` -- but it names the
/// other direction, so it keeps its own name.
pub const LED_REPLY_TAG: u8 = 0xFA;

/// Backlight modes on the `514c:8850`, in mode order. Modes 1 and 2 are
/// fixed COLOURS (red, green), not effects; mode 4 is the multicoloured
/// effect the knob ships in.
pub const LED_MODE_OFF: u8 = 0;
pub const LED_MODE_RED: u8 = 1;
pub const LED_MODE_GREEN: u8 = 2;
pub const LED_MODE_RIPPLE: u8 = 3;
pub const LED_MODE_RAINBOW: u8 = 4;
pub const LED_MODE_RGB: u8 = 5;

/// Every LED mode name this build can send, in mode order.
pub const LED_MODE_NAMES: [&str; 6] = ["off", "red", "green", "ripple", "rainbow", "rgb"];

/// Per-entry colours the device stores per layer. The vendor sends a
/// 48-byte palette of different colours per entry (its own rainbow); a
/// bare colour fills every entry.
pub const PALETTE_ENTRIES: usize = 16;

/// The vendor's own rainbow, in its own order, cycled to fill the palette.
pub const VENDOR_RAINBOW: [(u8, u8, u8); 6] = [
    (0xFF, 0x00, 0x00),
    (0xFF, 0x80, 0x30),
    (0xFF, 0xFF, 0x30),
    (0x00, 0xFF, 0x00),
    (0x00, 0xFF, 0xFF),
    (0x00, 0x00, 0xFF),
];

/// Base colour offset, then the per-key triples: `[03 FE B0 layer mode]
/// [base R G B] [16 x RGB]`. The base is what single-colour modes display.
pub const BASE_COLOR_OFFSET: usize = 5;
pub const PALETTE_OFFSET: usize = 8;

/// How many unrelated reports to skip looking for an LED reply. Small on
/// purpose: this drains a queue (the init packet's own echo arrives first),
/// it does not wait out a silent device.
pub const LED_MAX_SKIPPED_REPORTS: usize = 8;

// ---------------------------------------------------------------------------
// Exchange timings: one budget per exchange, in milliseconds.
// ---------------------------------------------------------------------------

/// Settle between the LED init and its mode packet (`send_led`).
pub const LED_INIT_SETTLE_MS: u64 = 20;

/// How long an LED mode query waits for its reply (`read_led_mode`).
pub const LED_READ_TIMEOUT_MS: u64 = 500;

/// Minimum gap between the START of two LED jobs in one process
/// (`led_sync` pacer). The renderer's freeze strikes on bursts.
pub const LED_JOB_INTERVAL_MS: u64 = 500;

/// Settle between consecutive layer writes inside one LED job.
pub const LED_INTER_LAYER_SETTLE_MS: u64 = 150;

/// Gap between consecutive slot-write reports (`bind-slots`, `bind-seq`,
/// gesture probing).
pub const SLOT_WRITE_GAP_MS: u64 = 15;

/// Gap between reports while flashing a standalone keymap's slots.
pub const UPLOAD_PACKET_GAP_MS: u64 = 10;

/// Gap between per-slot read queries while walking the table.
pub const SLOT_READ_GAP_MS: u64 = 30;

/// Gap between queries in the legacy per-slot walk (`read-slots`). Older
/// than the burst path and paced wider; kept as measured rather than
/// merged with the burst gap it resembles but is not.
pub const SLOT_WALK_GAP_MS: u64 = 50;

/// Settle after a flash before reading the table back to verify it.
pub const SLOT_VERIFY_SETTLE_MS: u64 = 100;

/// Gap between burst queries while reading the whole slot table.
pub const SLOT_TABLE_READ_GAP_MS: u64 = 20;

/// Settle after the slot-commit tail (`send_commit`).
pub const COMMIT_SETTLE_MS: u64 = 50;

/// Backoff base while retrying a flaky probe read.
pub const PROBE_READ_RETRY_BASE_MS: u64 = 300;
