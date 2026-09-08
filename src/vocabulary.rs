//! The action vocabulary, as DATA.
//!
//! One table per kind, and the parser reads them. Before this the names
//! existed three times over -- once in `parse_keycode`'s match arms, once in
//! the media arms, and once as printed strings in the CLI's `show-keys` --
//! and they had already drifted: `fastforward`, `rewind` and `eject` were
//! parseable for a whole session while `show-keys` never mentioned them, so
//! nothing that read the help could have used them.
//!
//! Both surfaces render these tables now, so the CLI and the MCP tool cannot
//! disagree about what the device understands, and a name that is listed is
//! a name that parses -- there is a test for exactly that.

/// Modifier names and their HID bit masks.
pub const MODIFIER_NAMES: &[(&str, u8)] = &[
    ("ctrl", 0x01),
    ("shift", 0x02),
    ("alt", 0x04),
    ("opt", 0x04),
    ("cmd", 0x08),
    ("win", 0x08),
    ("rctrl", 0x10),
    ("rshift", 0x20),
    ("ralt", 0x40),
    ("ropt", 0x40),
    ("rcmd", 0x80),
    ("rwin", 0x80),
];

/// Key names and their USB HID usage codes.
pub const KEY_NAMES: &[(&str, u8)] = &[
    ("a", 0x04),
    ("b", 0x05),
    ("c", 0x06),
    ("d", 0x07),
    ("e", 0x08),
    ("f", 0x09),
    ("g", 0x0A),
    ("h", 0x0B),
    ("i", 0x0C),
    ("j", 0x0D),
    ("k", 0x0E),
    ("l", 0x0F),
    ("m", 0x10),
    ("n", 0x11),
    ("o", 0x12),
    ("p", 0x13),
    ("q", 0x14),
    ("r", 0x15),
    ("s", 0x16),
    ("t", 0x17),
    ("u", 0x18),
    ("v", 0x19),
    ("w", 0x1A),
    ("x", 0x1B),
    ("y", 0x1C),
    ("z", 0x1D),
    ("1", 0x1E),
    ("2", 0x1F),
    ("3", 0x20),
    ("4", 0x21),
    ("5", 0x22),
    ("6", 0x23),
    ("7", 0x24),
    ("8", 0x25),
    ("9", 0x26),
    ("0", 0x27),
    ("enter", 0x28),
    ("return", 0x28),
    ("esc", 0x29),
    ("escape", 0x29),
    ("backspace", 0x2A),
    ("tab", 0x2B),
    ("space", 0x2C),
    ("minus", 0x2D),
    ("equal", 0x2E),
    ("leftbracket", 0x2F),
    ("rightbracket", 0x30),
    ("backslash", 0x31),
    ("semicolon", 0x33),
    ("quote", 0x34),
    ("grave", 0x35),
    ("comma", 0x36),
    ("period", 0x37),
    ("dot", 0x37),
    ("slash", 0x38),
    ("capslock", 0x39),
    ("f1", 0x3A),
    ("f2", 0x3B),
    ("f3", 0x3C),
    ("f4", 0x3D),
    ("f5", 0x3E),
    ("f6", 0x3F),
    ("f7", 0x40),
    ("f8", 0x41),
    ("f9", 0x42),
    ("f10", 0x43),
    ("f11", 0x44),
    ("f12", 0x45),
    ("f13", 0x68),
    ("f14", 0x69),
    ("f15", 0x6A),
    ("f16", 0x6B),
    ("f17", 0x6C),
    ("f18", 0x6D),
    ("f19", 0x6E),
    ("f20", 0x6F),
    ("f21", 0x70),
    ("f22", 0x71),
    ("f23", 0x72),
    ("f24", 0x73),
    ("printscreen", 0x46),
    ("prtscn", 0x46),
    ("scrolllock", 0x47),
    ("pause", 0x48),
    ("insert", 0x49),
    ("ins", 0x49),
    ("home", 0x4A),
    ("pageup", 0x4B),
    ("pgup", 0x4B),
    ("delete", 0x4C),
    ("del", 0x4C),
    ("end", 0x4D),
    ("pagedown", 0x4E),
    ("pgdn", 0x4E),
    ("right", 0x4F),
    ("left", 0x50),
    ("down", 0x51),
    ("up", 0x52),
];

/// Media names and their 16-bit consumer usages.
pub const MEDIA_NAMES: &[(&str, u16)] = &[
    ("volumedown", 0x00EA),
    ("voldown", 0x00EA),
    ("volumeup", 0x00E9),
    ("volup", 0x00E9),
    ("mute", 0x00E2),
    ("play", 0x00CD),
    ("playpause", 0x00CD),
    ("next", 0x00B5),
    ("prev", 0x00B6),
    ("previous", 0x00B6),
    ("stop", 0x00B7),
    ("fastforward", 0x00B3),
    ("ff", 0x00B3),
    ("rewind", 0x00B4),
    ("rw", 0x00B4),
    ("eject", 0x00B8),
    ("brightnessup", 0x006F),
    ("brightnessdown", 0x0070),
];

/// Mouse action names. Their encodings differ between the two write
/// commands, so they are named here and encoded in `protocol`.
pub const MOUSE_NAMES: &[&str] = &[
    "click",
    "lclick",
    "rclick",
    "mclick",
    "wheelup",
    "wheeldown",
];

/// Look up a key name.
pub fn keycode(name: &str) -> Option<u8> {
    KEY_NAMES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Look up a media name.
pub fn media_usage(name: &str) -> Option<u16> {
    MEDIA_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
}

/// Look up a modifier name.
pub fn modifier_mask(name: &str) -> Option<u8> {
    MODIFIER_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, m)| *m)
}
