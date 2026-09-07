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
pub const LED_MODE_NAMES: [&str; 6] = ["off", "backlight", "shock", "shock2", "press", "custom"];

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

    match parts[0] {
        "off" | "mode0" => {
            packet[4] = 0;
        }
        "backlight" | "steady" | "mode1" => {
            let color = parts.get(1).copied().unwrap_or("white");
            let (r, g, b) = parse_color(color)?;
            packet[4] = 1;
            fill_palette(&mut packet, r, g, b);
        }
        "shock" | "reactive" | "mode2" => {
            let color = parts.get(1).copied().unwrap_or("red");
            let (r, g, b) = parse_color(color)?;
            packet[4] = 2;
            fill_palette(&mut packet, r, g, b);
        }
        "shock2" | "ripple" | "mode3" => {
            let color = parts.get(1).copied().unwrap_or("blue");
            let (r, g, b) = parse_color(color)?;
            packet[4] = 3;
            fill_palette(&mut packet, r, g, b);
        }
        "press" | "mode4" => {
            let color = parts.get(1).copied().unwrap_or("green");
            let (r, g, b) = parse_color(color)?;
            packet[4] = 4;
            fill_palette(&mut packet, r, g, b);
        }
        // Mode 5 is real: the vendor app validates a mode read back from
        // the device with `cmpq $0x5; ja invalid`, and `led_mode_name` has
        // always called it "custom". Nothing here could produce it, which is
        // why this build could not restore a knob that shipped in it.
        // It takes no colour -- the firmware drives the palette itself.
        "custom" | "mode5" => {
            // No colour: the firmware drives its own palette here. Taking
            // one and ignoring it is how someone ends up believing they
            // set a colour that never applied.
            if let Some(extra) = parts.get(1) {
                return Err(anyhow!(
                    "LED mode 'custom' takes no colour (got '{}'); the firmware \
                     drives its own palette in this mode",
                    extra
                ));
            }
            packet[4] = 5;
        }
        other => return Err(anyhow!("Unknown LED mode '{}'. Available: off, backlight <color>, shock <color>, shock2 <color>, press <color>, custom, mode1..mode5", other)),
    }

    Ok(packet)
}

fn fill_palette(packet: &mut [u8], r: u8, g: u8, b: u8) {
    for i in 0..16 {
        let off = 5 + i * 3;
        packet[off] = r;
        packet[off + 1] = g;
        packet[off + 2] = b;
    }
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
