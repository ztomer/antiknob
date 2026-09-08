use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobEvent {
    RotateCCW,
    Press,
    RotateCW,
    HoldTwistL,
    HoldTwistR,
}

impl KnobEvent {
    /// Every gesture, in slot order.
    pub const ALL: [KnobEvent; GESTURES_PER_KNOB] = [
        KnobEvent::RotateCCW,
        KnobEvent::Press,
        KnobEvent::RotateCW,
        KnobEvent::HoldTwistL,
        KnobEvent::HoldTwistR,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RotateCCW => "ccw",
            Self::Press => "press",
            Self::RotateCW => "cw",
            Self::HoldTwistL => "hold_twist_l",
            Self::HoldTwistR => "hold_twist_r",
        }
    }
}

/// Slots one knob occupies. FIVE, not three.
///
/// Measured with `probe-gestures --map` on a VK01: distinct markers on keys
/// 1-6, all five gestures performed in a stated order, and the device named
/// them 2, 3, 4, 5, 6 for CCW, press, CW, hold-left, hold-right. Key 1 was
/// never driven.
pub const GESTURES_PER_KNOB: usize = 5;

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
        let media_code = crate::vocabulary::media_usage(&s.to_ascii_lowercase());
        if let Some(code) = media_code {
            return Ok(Action::Media(code));
        }

        // 4. Keyboard keys with modifiers (e.g., "cmd+c", "ctrl-alt-del", "a", "f5")
        let mut modifiers = 0u8;
        let parts: Vec<&str> = s.split(['+', '-']).collect();

        for part in &parts[..parts.len() - 1] {
            match crate::vocabulary::modifier_mask(&part.to_ascii_lowercase()) {
                Some(mask) => modifiers |= mask,
                None => return Err(anyhow!("Unknown modifier '{}' in '{}'", part, s)),
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
/// Knob slots continue the 1-based key-ID space after the buttons, so the
/// base is `button_count + 1` -- NOT a constant. A knob occupies FIVE slots,
/// not three: CCW, press, CW, hold-twist-left, hold-twist-right.
///
/// Measured 2026-09-08 with `probe-gestures --map`, which puts a distinct
/// marker on keys 1-6, asks for all five gestures in a stated order, and
/// reports which key each one drove. On this VK01:
///
/// ```text
/// key 1  never driven          <- at most one button, and it is not a gesture
/// key 2  twist CCW
/// key 3  press
/// key 4  twist CW
/// key 5  hold + twist LEFT
/// key 6  hold + twist RIGHT
/// ```
///
/// Two earlier models were wrong. A hardcoded base of 16 sent every knob
/// binding to slots the firmware never reads. Replacing it with
/// `button_count + 1` and a stride of THREE was closer but still wrong on
/// two counts: a knob spans five slots, and this device's layout declared
/// three buttons it does not have -- so `ccw` was written to key 4, which is
/// the CW gesture, and the declared buttons landed on keys 1-3, putting
/// `prev` on CCW and `next` on press. The knob did something for every
/// gesture, which is exactly why it went unnoticed.
///
/// Since keys 2-6 are all gestures, this device has AT MOST ONE button.
/// A layout that declares more pushes every gesture off by that many.
pub fn key_id_for_knob(button_count: usize, knob_index: usize, event: KnobEvent) -> u8 {
    let offset = match event {
        KnobEvent::RotateCCW => 0,
        KnobEvent::Press => 1,
        KnobEvent::RotateCW => 2,
        KnobEvent::HoldTwistL => 3,
        KnobEvent::HoldTwistR => 4,
    };
    (button_count as u8) + 1 + (knob_index as u8) * (GESTURES_PER_KNOB as u8) + offset
}

/// Look up a key name in the shared vocabulary.
///
/// The table used to BE this function's match arms, which meant the names
/// the CLI printed were a third, hand-kept copy that had already drifted.
fn parse_keycode(s: &str) -> Option<u8> {
    crate::vocabulary::keycode(s)
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
        assert_eq!(packet[2], 16); // Knob 0 CCW after 15 buttons
        assert_eq!(packet[3], 1); // Layer 0 + 1
        assert_eq!(packet[4], 2); // Media kind
    }

    /// The measured map. `probe-gestures --map` put a distinct marker on
    /// keys 1-6 of a real VK01, all five gestures were performed in order,
    /// and the device named 2, 3, 4, 5, 6. Key 1 was never driven.
    ///
    /// This is the layout the device HAS, so the test states it as the
    /// device answered rather than as the old three-button model assumed.
    #[test]
    fn the_five_gestures_are_keys_two_through_six_on_a_vk01() {
        let ids = KnobEvent::ALL.map(|e| key_id_for_knob(1, 0, e));
        assert_eq!(ids, [2, 3, 4, 5, 6]);
    }

    /// A knob spans five slots, so the next one starts five later. The old
    /// stride of three overlapped a second knob onto the first one's
    /// hold-twist gestures.
    #[test]
    fn a_second_knob_starts_five_slots_after_the_first() {
        assert_eq!(key_id_for_knob(1, 1, KnobEvent::RotateCCW), 7);
        assert_eq!(key_id_for_knob(1, 1, KnobEvent::HoldTwistR), 11);
        // Every gesture of knob 0 and knob 1 is distinct.
        let a: Vec<u8> = KnobEvent::ALL
            .iter()
            .map(|e| key_id_for_knob(1, 0, *e))
            .collect();
        let b: Vec<u8> = KnobEvent::ALL
            .iter()
            .map(|e| key_id_for_knob(1, 1, *e))
            .collect();
        assert!(a.iter().all(|k| !b.contains(k)), "{a:?} overlaps {b:?}");
    }

    /// The base still follows the declared buttons, so a layout that
    /// declares buttons the device does not have pushes every gesture off.
    /// That is how `ccw` came to be written to key 4 -- the CW gesture.
    #[test]
    fn a_wrongly_declared_button_count_shifts_every_gesture() {
        assert_eq!(key_id_for_knob(3, 0, KnobEvent::RotateCCW), 4);
        assert_eq!(key_id_for_knob(1, 0, KnobEvent::RotateCW), 4);
        // Same slot, different gesture: the whole defect in one line.
        assert_eq!(
            key_id_for_knob(3, 0, KnobEvent::RotateCCW),
            key_id_for_knob(1, 0, KnobEvent::RotateCW)
        );
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
