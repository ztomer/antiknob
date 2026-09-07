//! Preset migration: antiknob device presets to host layers (Phase 4).
//!
//! One-way bridge. Gesture map: ccw to twistL, press to press, cw to
//! twistR. The presets' press+twist entries (pccw/pcw) were never
//! functional on firmware; they are reinterpreted as holdTwistL/R, which
//! only the host translator can express. The physical button has no host
//! slot and is reported, never silently dropped.
//!
//! Device action strings go through `protocol::Action::parse` (single
//! source of truth for modifier bits and media usages); USB HID keycodes
//! then translate to CG keycodes via the explicit table below.

use super::{HostAction, HostConfig, HostLayer, MouseButton};
use crate::presets::{Preset, ALL_PRESETS};
use crate::protocol::Action;

/// Note printed by every migration run: press+twist never worked on the
/// device, so those entries become hold+twist host gestures.
pub const REINTERPRET_NOTE: &str =
    "pccw/pcw entries were device-side stubs; migrated as holdTwistL/R (host-only gestures)";

/// Scroll lines per wheel detent (matches the engine default).
const WHEEL_LINES: i32 = 3;

/// USB HID usage code to CG keycode. Covers every key the six presets use
/// plus letters, digits, and common punctuation for headroom.
fn usb_to_cg(code: u8) -> Option<u16> {
    Some(match code {
        0x04..=0x1D => {
            // Letters a-z are positional on the CG side, not sequential.
            const LETTERS: [u16; 26] = [
                0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13,
                7, 16, 6,
            ];
            LETTERS[(code - 0x04) as usize]
        }
        // Digits 1-0.
        0x1E => 18,
        0x1F => 19,
        0x20 => 20,
        0x21 => 21,
        0x22 => 23,
        0x23 => 22,
        0x24 => 26,
        0x25 => 28,
        0x26 => 25,
        0x27 => 29,
        0x28 => 36,  // enter/return
        0x29 => 53,  // esc
        0x2A => 51,  // backspace
        0x2B => 48,  // tab
        0x2C => 49,  // space
        0x2D => 27,  // minus
        0x2E => 24,  // equal
        0x2F => 33,  // leftbracket
        0x30 => 30,  // rightbracket
        0x31 => 42,  // backslash
        0x33 => 41,  // semicolon
        0x34 => 39,  // quote
        0x35 => 50,  // grave
        0x36 => 43,  // comma
        0x37 => 47,  // period
        0x38 => 44,  // slash
        0x3A => 122, // f1
        0x3B => 120, // f2
        0x3C => 99,  // f3
        0x3D => 118, // f4
        0x3E => 96,  // f5
        0x3F => 97,  // f6
        0x40 => 98,  // f7
        0x41 => 100, // f8
        0x42 => 101, // f9
        0x43 => 109, // f10
        0x44 => 103, // f11
        0x45 => 111, // f12
        // Navigation cluster.
        0x49 => 114, // insert (help/insert)
        0x4A => 115, // home
        0x4B => 116, // pageup
        0x4C => 117, // delete (forward delete)
        0x4D => 119, // end
        0x4E => 121, // pagedown
        0x4F => 124, // right
        0x50 => 123, // left
        0x51 => 125, // down
        0x52 => 126, // up
        _ => return None,
    })
}

/// USB HID modifier bits to canonical host modifier names. Left/right
/// collapse: synthetic CG events carry no side distinction.
fn usb_mods_to_names(mods: u8) -> Vec<String> {
    let mut out = Vec::new();
    if mods & (0x01 | 0x10) != 0 {
        out.push("ctrl".to_string());
    }
    if mods & (0x02 | 0x20) != 0 {
        out.push("shift".to_string());
    }
    if mods & (0x04 | 0x40) != 0 {
        out.push("alt".to_string());
    }
    if mods & (0x08 | 0x80) != 0 {
        out.push("cmd".to_string());
    }
    out
}

/// Convert one parsed device action to a host action. Returns the action
/// plus an optional warning (unmappable input degrades to None, never
/// errors: migration must be total).
fn device_action_to_host(action: &Action, original: &str) -> (HostAction, Option<String>) {
    match action {
        Action::Media(code) => {
            let aux = match code {
                0x00E9 => super::AuxKey::VolumeUp,
                0x00EA => super::AuxKey::VolumeDown,
                0x00E2 => super::AuxKey::Mute,
                0x00CD => super::AuxKey::PlayPause,
                0x00B5 => super::AuxKey::Next,
                0x00B6 => super::AuxKey::Previous,
                0x006F => super::AuxKey::BrightnessUp,
                0x0070 => super::AuxKey::BrightnessDown,
                _ => {
                    return (
                        HostAction::None,
                        Some(format!(
                            "'{}': no host media equivalent, left empty",
                            original
                        )),
                    );
                }
            };
            (HostAction::Aux { key: aux }, None)
        }
        Action::MouseClick { button } => {
            let btn = match button {
                1 => MouseButton::Left,
                2 => MouseButton::Right,
                _ => MouseButton::Middle,
            };
            (HostAction::MouseClick { button: btn }, None)
        }
        Action::MouseWheel { delta } => (scroll_for(*delta), None),
        Action::Key { modifiers, code } => match usb_to_cg(*code) {
            Some(cg) => (
                HostAction::KeyChord {
                    key: cg,
                    mods: usb_mods_to_names(*modifiers),
                    label: original.to_string(),
                },
                None,
            ),
            None => (
                HostAction::None,
                Some(format!(
                    "'{}': keycode has no CG mapping, left empty",
                    original
                )),
            ),
        },
    }
}

fn scroll_for(delta: i8) -> HostAction {
    HostAction::Scroll {
        lines: Some(if delta > 0 { WHEEL_LINES } else { -WHEEL_LINES }),
    }
}

/// One migrated preset: its host layer plus the device LED spec the
/// companion device layer should keep (applied via `antiknob led`).
pub struct MigratedPreset {
    pub layer: HostLayer,
    pub led_spec: String,
}

/// Convert one device-style action string ("cmd+c", "mute", "wheelup")
/// to a host action, degrading to None when unparseable or unmappable.
/// Used by the GUI keystroke recorder for host gestures.
pub fn device_str_to_host_action(value: &str) -> HostAction {
    match Action::parse(value) {
        Ok(action) => device_action_to_host(&action, value).0,
        Err(_) => HostAction::None,
    }
}

fn migrate_slot(value: &str, warnings: &mut Vec<String>) -> HostAction {
    match Action::parse(value) {
        Ok(action) => {
            let (host, warning) = device_action_to_host(&action, value);
            if let Some(w) = warning {
                warnings.push(w);
            }
            host
        }
        Err(e) => {
            warnings.push(format!("'{}': {}, left empty", value, e));
            HostAction::None
        }
    }
}

/// Migrate one preset. Never fails; problems land in `warnings` and the
/// physical button is always reported (it has no host slot).
pub fn migrate_preset(preset: &Preset) -> (MigratedPreset, Vec<String>) {
    let mut warnings = Vec::new();
    warnings.push(format!(
        "button '{}' has no host slot and was not migrated",
        preset.button
    ));
    let layer = HostLayer {
        name: preset.name.to_string(),
        twist_l: migrate_slot(preset.ccw, &mut warnings),
        twist_r: migrate_slot(preset.cw, &mut warnings),
        hold_twist_l: preset
            .pccw
            .map(|s| migrate_slot(s, &mut warnings))
            .unwrap_or_default(),
        hold_twist_r: preset
            .pcw
            .map(|s| migrate_slot(s, &mut warnings))
            .unwrap_or_default(),
        press: migrate_slot(preset.press, &mut warnings),
    };
    let led_spec = format!("mode{} {}", preset.led_mode, preset.led_color);
    (MigratedPreset { layer, led_spec }, warnings)
}

/// Migrate all six presets to a host config (layers in preset order).
pub fn migrate_all_presets() -> (HostConfig, Vec<(String, String)>, Vec<String>) {
    let mut layers = Vec::new();
    let mut leds = Vec::new();
    let mut warnings = Vec::new();
    for preset in ALL_PRESETS {
        let (migrated, mut w) = migrate_preset(preset);
        leds.push((migrated.layer.name.clone(), migrated.led_spec));
        layers.push(migrated.layer);
        warnings.append(&mut w);
    }
    let cfg = HostConfig {
        layers,
        ..HostConfig::default_config()
    };
    (cfg, leds, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_letters_digits_arrows_map_to_cg_positions() {
        assert_eq!(usb_to_cg(0x04), Some(0)); // a
        assert_eq!(usb_to_cg(0x05), Some(11)); // b
        assert_eq!(usb_to_cg(0x1A), Some(13)); // w
        assert_eq!(usb_to_cg(0x1E), Some(18)); // 1
        assert_eq!(usb_to_cg(0x27), Some(29)); // 0
        assert_eq!(usb_to_cg(0x50), Some(123)); // left
        assert_eq!(usb_to_cg(0x4F), Some(124)); // right
        assert_eq!(usb_to_cg(0x2C), Some(49)); // space
        assert_eq!(usb_to_cg(0x28), Some(36)); // enter
    }

    #[test]
    fn usb_modifiers_collapse_sides() {
        assert_eq!(usb_mods_to_names(0x01 | 0x04), vec!["ctrl", "alt"]);
        assert_eq!(usb_mods_to_names(0x10 | 0x80), vec!["ctrl", "cmd"]);
        assert!(usb_mods_to_names(0).is_empty());
    }

    #[test]
    fn media_master_migrates_to_aux_with_button_warning() {
        let preset = ALL_PRESETS.iter().find(|p| p.id == "media_master").unwrap();
        let (migrated, warnings) = migrate_preset(preset);
        assert_eq!(
            migrated.layer.twist_l,
            HostAction::Aux {
                key: crate::host::AuxKey::VolumeDown
            }
        );
        assert_eq!(
            migrated.layer.press,
            HostAction::Aux {
                key: crate::host::AuxKey::Mute
            }
        );
        // pccw/pcw stubs become hold-twist host gestures.
        assert_eq!(
            migrated.layer.hold_twist_l,
            HostAction::Aux {
                key: crate::host::AuxKey::Previous
            }
        );
        assert_eq!(migrated.led_spec, "mode1 white");
        assert!(warnings.iter().any(|w| w.contains("button")));
    }

    #[test]
    fn all_presets_migrate_without_empty_slots() {
        let (cfg, leds, warnings) = migrate_all_presets();
        assert_eq!(cfg.layers.len(), 6);
        assert_eq!(leds.len(), 6);
        for layer in &cfg.layers {
            for gesture in [
                crate::host::Gesture::TwistL,
                crate::host::Gesture::TwistR,
                crate::host::Gesture::Press,
            ] {
                assert_ne!(
                    *layer.action(gesture),
                    HostAction::None,
                    "layer '{}' has an empty core gesture",
                    layer.name
                );
            }
        }
        // Only the six expected button warnings.
        assert_eq!(warnings.len(), 6);
        // Migrated config still round-trips through the defensive loader.
        assert_eq!(HostConfig::load_json(&cfg.to_json_pretty()), cfg);
    }

    #[test]
    fn device_strings_convert_for_host_recorder() {
        assert_eq!(
            device_str_to_host_action("cmd+c"),
            HostAction::KeyChord {
                key: 8,
                mods: vec!["cmd".to_string()],
                label: "cmd+c".to_string(),
            }
        );
        assert_eq!(
            device_str_to_host_action("mute"),
            HostAction::Aux {
                key: crate::host::AuxKey::Mute
            }
        );
        assert_eq!(device_str_to_host_action("bogus-key-xyz"), HostAction::None);
    }

    #[test]
    fn browser_preset_uses_scroll_and_middle_click() {
        let preset = ALL_PRESETS
            .iter()
            .find(|p| p.id == "browser_reading")
            .unwrap();
        let (migrated, _) = migrate_preset(preset);
        assert_eq!(
            migrated.layer.twist_l,
            HostAction::Scroll { lines: Some(3) }
        );
        assert_eq!(
            migrated.layer.press,
            HostAction::MouseClick {
                button: MouseButton::Middle
            }
        );
    }
}
