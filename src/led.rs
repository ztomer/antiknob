//! Backlight packets.
//!
//! Split from `protocol.rs` for the file-length gate. The format is the
//! vendor app's, read out of its binary rather than guessed:
//! `03 FE B0 <layer> <mode>` followed by sixteen RGB triples from offset 5.

use crate::firmware::{
    BASE_COLOR_OFFSET, LED_CMD, LED_INIT_CMD, LED_MODE_GREEN, LED_MODE_OFF, LED_MODE_RAINBOW,
    LED_MODE_RED, LED_MODE_RGB, LED_MODE_RIPPLE, LED_WRITE_SUB, PALETTE_ENTRIES, PALETTE_OFFSET,
    REPORT_ID, REPORT_LEN, VENDOR_RAINBOW,
};
use anyhow::{anyhow, Result};

/// The mode number a spec string names, or `None` when it names nothing.
///
/// The same alias table `build_led_packet` matches on, factored out so a
/// caller can compare "what the layer wants" against "what the device
/// holds" without building (and sending) a packet to find out. The two
/// matches name the same modes; `mode_number_names_its_index` pins the
/// canonical half, and the alias cases below pin the rest.
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

pub fn build_led_packet(layer: u8, mode: &str) -> Result<Vec<u8>> {
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
                "Unknown LED mode '{}'. Available: off, red, green, ripple, rainbow, rgb \
                 (mode0..mode5). Each mode carries its own colour on this device, so \
                 colour arguments are sent but not honoured.",
                other
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
            "Unknown color '{}'. Supported: white, red, orange, yellow, green, cyan, blue, purple",
            other
        )),
    }
}

#[cfg(test)]
mod palette_tests {
    use super::*;
    use crate::firmware::LED_MODE_NAMES;

    /// This knob ignores the colour bytes -- three different values all came
    /// out red -- but the 16-key variant sharing its product id honours
    /// them, so they must still be well-formed: base at 5-7, per-key from 8.
    #[test]
    fn the_base_colour_and_palette_sit_where_the_protocol_says() {
        let p = build_led_packet(0, "static blue").expect("packet");
        assert_eq!(&p[BASE_COLOR_OFFSET..BASE_COLOR_OFFSET + 3], &[0, 0, 255]);
        assert_eq!(&p[PALETTE_OFFSET..PALETTE_OFFSET + 3], &[0, 0, 255]);
    }

    #[test]
    fn a_list_repeats_to_fill_the_palette() {
        let p = build_led_packet(0, "static red,blue").expect("packet");
        for i in 0..PALETTE_ENTRIES {
            let o = PALETTE_OFFSET + i * 3;
            let want: [u8; 3] = if i % 2 == 0 { [255, 0, 0] } else { [0, 0, 255] };
            assert_eq!(&p[o..o + 3], &want, "entry {i}");
        }
    }

    #[test]
    fn an_unknown_colour_in_a_list_is_refused_rather_than_skipped() {
        assert!(build_led_packet(0, "static red,chartreuse").is_err());
        assert!(parse_palette("").is_err());
    }

    /// Mode 5 was refused for weeks as "crashes the firmware". It does not:
    /// a capture of ANTICATER.app walking its own mode buttons shows
    /// `03 FE B0 00 05` sent like any other mode, and the knob renders a
    /// second multicoloured effect. This test used to pin the refusal, which
    /// made it part of the defect rather than a guard against one.
    #[test]
    fn mode_five_builds_because_the_vendor_app_sends_it() {
        for spec in ["mode5", "custom", "rgb", "rgb rainbow"] {
            let p = build_led_packet(0, spec).unwrap_or_else(|e| panic!("{spec}: {e}"));
            assert_eq!(p[4], 5, "{spec}");
            assert_eq!(&p[..4], &[0x03, 0xFE, 0xB0, 0x00]);
        }
    }

    /// Modes 1 and 2 are fixed COLOURS on this device, not the effects the
    /// reference project's table names. The old names stay as aliases so
    /// configs written against them keep loading.
    #[test]
    fn the_colour_names_and_their_legacy_aliases_reach_the_same_modes() {
        for (spec, mode) in [
            ("red", 1u8),
            ("static", 1),
            ("backlight", 1),
            ("green", 2),
            ("reactive", 2),
            ("shock", 2),
            ("rainbow", 4),
            ("press", 4),
            ("rgb", 5),
            ("custom", 5),
        ] {
            let p = build_led_packet(0, spec).unwrap_or_else(|e| panic!("{spec}: {e}"));
            assert_eq!(p[4], mode, "{spec}");
        }
    }

    /// Every mode the device has must be nameable, or `led-read` reports a
    /// live mode as unknown and `led-probe` cannot walk them all.
    #[test]
    fn every_mode_the_device_has_is_named_and_buildable() {
        assert_eq!(LED_MODE_NAMES.len(), 6);
        for (mode, name) in LED_MODE_NAMES.iter().enumerate() {
            let p = build_led_packet(0, name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(p[4] as usize, mode, "{name}");
        }
    }

    /// The number mapping agrees with the packet builder everywhere they
    /// overlap: canonical names hit their own index, aliases hit the same
    /// mode the packet carries, and unknown words are `None` rather than a
    /// mode that would read back as a mismatch. `sync_led` trusts this to
    /// skip writes, so a wrong entry here silently drops a light change.
    #[test]
    fn mode_number_names_its_index() {
        for (mode, name) in LED_MODE_NAMES.iter().enumerate() {
            assert_eq!(led_mode_number(name), Some(mode as u8), "{name}");
        }
        for (spec, mode) in [
            ("static", 1u8),
            ("backlight", 1),
            ("steady", 1),
            ("mode1", 1),
            ("reactive", 2),
            ("shock", 2),
            ("mode2", 2),
            ("shock2", 3),
            ("mode3", 3),
            ("press", 4),
            ("mode4", 4),
            ("custom", 5),
            ("mode5", 5),
            ("mode0", 0),
            ("red white", 1),
            ("  GREEN  ", 2),
        ] {
            assert_eq!(led_mode_number(spec), Some(mode), "{spec}");
        }
        for spec in ["", "   ", "chartreuse", "mode6", "reddish"] {
            assert_eq!(led_mode_number(spec), None, "{spec}");
        }
    }

    /// Every LED write needs this in front or the device stores the mode,
    /// reads it back, and changes nothing.
    #[test]
    fn the_init_packet_is_the_one_the_firmware_requires() {
        let p = led_init_packet();
        assert_eq!(&p[..4], &[0x03, 0xFB, 0xFB, 0xFB]);
        assert_eq!(p.len(), 64);
    }

    #[test]
    fn the_mode_table_matches_the_hardware_tested_one() {
        // Named for what the knob shows, not for the reference project's
        // effect names: mode 1 is red and mode 2 is green on this device.
        assert_eq!(
            LED_MODE_NAMES,
            ["off", "red", "green", "ripple", "rainbow", "rgb"]
        );
        for (name, want) in [
            ("off", 0u8),
            ("red", 1),
            ("green", 2),
            ("ripple", 3),
            ("rainbow", 4),
            ("rgb", 5),
        ] {
            let p = build_led_packet(0, name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(p[4], want, "{name}");
        }
    }
}
