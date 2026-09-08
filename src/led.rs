//! Backlight packets.
//!
//! Split from `protocol.rs` for the file-length gate. The format is the
//! vendor app's, read out of its binary rather than guessed:
//! `03 FE B0 <layer> <mode>` followed by sixteen RGB triples from offset 5.

use anyhow::{anyhow, Result};

/// Every LED mode name this build can send, in mode order.
///
/// Named for what the knob DOES, because that is what was finally looked
/// at. The previous names came from `kriomant/ch57x-keyboard-tool#173` and
/// describe effects (`static`, `reactive`) rather than this device's
/// behaviour: on a `514c:8850` mode 1 is red and mode 2 is green, fixed,
/// whatever colour bytes are sent. That is why setting mode 1 to blue, red
/// and green in turn produced red three times -- not a device that ignores
/// colour arbitrarily, but a mode that IS red.
///
/// Mode 5 is here. It was refused for weeks as "crashes the firmware", and
/// it does not: a capture of ANTICATER.app walking its own mode buttons
/// shows `03 FE B0 00 05` sent like any other, and the knob renders a
/// second multicoloured effect. Whatever wedged the LED renderer once was
/// not mode 5 -- the likeliest candidate is the known freeze bug
/// (kriomant/ch57x-keyboard-tool#175), which strikes 2s-2m after ANY mode
/// change and needs a replug.
///
/// Mode 3 is the one name here still taken on trust from #173; nobody has
/// reported what this device shows for it.
pub const LED_MODE_NAMES: [&str; 6] = ["off", "red", "green", "ripple", "rainbow", "rgb"];

pub fn build_led_packet(layer: u8, mode: &str) -> Result<Vec<u8>> {
    let mut packet = vec![0u8; 64];
    packet[0] = 0x03;
    packet[1] = 0xFE;
    packet[2] = 0xB0;
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
            packet[4] = 0;
        }
        "red" | "static" | "backlight" | "steady" | "mode1" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("white"))?;
            packet[4] = 1;
            fill_palette_entries(&mut packet, &entries);
        }
        "green" | "reactive" | "shock" | "mode2" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("red"))?;
            packet[4] = 2;
            fill_palette_entries(&mut packet, &entries);
        }
        "ripple" | "shock2" | "mode3" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("blue"))?;
            packet[4] = 3;
            fill_palette_entries(&mut packet, &entries);
        }
        // The multicoloured effect this device ships in.
        "rainbow" | "press" | "mode4" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("rainbow"))?;
            packet[4] = 4;
            fill_palette_entries(&mut packet, &entries);
        }
        // The second multicoloured effect. Refused for weeks as "crashes the
        // firmware", which it does not: the vendor app sends `03 FE B0 00
        // 05` while walking its own mode buttons, captured on this exact
        // hardware, and the knob renders it.
        "rgb" | "custom" | "mode5" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("rainbow"))?;
            packet[4] = 5;
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
pub const PALETTE_ENTRIES: usize = 16;

/// The vendor's own rainbow, in its own order, cycled to fill the palette.
const RAINBOW: [(u8, u8, u8); 6] = [
    (0xFF, 0x00, 0x00),
    (0xFF, 0x80, 0x30),
    (0xFF, 0xFF, 0x30),
    (0x00, 0xFF, 0x00),
    (0x00, 0xFF, 0xFF),
    (0x00, 0x00, 0xFF),
];

/// Resolve a palette argument into sixteen entries.
///
/// A bare colour fills every entry, as before. `rainbow` reproduces the
/// vendor's. A comma-separated list sets entries in order and repeats to
/// fill, so `red,blue` alternates.
pub fn parse_palette(spec: &str) -> Result<Vec<(u8, u8, u8)>> {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("rainbow") {
        return Ok((0..PALETTE_ENTRIES)
            .map(|i| RAINBOW[i % RAINBOW.len()])
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
const BASE_COLOR_OFFSET: usize = 5;
const PALETTE_OFFSET: usize = 8;

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
    let mut p = vec![0u8; 64];
    p[0] = 0x03;
    p[1] = 0xFB;
    p[2] = 0xFB;
    p[3] = 0xFB;
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
