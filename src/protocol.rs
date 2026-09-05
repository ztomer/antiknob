use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobEvent {
    RotateCCW,
    Press,
    RotateCW,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Key {
        modifiers: u8,
        code: u8,
    },
    Media(u16),
    MouseClick {
        button: u8, // 1=left, 2=right, 4=middle
    },
    MouseWheel {
        delta: i8, // 1=up, -1=down
    },
}

impl Action {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // 1. Mouse Wheel
        if s.eq_ignore_ascii_case("wheelup") {
            return Ok(Action::MouseWheel { delta: 1 });
        }
        if s.eq_ignore_ascii_case("wheeldown") {
            return Ok(Action::MouseWheel { delta: -1 });
        }

        // 2. Mouse Clicks
        if s.eq_ignore_ascii_case("click") || s.eq_ignore_ascii_case("lclick") {
            return Ok(Action::MouseClick { button: 1 });
        }
        if s.eq_ignore_ascii_case("rclick") {
            return Ok(Action::MouseClick { button: 2 });
        }
        if s.eq_ignore_ascii_case("mclick") {
            return Ok(Action::MouseClick { button: 4 });
        }

        // 3. Media Keys
        let media_code = match s.to_ascii_lowercase().as_str() {
            "volumedown" | "voldown" => Some(0x00EA),
            "volumeup" | "volup" => Some(0x00E9),
            "mute" => Some(0x00E2),
            "play" | "playpause" => Some(0x00CD),
            "next" => Some(0x00B5),
            "prev" | "previous" => Some(0x00B6),
            "stop" => Some(0x00B7),
            "brightnessup" => Some(0x006F),
            "brightnessdown" => Some(0x0070),
            _ => None,
        };
        if let Some(code) = media_code {
            return Ok(Action::Media(code));
        }

        // 4. Keyboard keys with modifiers (e.g., "cmd-c", "ctrl-alt-del", "a", "f5")
        let mut modifiers = 0u8;
        let parts: Vec<&str> = s.split('-').collect();

        for part in &parts[..parts.len() - 1] {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" => modifiers |= 0x01,
                "shift" => modifiers |= 0x02,
                "alt" | "opt" => modifiers |= 0x04,
                "cmd" | "win" => modifiers |= 0x08,
                "rctrl" => modifiers |= 0x10,
                "rshift" => modifiers |= 0x20,
                "ralt" | "ropt" => modifiers |= 0x40,
                "rcmd" | "rwin" => modifiers |= 0x80,
                other => return Err(anyhow!("Unknown modifier '{}' in '{}'", other, s)),
            }
        }

        let key_str = parts.last().unwrap().to_ascii_lowercase();
        let code = parse_keycode(&key_str)
            .ok_or_else(|| anyhow!("Unknown key '{}' in '{}'", key_str, s))?;

        Ok(Action::Key { modifiers, code })
    }

    pub fn to_packet(&self, key_id: u8, layer: u8) -> Vec<u8> {
        let mut packet = vec![0u8; 64];
        packet[0] = 0x03;
        packet[1] = 0xFE;
        packet[2] = key_id;
        packet[3] = layer + 1;

        match self {
            Action::Key { modifiers, code } => {
                packet[4] = 1; // Kind = Keyboard
                packet[10] = 1; // 1 key press
                packet[11] = *modifiers;
                packet[12] = *code;
            }
            Action::Media(code) => {
                packet[4] = 2; // Kind = Media
                let [low, high] = code.to_le_bytes();
                packet[11] = low;
                packet[12] = high;
            }
            Action::MouseClick { button } => {
                packet[4] = 3; // Kind = Mouse
                packet[10] = 0x01; // Click
                packet[12] = *button;
            }
            Action::MouseWheel { delta } => {
                packet[4] = 3; // Kind = Mouse
                packet[10] = 0x03; // Wheel
                packet[15] = *delta as u8;
            }
        }

        packet
    }
}

pub fn key_id_for_button(button_index: usize) -> u8 {
    (button_index + 1) as u8
}

pub fn key_id_for_knob(knob_index: usize, event: KnobEvent) -> u8 {
    const BASE: u8 = 16;
    let offset = match event {
        KnobEvent::RotateCCW => 0,
        KnobEvent::Press => 1,
        KnobEvent::RotateCW => 2,
    };
    BASE + (knob_index as u8) * 3 + offset
}

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
        "off" => {
            packet[4] = 0;
        }
        "backlight" | "steady" => {
            let color = parts.get(1).copied().unwrap_or("white");
            let (r, g, b) = parse_color(color)?;
            packet[4] = 1;
            packet[5] = r;
            packet[6] = g;
            packet[7] = b;
        }
        "shock" | "reactive" => {
            let color = parts.get(1).copied().unwrap_or("red");
            let (r, g, b) = parse_color(color)?;
            packet[4] = 2;
            packet[5] = r;
            packet[6] = g;
            packet[7] = b;
        }
        "shock2" | "ripple" => {
            let color = parts.get(1).copied().unwrap_or("blue");
            let (r, g, b) = parse_color(color)?;
            packet[4] = 3;
            packet[5] = r;
            packet[6] = g;
            packet[7] = b;
        }
        "press" => {
            let color = parts.get(1).copied().unwrap_or("green");
            let (r, g, b) = parse_color(color)?;
            packet[4] = 4;
            packet[5] = r;
            packet[6] = g;
            packet[7] = b;
        }
        other => return Err(anyhow!("Unknown LED mode '{}'. Available: off, backlight <color>, shock <color>, shock2 <color>, press <color>", other)),
    }

    Ok(packet)
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
        other => Err(anyhow!("Unknown color '{}'. Supported: white, red, orange, yellow, green, cyan, blue, purple", other)),
    }
}

fn parse_keycode(s: &str) -> Option<u8> {
    match s {
        // Letters a-z
        "a" => Some(0x04), "b" => Some(0x05), "c" => Some(0x06), "d" => Some(0x07),
        "e" => Some(0x08), "f" => Some(0x09), "g" => Some(0x0A), "h" => Some(0x0B),
        "i" => Some(0x0C), "j" => Some(0x0D), "k" => Some(0x0E), "l" => Some(0x0F),
        "m" => Some(0x10), "n" => Some(0x11), "o" => Some(0x12), "p" => Some(0x13),
        "q" => Some(0x14), "r" => Some(0x15), "s" => Some(0x16), "t" => Some(0x17),
        "u" => Some(0x18), "v" => Some(0x19), "w" => Some(0x1A), "x" => Some(0x1B),
        "y" => Some(0x1C), "z" => Some(0x1D),

        // Digits 1-0
        "1" => Some(0x1E), "2" => Some(0x1F), "3" => Some(0x20), "4" => Some(0x21),
        "5" => Some(0x22), "6" => Some(0x23), "7" => Some(0x24), "8" => Some(0x25),
        "9" => Some(0x26), "0" => Some(0x27),

        // Controls
        "enter" | "return" => Some(0x28),
        "esc" | "escape" => Some(0x29),
        "backspace" => Some(0x2A),
        "tab" => Some(0x2B),
        "space" => Some(0x2C),
        "minus" => Some(0x2D),
        "equal" => Some(0x2E),
        "leftbracket" => Some(0x2F),
        "rightbracket" => Some(0x30),
        "backslash" => Some(0x31),
        "semicolon" => Some(0x33),
        "quote" => Some(0x34),
        "grave" => Some(0x35),
        "comma" => Some(0x36),
        "period" | "dot" => Some(0x37),
        "slash" => Some(0x38),
        "capslock" => Some(0x39),

        // Function keys F1-F12
        "f1" => Some(0x3A), "f2" => Some(0x3B), "f3" => Some(0x3C), "f4" => Some(0x3D),
        "f5" => Some(0x3E), "f6" => Some(0x3F), "f7" => Some(0x40), "f8" => Some(0x41),
        "f9" => Some(0x42), "f10" => Some(0x43), "f11" => Some(0x44), "f12" => Some(0x45),

        // Navigation
        "printscreen" | "prtscn" => Some(0x46),
        "scrolllock" => Some(0x47),
        "pause" => Some(0x48),
        "insert" | "ins" => Some(0x49),
        "home" => Some(0x4A),
        "pageup" | "pgup" => Some(0x4B),
        "delete" | "del" => Some(0x4C),
        "end" => Some(0x4D),
        "pagedown" | "pgdn" => Some(0x4E),
        "right" => Some(0x4F),
        "left" => Some(0x50),
        "down" => Some(0x51),
        "up" => Some(0x52),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_with_modifiers() {
        let action = Action::parse("cmd-c").unwrap();
        assert_eq!(action, Action::Key { modifiers: 0x08, code: 0x06 });

        let combo = Action::parse("ctrl-alt-del").unwrap();
        assert_eq!(combo, Action::Key { modifiers: 0x01 | 0x04, code: 0x4C });
    }

    #[test]
    fn test_parse_media() {
        let vol_down = Action::parse("volumedown").unwrap();
        assert_eq!(vol_down, Action::Media(0x00EA));

        let mute = Action::parse("mute").unwrap();
        assert_eq!(mute, Action::Media(0x00E2));
    }

    #[test]
    fn test_parse_mouse() {
        let wheel = Action::parse("wheelup").unwrap();
        assert_eq!(wheel, Action::MouseWheel { delta: 1 });

        let click = Action::parse("click").unwrap();
        assert_eq!(click, Action::MouseClick { button: 1 });
    }

    #[test]
    fn test_packet_structure() {
        let action = Action::parse("volumedown").unwrap();
        let packet = action.to_packet(key_id_for_knob(0, KnobEvent::RotateCCW), 0);
        assert_eq!(packet.len(), 64);
        assert_eq!(packet[0], 0x03);
        assert_eq!(packet[1], 0xFE);
        assert_eq!(packet[2], 16); // Knob 0 CCW ID
        assert_eq!(packet[3], 1);  // Layer 0 + 1
        assert_eq!(packet[4], 2);  // Media kind
    }

    #[test]
    fn test_led_packet() {
        let packet = build_led_packet(0, "backlight white").unwrap();
        assert_eq!(packet.len(), 64);
        assert_eq!(packet[0], 0x03);
        assert_eq!(packet[1], 0xFE);
        assert_eq!(packet[2], 0xB0);
        assert_eq!(packet[4], 1); // Backlight mode
        assert_eq!(packet[5], 255); // R
        assert_eq!(packet[6], 255); // G
        assert_eq!(packet[7], 255); // B
    }
}
