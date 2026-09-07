//! Backlight packets.
//!
//! Split from `protocol.rs` for the file-length gate. The format is the
//! vendor app's, read out of its binary rather than guessed:
//! `03 FE B0 <layer> <mode>` followed by sixteen RGB triples from offset 5.

use anyhow::{anyhow, Result};

/// Every LED mode name this build can send, in mode order.
///
/// The request was a *breathing* colour per layer and none of these is
/// known to be one -- the vendor mode table is only partly mapped and
/// `led_mode_name` reports a mode 5 ("custom") nothing here can produce.
/// `antiknob led-probe` walks this list against the hardware so the
/// question can be answered by looking rather than by guessing.
/// Mode names, in mode order, from hardware testing on a 514c:8850
/// (kriomant/ch57x-keyboard-tool#173). Mode 5 is omitted deliberately: it
/// crashes this firmware.
pub const LED_MODE_NAMES: [&str; 5] = ["off", "static", "reactive", "ripple", "rainbow"];

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

    // Mode table for the 514c:8850, from hardware testing in
    // kriomant/ch57x-keyboard-tool#173 and confirmed on a real knob by
    // watching each one:
    //
    //   0 off   1 static   2 reactive   3 ripple   4 rainbow   5 CRASHES
    //
    // The older names (`backlight`, `shock`, `shock2`, `press`) came from
    // the 1189:884x family and are kept as aliases so existing configs load,
    // but they are not what this device calls these effects -- `press` in
    // particular is mode 4, which is the rainbow.
    //
    // Colour arguments are parsed and sent, and THIS knob ignores them:
    // mode 1 was set with blue, red and green in turn and stayed red every
    // time. The 16-key variant sharing this product id does honour them.
    match parts[0] {
        "off" | "mode0" => {
            packet[4] = 0;
        }
        "static" | "backlight" | "steady" | "mode1" => {
            let entries = parse_palette(parts.get(1).copied().unwrap_or("white"))?;
            packet[4] = 1;
            fill_palette_entries(&mut packet, &entries);
        }
        "reactive" | "shock" | "mode2" => {
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
        // Refused, not supported. Mode 5 crashes this firmware -- sending it
        // wedged a real knob's LED renderer until the device was
        // power-cycled, and it had been sitting lit on battery meanwhile.
        "custom" | "mode5" => {
            return Err(anyhow!(
                "LED mode 5 crashes the firmware on this device (see \
                 kriomant/ch57x-keyboard-tool#173); use `rainbow` (mode 4) \
                 for the multicoloured effect"
            ));
        }
        other => {
            return Err(anyhow!(
                "Unknown LED mode '{}'. Available: off, static, reactive, ripple, rainbow \
                 (mode0..mode4). Note this device ignores colour arguments.",
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

    /// Mode 5 crashed a real device's LED renderer. It must never build.
    #[test]
    fn mode_five_is_refused_and_says_why() {
        for spec in ["mode5", "custom", "custom rainbow"] {
            let err = build_led_packet(0, spec).expect_err(spec);
            assert!(err.to_string().contains("crashes"), "{err}");
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
        assert_eq!(
            LED_MODE_NAMES,
            ["off", "static", "reactive", "ripple", "rainbow"]
        );
        for (name, want) in [
            ("off", 0u8),
            ("static", 1),
            ("reactive", 2),
            ("ripple", 3),
            ("rainbow", 4),
        ] {
            let p = build_led_packet(0, name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(p[4], want, "{name}");
        }
    }
}
