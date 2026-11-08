//! Backlight packets.
//!
//! Split from `protocol.rs` for the file-length gate. The format is the
//! vendor app's, read out of its binary rather than guessed:
//! `03 FE B0 <layer> <mode>` followed by sixteen RGB triples from offset 5.

use crate::firmware::{
    BASE_COLOR_OFFSET, KEY_LED_OFFSET, LED_CMD, LED_INIT_CMD, LED_MODE_GREEN, LED_MODE_OFF,
    LED_MODE_RAINBOW, LED_MODE_RED, LED_MODE_RGB, LED_MODE_RIPPLE, LED_WRITE_SUB, PALETTE_ENTRIES,
    PALETTE_OFFSET, REPORT_ID, REPORT_LEN, VENDOR_KEY_LED, VENDOR_RAINBOW,
};
use anyhow::{anyhow, Result};

/// The mode number a spec string names, or `None` when it names nothing.
///
/// The same alias table `build_led_packet` matches on, factored out so a
/// caller can compare "what the layer wants" against "what the device
/// holds" without building (and sending) a packet to find out. The two
/// matches name the same modes; `mode_number_names_its_index` pins the
/// canonical half, and the alias cases below pin the rest.
#[must_use]
pub fn led_mode_number(spec: &str) -> Option<u8> {
    let first = spec
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .next()?
        .to_string();
    match first.as_str() {
        "off" | "mode0" => Some(LED_MODE_OFF),
        "red" | "static" | "backlight" | "steady" | "mode1" => Some(LED_MODE_RED),
        "green" | "reactive" | "shock" | "mode2" => Some(LED_MODE_GREEN),
        "ripple" | "shock2" | "mode3" => Some(LED_MODE_RIPPLE),
        "rainbow" | "press" | "mode4" => Some(LED_MODE_RAINBOW),
        "rgb" | "custom" | "mode5" => Some(LED_MODE_RGB),
        _ => None,
    }
}

/// The mode number a write may be SKIPPED on, because the layer already
/// holds it. `None` when the spec carries a per-key palette: the stored
/// mode says nothing about the stored colours, so a matching mode must not
/// suppress the write that changes them.
#[must_use]
pub fn skippable_mode_number(spec: &str) -> Option<u8> {
    if spec_keys(spec).is_some() {
        None
    } else {
        led_mode_number(spec)
    }
}

/// The `keys=...` token of a spec, if it has one.
fn spec_keys(spec: &str) -> Option<String> {
    spec.split_whitespace()
        .skip(1)
        .find(|t| t.to_ascii_lowercase().starts_with(KEYS_PREFIX))
        .map(|t| t[KEYS_PREFIX.len()..].to_ascii_lowercase())
}

/// The spec token that selects a per-key palette: `<mode> keys=<palette>`.
const KEYS_PREFIX: &str = "keys=";

/// Parse a per-key palette: `vendor` (the table ANTICATER.app itself holds,
/// `_KeyBoard_KeyLed` @0x10022f6e0) or exactly sixteen `RRGGBB` triplets,
/// comma-separated, each with an optional leading `#`.
///
/// Strict on purpose: the static reading pins 16 triplets and nothing else
/// (`VENDOR_UI_MAP`, "STATIC READING"). A short list that repeats to fill, as
/// `parse_palette` does for named colours, would invent a layout the vendor
/// never writes, so fewer or more than sixteen is refused, not padded.
fn parse_key_palette(list: &str) -> Result<Vec<(u8, u8, u8)>> {
    if list == "vendor" {
        return Ok(VENDOR_KEY_LED.to_vec());
    }
    let entries: Vec<&str> = list.split(',').collect();
    if entries.len() != PALETTE_ENTRIES {
        return Err(anyhow!(
            "keys= takes exactly {PALETTE_ENTRIES} comma-separated RRGGBB colours (one per \
             entry the vendor writes), or `vendor`; got {}",
            entries.len()
        ));
    }
    entries
        .into_iter()
        .map(|e| {
            let digits = e.strip_prefix('#').unwrap_or(e);
            let ok = digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit());
            let v = ok.then(|| u32::from_str_radix(digits, 16).ok()).flatten();
            v.map(|v| {
                let [_, r, g, b] = v.to_be_bytes();
                (r, g, b)
            })
            .ok_or_else(|| anyhow!("keys= colour '{e}' is not six hex digits (RRGGBB)"))
        })
        .collect()
}

/// The vendor's LED write: `fe b0 <layer> <mode>` and the 48 colour bytes
/// straight after the mode (`Widget::HID_write` @0x10003dea0). No base
/// colour in front: the sixteen triplets start at byte 5.
fn build_key_led_packet(layer: u8, mode: u8, palette: &[(u8, u8, u8)]) -> Vec<u8> {
    let mut packet = vec![0u8; REPORT_LEN];
    packet[0] = REPORT_ID;
    packet[1] = LED_CMD;
    packet[2] = LED_WRITE_SUB;
    packet[3] = layer;
    packet[4] = mode;
    for (i, (r, g, b)) in palette.iter().take(PALETTE_ENTRIES).enumerate() {
        let off = KEY_LED_OFFSET + i * 3;
        packet[off..off + 3].copy_from_slice(&[*r, *g, *b]);
    }
    packet
}

/// # Errors
///
/// Fails when the layer is past the firmware's layers, or the mode is empty,
/// unknown or malformed.
pub fn build_led_packet(layer: u8, mode: &str) -> Result<Vec<u8>> {
    // The firmware holds 3 layers and nothing switches the active one, so a
    // mode stored past them can never render (PLAN H.4, webcam 2026-11-01).
    if layer > crate::policy::SET_LED_LAYER_MAX {
        return Err(anyhow!(
            "Layer {layer} is past the firmware's {} layers (0-{}): a mode stored there can never render",
            crate::firmware::DEVICE_LAYERS,
            crate::policy::SET_LED_LAYER_MAX
        ));
    }
    let mut packet = vec![0u8; REPORT_LEN];
    packet[0] = REPORT_ID;
    packet[1] = LED_CMD;
    packet[2] = LED_WRITE_SUB;
    packet[3] = layer;

    let mode_lower = mode.trim().to_ascii_lowercase();
    let parts: Vec<&str> = mode_lower.split_whitespace().collect();

    if parts.is_empty() {
        return Err(anyhow!("Empty LED mode"));
    }

    // Per-key colours: `<mode> keys=<palette>`, the vendor's layout.
    if let Some(list) = spec_keys(&mode_lower) {
        if parts.len() != 2 {
            return Err(anyhow!(
                "keys= stands alone after the mode (`rgb keys=vendor`); a colour name \
                 beside it has no vendor encoding"
            ));
        }
        let number = led_mode_number(parts[0]).ok_or_else(|| {
            anyhow!(
                "Unknown LED mode '{}'. Available: off, red, green, ripple, rainbow, rgb",
                parts[0]
            )
        })?;
        return Ok(build_key_led_packet(
            layer,
            number,
            &parse_key_palette(&list)?,
        ));
    }

    // Mode table for the 514c:8850, from watching the knob:
    //
    //   0 off   1 red   2 green   3 ripple(*)   4 rainbow   5 rgb
    //
    // (*) mode 3 is the one entry still taken from
    // kriomant/ch57x-keyboard-tool#173 rather than observed here.
    //
    // Two names in the old table were wrong about this device rather than
    // merely vague. `static` and `reactive` describe effects; modes 1 and 2
    // are fixed COLOURS, red and green. That also explains the standing note
    // that this knob "ignores the colour bytes": it does, but the reason is
    // that each mode carries its own colour, not that the renderer discards
    // them. The 16-key variant sharing this product id does honour them.
    //
    // The older names (`backlight`, `shock`, `shock2`, `press`, `static`,
    // `reactive`) are kept as aliases so existing configs keep loading.
    match parts[0] {
        "off" | "mode0" => {
            packet[4] = LED_MODE_OFF;
        }
        "red" | "static" | "backlight" | "steady" | "mode1" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("white"))?;
            packet[4] = LED_MODE_RED;
            fill_palette_entries(&mut packet, &entries);
        }
        "green" | "reactive" | "shock" | "mode2" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("red"))?;
            packet[4] = LED_MODE_GREEN;
            fill_palette_entries(&mut packet, &entries);
        }
        "ripple" | "shock2" | "mode3" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("blue"))?;
            packet[4] = LED_MODE_RIPPLE;
            fill_palette_entries(&mut packet, &entries);
        }
        // The multicoloured effect this device ships in.
        "rainbow" | "press" | "mode4" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("rainbow"))?;
            packet[4] = LED_MODE_RAINBOW;
            fill_palette_entries(&mut packet, &entries);
        }
        // The second multicoloured effect. Refused for weeks as "crashes the
        // firmware", which it does not: the vendor app sends `03 FE B0 00
        // 05` while walking its own mode buttons, captured on this exact
        // hardware, and the knob renders it.
        "rgb" | "custom" | "mode5" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("rainbow"))?;
            packet[4] = LED_MODE_RGB;
            fill_palette_entries(&mut packet, &entries);
        }
        other => {
            return Err(anyhow!(
                "Unknown LED mode '{other}'. Available: off, red, green, ripple, rainbow, rgb \
                 (mode0..mode5). Each mode carries its own colour on this device, so \
                 colour arguments are sent but not honoured."
            ))
        }
    }

    Ok(packet)
}

/// The sixteen per-entry colours the device stores for a layer.
///
/// Captured off the wire from ANTICATER.app: it sends a 48-byte palette of
/// DIFFERENT colours per entry (`ff0000 ff8030 ffff30 00ff00 00ffff 0000ff
/// ...`), which is the multicoloured effect the knob ships with. Its own UI
/// offers six preset buttons and no colour control at all, so the hardware
/// has always been able to do more than the vendor exposed.
///
/// `fill_palette` wrote one colour to all sixteen entries, which is why this
/// build could only ever produce a solid colour.
///
/// Resolve a palette argument into sixteen entries.
///
/// Counts and the vendor palette live in the firmware map.
/// A bare colour fills every entry, as before. `rainbow` reproduces the
/// vendor's. A comma-separated list sets entries in order and repeats to
/// fill, so `red,blue` alternates.
///
/// # Errors
///
/// Fails when a colour in the spec does not parse.
pub fn parse_palette(spec: &str) -> Result<Vec<(u8, u8, u8)>> {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("rainbow") {
        return Ok((0..PALETTE_ENTRIES)
            .map(|i| VENDOR_RAINBOW[i % VENDOR_RAINBOW.len()])
            .collect());
    }
    let listed: Vec<(u8, u8, u8)> = spec
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_color)
        .collect::<Result<_>>()?;
    if listed.is_empty() {
        return Err(anyhow!("empty palette"));
    }
    Ok((0..PALETTE_ENTRIES)
        .map(|i| listed[i % listed.len()])
        .collect())
}

/// Colour bytes are sent, and this knob ignores them.
///
/// Measured, not assumed: mode 1 was set with `00 00 FF`, `FF 00 00` and
/// `00 FF 00` in turn, and the light stayed red every time. Every visible
/// change on this device has tracked the MODE byte and never a colour.
///
/// They are still sent because the 16-key `514c:8850` variant that
/// kriomant/ch57x-keyboard-tool#175 was tested against does honour per-key
/// RGB -- same product id, different hardware. Sending them costs nothing
/// and helps that variant; believing they do something here does not.
///
/// Where the per-key palette starts.
///
/// Bytes 5-7 are a BASE colour and the sixteen per-key triples begin at 8 --
/// `[03 FE B0 layer mode] [base R G B] [16 x RGB]`, documented from hardware
/// testing in kriomant/ch57x-keyboard-tool#173 and #175. This build wrote the
/// palette from byte 5, shifting every entry three bytes and overwriting the
/// base colour with the first one. Re-reading the vendor capture with the
/// right frame, `ff 00 00` at 5-7 is the base and `ff 80 30 ...` from 8 is
/// the palette -- the same bytes, and I had read them as one array.
///
/// Offsets live in the firmware map; the layout note stays here, where the
/// writer that must honour it lives.
fn fill_palette_entries(packet: &mut [u8], entries: &[(u8, u8, u8)]) {
    // The base colour is what the single-colour modes actually display.
    if let Some((r, g, b)) = entries.first() {
        packet[BASE_COLOR_OFFSET] = *r;
        packet[BASE_COLOR_OFFSET + 1] = *g;
        packet[BASE_COLOR_OFFSET + 2] = *b;
    }
    for (i, (r, g, b)) in entries.iter().take(PALETTE_ENTRIES).enumerate() {
        let off = PALETTE_OFFSET + i * 3;
        packet[off] = *r;
        packet[off + 1] = *g;
        packet[off + 2] = *b;
    }
}

/// The packet that must precede any LED write on a `514c:8850`.
///
/// Without it the firmware accepts an LED write, stores the mode, reads it
/// back, and changes nothing -- which is exactly the symptom that cost this
/// session several wrong conclusions. The vendor app sends it on connect.
/// Documented in kriomant/ch57x-keyboard-tool#173.
#[must_use]
pub fn led_init_packet() -> Vec<u8> {
    let mut p = vec![0u8; REPORT_LEN];
    p[0] = REPORT_ID;
    p[1] = LED_INIT_CMD;
    p[2] = LED_INIT_CMD;
    p[3] = LED_INIT_CMD;
    p
}

fn parse_color(s: &str) -> Result<(u8, u8, u8)> {
    match s.to_ascii_lowercase().as_str() {
        "white" => Ok((255, 255, 255)),
        "red" => Ok((255, 0, 0)),
        "orange" => Ok((255, 128, 0)),
        "yellow" => Ok((255, 255, 0)),
        "green" => Ok((0, 255, 0)),
        "cyan" => Ok((0, 255, 255)),
        "blue" => Ok((0, 0, 255)),
        "purple" | "magenta" => Ok((255, 0, 255)),
        other => Err(anyhow!(
            "Unknown color '{other}'. Supported: white, red, orange, yellow, green, cyan, blue, purple"
        )),
    }
}

#[cfg(test)]
#[path = "led_tests.rs"]
mod palette_tests;
