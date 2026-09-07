//! Reading the snoop stream: which devices to watch, and what a report says.
//!
//! Pure, because both halves were wrong in ways that only a decoded dump
//! would show, and a dump nobody can trust is worse than no dump:
//!
//! * The mouse branch read `buf[0]` as the button byte while the keyboard
//!   branch stripped a report ID first. Every mouse move therefore came out
//!   as `buttons=[left]`, and a 3-byte consumer report (`05 e9 00`) as
//!   `buttons=[left+middle] x=-23` -- three claims, none of them true.
//! * Consumer-page usages had no branch at all, so the one report the knob
//!   actually sends printed as raw hex with no note beside it.

/// A `VID:PID` pair to watch, as typed on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceFilter {
    pub vendor_id: u16,
    pub product_id: u16,
}

impl DeviceFilter {
    pub fn matches(&self, vendor_id: u16, product_id: u16) -> bool {
        self.vendor_id == vendor_id && self.product_id == product_id
    }
}

/// Parse `514c:8850` (or `0x514c:0x8850`) into a filter.
pub fn parse_device_filter(spec: &str) -> Result<DeviceFilter, String> {
    let (v, p) = spec
        .split_once(':')
        .ok_or_else(|| format!("expected VID:PID, got `{}`", spec))?;
    let hex = |s: &str| {
        let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
        u16::from_str_radix(t, 16).map_err(|_| format!("`{}` is not a 16-bit hex id", s))
    };
    Ok(DeviceFilter {
        vendor_id: hex(v)?,
        product_id: hex(p)?,
    })
}

/// True when no filter was given (watch everything supported) or one of
/// them names this device.
pub fn wanted(filters: &[DeviceFilter], vendor_id: u16, product_id: u16) -> bool {
    filters.is_empty() || filters.iter().any(|f| f.matches(vendor_id, product_id))
}

/// Short name for one top-level collection, for the snoop header.
///
/// An unknown pair keeps its hex rather than being dropped: a device that
/// grows a collection we have no name for should still be visible.
pub fn collection_name(usage_page: u16, usage: u16) -> String {
    match (usage_page, usage) {
        (0x0001, 0x0001) => "pointer".to_string(),
        (0x0001, 0x0002) => "mouse".to_string(),
        (0x0001, 0x0006) => "kbd".to_string(),
        (0x0001, 0x0080) => "sysctl".to_string(),
        (0x000C, 0x0001) => "consumer".to_string(),
        (page, usage) if page >= 0xFF00 => format!("vendor:{page:#06x}/{usage:#04x}"),
        (page, usage) => format!("{page:#06x}/{usage:#04x}"),
    }
}

/// The collection set as one readable phrase.
pub fn collections_label(usages: &[(u16, u16)]) -> String {
    let mut names: Vec<String> = usages
        .iter()
        .map(|(p, u)| collection_name(*p, *u))
        .collect();
    names.dedup();
    names.join(" ")
}

/// HID Consumer Page (0x0C) usages this hardware emits, by usage id.
const CONSUMER_USAGES: &[(u16, &str)] = &[
    (0x00B5, "scan next track"),
    (0x00B6, "scan previous track"),
    (0x00B7, "stop"),
    (0x00CD, "play/pause"),
    (0x00E2, "mute"),
    (0x00E9, "volume up"),
    (0x00EA, "volume down"),
    // HID Consumer page: 0x6F is Display Brightness Increment, 0x70 the
    // decrement -- the same codes `protocol::Action::parse` flashes for
    // "brightnessup"/"brightnessdown".
    (0x006F, "brightness up"),
    (0x0070, "brightness down"),
];

fn consumer_name(usage: u16) -> Option<&'static str> {
    CONSUMER_USAGES
        .iter()
        .find(|(u, _)| *u == usage)
        .map(|(_, n)| *n)
}

/// Split a report into (report_id, body).
///
/// hidapi hands back the report ID as the first byte when the device uses
/// numbered reports, and not at all when it does not -- and nothing in the
/// buffer says which. `expected_len` is how long the body is for this
/// interface's known layout, so a buffer one byte longer is one carrying an
/// ID. Guessing from `buf[0] == 0` (what the keyboard branch did) reads a
/// real report ID of 0x05 as part of the payload.
fn split_report(buf: &[u8], expected_len: usize) -> (Option<u8>, &[u8]) {
    if buf.len() == expected_len + 1 {
        (Some(buf[0]), &buf[1..])
    } else {
        (None, buf)
    }
}

fn modifier_names(mods: u8) -> Vec<&'static str> {
    let mut out = Vec::new();
    for (bit, name) in [
        (0x01, "ctrl"),
        (0x02, "shift"),
        (0x04, "alt"),
        (0x08, "cmd"),
        (0x10, "r-ctrl"),
        (0x20, "r-shift"),
        (0x40, "r-alt"),
        (0x80, "r-cmd"),
    ] {
        if mods & bit != 0 {
            out.push(name);
        }
    }
    out
}

/// Best-effort note beside the hex dump. Never fails; an unknown layout
/// returns an empty string rather than a confident wrong reading.
///
/// `usages` is every top-level collection the OPENED DEVICE declares, not
/// the one collection its handle happened to be enumerated under. One
/// IOHIDDevice carries all of them down a single handle, so a knob whose
/// first-listed collection is the keyboard still delivers consumer reports
/// there -- decoding by the handle's own usage would read `05 e9 00` as a
/// malformed keyboard report. Try only layouts the device actually
/// declares, and only the one whose report length fits.
pub fn decode_input(usages: &[(u16, u16)], buf: &[u8]) -> String {
    // Most specific first: a 3-byte consumer report and a 3-byte mouse
    // report are the same length, and only the consumer page carries a
    // 16-bit usage, so it must be offered the buffer first.
    const ORDER: [(u16, u16); 4] = [
        (0xFF00, 0x0001),
        (0x000C, 0x0001),
        (0x0001, 0x0006),
        (0x0001, 0x0002),
    ];
    for (page, usage) in ORDER {
        let declared = usages
            .iter()
            .any(|(p, u)| *p == page && (page != 0x0001 || *u == usage));
        if !declared {
            continue;
        }
        let note = decode_as(page, usage, buf);
        if !note.is_empty() {
            return note;
        }
    }
    String::new()
}

/// Decode `buf` as one specific collection's layout, or "" if it does not fit.
fn decode_as(usage_page: u16, usage: u16, buf: &[u8]) -> String {
    match (usage_page, usage) {
        // Consumer control: one 16-bit usage, little-endian, 0 on release.
        (0x000C, _) => {
            let (_, body) = split_report(buf, 2);
            if body.len() != 2 {
                return String::new();
            }
            let code = u16::from(body[0]) | (u16::from(body[1]) << 8);
            if code == 0 {
                return "<-- consumer release".to_string();
            }
            match consumer_name(code) {
                Some(name) => format!("<-- consumer {:#06x} ({})", code, name),
                None => format!("<-- consumer {:#06x}", code),
            }
        }
        // Keyboard boot protocol: mods, reserved, 6 keycodes.
        (0x0001, 0x0006) => {
            let (_, body) = split_report(buf, 8);
            if body.len() != 8 || body[1] != 0 {
                return String::new();
            }
            let keys: Vec<String> = body[2..8]
                .iter()
                .filter(|&&k| k != 0)
                .map(|k| format!("{:02x}", k))
                .collect();
            format!(
                "<-- keyboard mods=[{}] keys=[{}]",
                modifier_names(body[0]).join("+"),
                keys.join(" ")
            )
        }
        // Mouse boot protocol: buttons, dx, dy, then an optional wheel.
        (0x0001, 0x0002) => {
            let (_, body) = split_report(buf, 3);
            if !(3..=4).contains(&body.len()) {
                return String::new();
            }
            let mut names = Vec::new();
            for (bit, name) in [(0x01, "left"), (0x02, "right"), (0x04, "middle")] {
                if body[0] & bit != 0 {
                    names.push(name);
                }
            }
            format!(
                "<-- mouse buttons=[{}] dx={} dy={} wheel={}",
                names.join("+"),
                body[1] as i8,
                body[2] as i8,
                body.get(3).map_or(0, |w| *w as i8)
            )
        }
        (0xFF00, _) if buf.len() > 2 && buf[0] == 0x03 && (16..=27).contains(&buf[2]) => {
            format!("<-- vendor slot key_id={}", buf[2])
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_filters_parse_both_hex_spellings() {
        let a = parse_device_filter("514c:8850").unwrap();
        let b = parse_device_filter("0x514C:0X8850").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.vendor_id, 0x514C);
        assert_eq!(a.product_id, 0x8850);
        assert!(parse_device_filter("514c").is_err());
        assert!(parse_device_filter("zzzz:8850").is_err());
    }

    #[test]
    fn no_filter_watches_everything_and_a_filter_narrows() {
        let knob = parse_device_filter("514c:8850").unwrap();
        assert!(wanted(&[], 0x25A7, 0xFA11));
        assert!(wanted(&[knob], 0x514C, 0x8850));
        assert!(!wanted(&[knob], 0x25A7, 0xFA11));
    }

    /// The exact bytes the VK01 sends, captured from the device. The old
    /// decoder called this `mouse buttons=[left+middle] x=-23 wheel=0`.
    #[test]
    fn the_knobs_real_report_decodes_as_volume_up() {
        assert_eq!(
            decode_input(&[(0x000C, 0x0001)], &[0x05, 0xE9, 0x00]),
            "<-- consumer 0x00e9 (volume up)"
        );
        assert_eq!(
            decode_input(&[(0x000C, 0x0001)], &[0x05, 0x00, 0x00]),
            "<-- consumer release"
        );
        assert_eq!(
            decode_input(&[(0x000C, 0x0001)], &[0x05, 0xEA, 0x00]),
            "<-- consumer 0x00ea (volume down)"
        );
    }

    #[test]
    fn an_unknown_consumer_usage_reports_its_number_rather_than_guessing() {
        assert_eq!(
            decode_input(&[(0x000C, 0x0001)], &[0x05, 0x34, 0x12]),
            "<-- consumer 0x1234"
        );
    }

    /// A report ID in front must not be read as payload. The mouse branch
    /// used to do exactly that, inventing a held left button on every move.
    #[test]
    fn a_report_id_is_not_mistaken_for_the_button_byte() {
        // 4 bytes = report id 0x01 + 3-byte body: no buttons, dx=5.
        let with_id = decode_input(&[(0x0001, 0x0002)], &[0x01, 0x00, 0x05, 0x00]);
        assert_eq!(with_id, "<-- mouse buttons=[] dx=5 dy=0 wheel=0");
        // 3 bytes = no report id: left button genuinely held.
        let without_id = decode_input(&[(0x0001, 0x0002)], &[0x01, 0x00, 0x05]);
        assert_eq!(without_id, "<-- mouse buttons=[left] dx=0 dy=5 wheel=0");
    }

    #[test]
    fn keyboard_reports_decode_with_and_without_a_report_id() {
        // ctrl+alt+F16 (usage 0x6B) -- what a bound slot chord looks like.
        let body = [0x05u8, 0, 0x6B, 0, 0, 0, 0, 0];
        let expected = "<-- keyboard mods=[ctrl+alt] keys=[6b]";
        assert_eq!(decode_input(&[(0x0001, 0x0006)], &body), expected);
        let mut with_id = vec![0x01u8];
        with_id.extend_from_slice(&body);
        assert_eq!(decode_input(&[(0x0001, 0x0006)], &with_id), expected);
    }

    #[test]
    fn unknown_layouts_say_nothing_instead_of_guessing() {
        assert_eq!(decode_input(&[(0xFF00, 0x0001)], &[0x03, 0xFA, 0x01]), "");
        assert_eq!(decode_input(&[(0x0001, 0x0006)], &[0x05, 0x01, 0x6B]), "");
        assert_eq!(decode_input(&[(0x1234, 0x0001)], &[1, 2, 3]), "");
    }

    /// The regression that motivated decoding against DECLARED collections:
    /// the VK01's handle is enumerated under the keyboard collection, but
    /// the report it delivers is a consumer one.
    #[test]
    fn a_consumer_report_on_a_multi_collection_device_is_not_read_as_keyboard() {
        let vk01 = [
            (0x0001u16, 0x0006u16),
            (0x0001, 0x0002),
            (0x0001, 0x0001),
            (0x000C, 0x0001),
        ];
        assert_eq!(
            decode_input(&vk01, &[0x05, 0xE9, 0x00]),
            "<-- consumer 0x00e9 (volume up)"
        );
        // A device with no consumer collection must not borrow that reading.
        assert_eq!(decode_input(&[(0x0001, 0x0006)], &[0x05, 0xE9, 0x00]), "");
    }

    #[test]
    fn brightness_codes_match_what_the_firmware_is_flashed_with() {
        assert_eq!(
            decode_input(&[(0x000C, 0x0001)], &[0x05, 0x6F, 0x00]),
            "<-- consumer 0x006f (brightness up)"
        );
        assert_eq!(
            decode_input(&[(0x000C, 0x0001)], &[0x05, 0x70, 0x00]),
            "<-- consumer 0x0070 (brightness down)"
        );
    }

    #[test]
    fn collections_read_as_names_and_unknown_ones_keep_their_hex() {
        let vk01 = [
            (0x0001u16, 0x0006u16),
            (0x0001, 0x0002),
            (0x0001, 0x0001),
            (0x000C, 0x0001),
        ];
        assert_eq!(collections_label(&vk01), "kbd mouse pointer consumer");
        assert_eq!(collections_label(&[(0xFF00, 0x0001)]), "vendor:0xff00/0x01");
        // Nothing is dropped just because it has no name.
        assert_eq!(collections_label(&[(0x000D, 0x0004)]), "0x000d/0x04");
        assert_eq!(collections_label(&[]), "");
    }

    #[test]
    fn vendor_slot_replies_are_named() {
        assert_eq!(
            decode_input(&[(0xFF00, 0x0001)], &[0x03, 0xFA, 17, 0]),
            "<-- vendor slot key_id=17"
        );
    }
}
