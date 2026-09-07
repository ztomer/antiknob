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

        // 4. Keyboard keys with modifiers (e.g., "cmd+c", "ctrl-alt-del", "a", "f5")
        let mut modifiers = 0u8;
        let parts: Vec<&str> = s.split(['+', '-']).collect();

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
                // Bytes 9-10, not 11-12. Keyboard actions put their mods and
                // keycode at 11/12 and read back confirmed; media put its
                // usage there too and never did. The device stores media
                // codes at byte 9 -- every media slot read off real hardware
                // has it there (`03 FA 04 03 02 01 01 00 00 E9` is volume up)
                // -- so writes to 11/12 landed in fields the firmware does
                // not consult, and the slot kept whatever it already held.
                packet[4] = 2; // Kind = Media
                let [low, high] = code.to_le_bytes();
                packet[9] = low;
                packet[10] = high;
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

/// Slot key ID for one knob gesture.
///
/// Knob slots continue the 1-based key-ID space straight after the buttons,
/// so the base is `button_count + 1` -- NOT a constant. It used to be a
/// hardcoded 16, which is only right for a 15-key device; on this VK01
/// (3 buttons, 1 knob) the knob reads slots 4/5/6, so every knob binding
/// this tool ever flashed landed in 16/17/18, slots the firmware does not
/// read. Nothing failed: the packets were accepted, the slot table really
/// did change, and the knob went on doing whatever it did before. Read back
/// from a real device, layer 3:
///
/// ```text
/// key_id 1,2,3   ctrl-left, ctrl-up, ctrl-right   <- our buttons, live
/// key_id 4,5,6   volume up, prev, next            <- what the knob reads
/// key_id 16,17,18  cmd--, cmd-0, cmd-=            <- our knob writes, inert
/// ```
pub fn key_id_for_knob(button_count: usize, knob_index: usize, event: KnobEvent) -> u8 {
    let offset = match event {
        KnobEvent::RotateCCW => 0,
        KnobEvent::Press => 1,
        KnobEvent::RotateCW => 2,
    };
    (button_count as u8) + 1 + (knob_index as u8) * 3 + offset
}

fn parse_keycode(s: &str) -> Option<u8> {
    match s {
        // Letters a-z
        "a" => Some(0x04),
        "b" => Some(0x05),
        "c" => Some(0x06),
        "d" => Some(0x07),
        "e" => Some(0x08),
        "f" => Some(0x09),
        "g" => Some(0x0A),
        "h" => Some(0x0B),
        "i" => Some(0x0C),
        "j" => Some(0x0D),
        "k" => Some(0x0E),
        "l" => Some(0x0F),
        "m" => Some(0x10),
        "n" => Some(0x11),
        "o" => Some(0x12),
        "p" => Some(0x13),
        "q" => Some(0x14),
        "r" => Some(0x15),
        "s" => Some(0x16),
        "t" => Some(0x17),
        "u" => Some(0x18),
        "v" => Some(0x19),
        "w" => Some(0x1A),
        "x" => Some(0x1B),
        "y" => Some(0x1C),
        "z" => Some(0x1D),

        // Digits 1-0
        "1" => Some(0x1E),
        "2" => Some(0x1F),
        "3" => Some(0x20),
        "4" => Some(0x21),
        "5" => Some(0x22),
        "6" => Some(0x23),
        "7" => Some(0x24),
        "8" => Some(0x25),
        "9" => Some(0x26),
        "0" => Some(0x27),

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
        "f1" => Some(0x3A),
        "f2" => Some(0x3B),
        "f3" => Some(0x3C),
        "f4" => Some(0x3D),
        "f5" => Some(0x3E),
        "f6" => Some(0x3F),
        "f7" => Some(0x40),
        "f8" => Some(0x41),
        "f9" => Some(0x42),
        "f10" => Some(0x43),
        "f11" => Some(0x44),
        "f12" => Some(0x45),

        // Extended function keys F13-F24 (USB HID usage codes 0x68-0x73).
        // Needed for host-translate slot bindings (ctrl-alt-F16..F20).
        "f13" => Some(0x68),
        "f14" => Some(0x69),
        "f15" => Some(0x6A),
        "f16" => Some(0x6B),
        "f17" => Some(0x6C),
        "f18" => Some(0x6D),
        "f19" => Some(0x6E),
        "f20" => Some(0x6F),
        "f21" => Some(0x70),
        "f22" => Some(0x71),
        "f23" => Some(0x72),
        "f24" => Some(0x73),

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

// Re-exported so the many `protocol::build_led_packet` call sites keep
// working after the split; the implementation lives in `crate::led`.
pub use crate::led::{build_led_packet, LED_MODE_NAMES};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_with_modifiers() {
        let action = Action::parse("cmd-c").unwrap();
        assert_eq!(
            action,
            Action::Key {
                modifiers: 0x08,
                code: 0x06
            }
        );

        let combo = Action::parse("ctrl-alt-del").unwrap();
        assert_eq!(
            combo,
            Action::Key {
                modifiers: 0x01 | 0x04,
                code: 0x4C
            }
        );
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
    fn test_parse_extended_function_keys() {
        // USB HID: F13=0x68 .. F24=0x73.
        for (i, name) in ["f13", "f14", "f15", "f16", "f17", "f18", "f19", "f20"]
            .iter()
            .enumerate()
        {
            let action = Action::parse(name).unwrap();
            assert_eq!(
                action,
                Action::Key {
                    modifiers: 0,
                    code: 0x68 + i as u8
                },
                "keycode for {name}"
            );
        }
        // The host-translate slot chord.
        let slot = Action::parse("ctrl-alt-f16").unwrap();
        assert_eq!(
            slot,
            Action::Key {
                modifiers: 0x01 | 0x04,
                code: 0x6B
            }
        );
    }

    #[test]
    fn test_packet_structure() {
        let action = Action::parse("volumedown").unwrap();
        let packet = action.to_packet(key_id_for_knob(15, 0, KnobEvent::RotateCCW), 0);
        assert_eq!(packet.len(), 64);
        assert_eq!(packet[0], 0x03);
        assert_eq!(packet[1], 0xFE);
        assert_eq!(packet[2], 16); // Knob 0 CCW on a 15-key device
        assert_eq!(packet[3], 1); // Layer 0 + 1
        assert_eq!(packet[4], 2); // Media kind
    }

    /// The device that exposed the bug: 1 row of 3 buttons plus one knob.
    /// Knob slots must follow the buttons, not sit at a fixed 16.
    #[test]
    fn knob_slots_follow_the_buttons_rather_than_a_fixed_base() {
        let ids = |buttons: usize| {
            [KnobEvent::RotateCCW, KnobEvent::Press, KnobEvent::RotateCW]
                .map(|e| key_id_for_knob(buttons, 0, e))
        };
        // VK01: 3 buttons -> the knob reads 4/5/6 (read back from hardware).
        assert_eq!(ids(3), [4, 5, 6]);
        // A knob-only device has no buttons to follow.
        assert_eq!(ids(0), [1, 2, 3]);
        // 15-key macropad: the old hardcoded base was right only here.
        assert_eq!(ids(15), [16, 17, 18]);
    }

    #[test]
    fn a_second_knob_follows_the_first() {
        assert_eq!(key_id_for_knob(3, 1, KnobEvent::RotateCCW), 7);
        assert_eq!(key_id_for_knob(3, 1, KnobEvent::RotateCW), 9);
    }

    /// Buttons and knobs must never claim the same slot, whatever the layout.
    #[test]
    fn button_and_knob_slots_never_collide() {
        for buttons in 0..20usize {
            let last_button = (0..buttons).map(key_id_for_button).max();
            let first_knob = key_id_for_knob(buttons, 0, KnobEvent::RotateCCW);
            assert!(
                last_button.is_none_or(|b| b < first_knob),
                "{} buttons: last button {:?} collides with knob {}",
                buttons,
                last_button,
                first_knob
            );
        }
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

// Re-exported so the many  call sites keep
// working after the split; the implementation lives in .
