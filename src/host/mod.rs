//! Host-side translation config (Phase 1 of the vk01 hybrid adoption).
//!
//! Mirrors the vk01-anticater action vocabulary and `config.json` schema so
//! host layers built here behave identically there: the knob's five firmware
//! slots are bound once to `ctrl+alt+F16..F20`, and day-to-day behavior is
//! decided host-side by matching those chords to gestures. Pure logic, no
//! HID, no TCC, no threads.

pub mod bind;
pub mod engine;
pub mod grants;
pub mod login_item;
pub mod migrate;
pub mod output;
pub mod tap;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default host config location, shared by the daemon, the CLI importer,
/// and the GUI exporter so all three agree on one file.
pub fn default_config_path() -> Result<PathBuf, std::env::VarError> {
    Ok(
        PathBuf::from(std::env::var("HOME")?)
            .join("Library/Application Support/antiknob/host.json"),
    )
}

/// The five knob gestures, one per bound firmware slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gesture {
    TwistL,
    Press,
    TwistR,
    HoldTwistL,
    HoldTwistR,
}

/// A recorded keyboard chord: CG keycode plus lowercase modifier names
/// (`cmd`, `shift`, `opt`/`alt`, `ctrl`, `fn`). `label` is display-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordSpec {
    pub key: u16,
    pub mods: Vec<String>,
    #[serde(default)]
    pub label: String,
}

/// Ground truth shared with vk01-anticater: slot chord per gesture.
/// CG keycodes: F16=106, F17=64, F18=79, F19=80, F20=90. Required mods:
/// control + alternate. Must NOT be user-editable.
pub fn slot_chord(gesture: Gesture) -> (u16, [&'static str; 2]) {
    match gesture {
        Gesture::TwistL => (106, ["ctrl", "alt"]),
        Gesture::Press => (64, ["ctrl", "alt"]),
        Gesture::TwistR => (79, ["ctrl", "alt"]),
        Gesture::HoldTwistL => (80, ["ctrl", "alt"]),
        Gesture::HoldTwistR => (90, ["ctrl", "alt"]),
    }
}

/// True when an incoming chord is exactly one of the five bound slot
/// chords (keycode plus ctrl+alt, no more, no less).
pub fn is_slot_chord(key: u16, mods: &[&str]) -> bool {
    let mut sorted: Vec<&str> = mods.to_vec();
    sorted.sort_unstable();
    matches_slot(key, &sorted)
}

fn matches_slot(key: u16, sorted_mods: &[&str]) -> bool {
    let wants_alt = |m: &[&str]| m == ["alt", "ctrl"];
    match key {
        106 | 64 | 79 | 80 | 90 => wants_alt(sorted_mods),
        _ => false,
    }
}

/// Which gesture a slot chord represents, if any.
pub fn gesture_for_chord(key: u16, mods: &[&str]) -> Option<Gesture> {
    if !is_slot_chord(key, mods) {
        return None;
    }
    match key {
        106 => Some(Gesture::TwistL),
        64 => Some(Gesture::Press),
        79 => Some(Gesture::TwistR),
        80 => Some(Gesture::HoldTwistL),
        90 => Some(Gesture::HoldTwistR),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuxKey {
    VolumeUp,
    VolumeDown,
    Mute,
    PlayPause,
    Next,
    Previous,
    BrightnessUp,
    BrightnessDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeqStep {
    #[serde(default)]
    pub key: Option<u16>,
    #[serde(default)]
    pub mods: Vec<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub delay_ms: Option<u64>,
}

/// Host action vocabulary, JSON-tagged exactly like vk01-anticater so
/// configs are interchangeable. `label` fields are display-only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HostAction {
    #[default]
    None,
    Scroll {
        #[serde(default)]
        lines: Option<i32>,
    },
    KeyChord {
        key: u16,
        #[serde(default)]
        mods: Vec<String>,
        #[serde(default)]
        label: String,
    },
    Sequence {
        #[serde(default)]
        steps: Vec<SeqStep>,
    },
    Aux {
        key: AuxKey,
    },
    MouseClick {
        button: MouseButton,
    },
    LaunchApp {
        bundle_id: String,
    },
    OpenUrl {
        url: String,
    },
    OpenPath {
        path: String,
    },
    QuitApp {
        bundle_id: String,
        #[serde(default)]
        force: bool,
    },
    HotkeySwitch {
        #[serde(default)]
        first: Option<ChordSpec>,
        #[serde(default)]
        second: Option<ChordSpec>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostLayer {
    pub name: String,
    #[serde(default, alias = "twist_l")]
    pub twist_l: HostAction,
    #[serde(default, alias = "twist_r")]
    pub twist_r: HostAction,
    #[serde(default, alias = "hold_twist_l")]
    pub hold_twist_l: HostAction,
    #[serde(default, alias = "hold_twist_r")]
    pub hold_twist_r: HostAction,
    #[serde(default)]
    pub press: HostAction,
}

impl HostLayer {
    pub fn action(&self, gesture: Gesture) -> &HostAction {
        match gesture {
            Gesture::TwistL => &self.twist_l,
            Gesture::Press => &self.press,
            Gesture::TwistR => &self.twist_r,
            Gesture::HoldTwistL => &self.hold_twist_l,
            Gesture::HoldTwistR => &self.hold_twist_r,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostConfig {
    pub layers: Vec<HostLayer>,
    #[serde(default = "default_double_tap")]
    pub double_tap_switch: bool,
    #[serde(default = "default_tap_window")]
    pub double_tap_window: f64,
    #[serde(default)]
    pub layer_hotkey: Option<ChordSpec>,
    #[serde(default)]
    pub layer_hotkey_back: Option<ChordSpec>,
    #[serde(default = "default_scroll_lines")]
    pub scroll_lines_per_detent: i32,
}

fn default_double_tap() -> bool {
    true
}

fn default_tap_window() -> f64 {
    0.25
}

fn default_scroll_lines() -> i32 {
    3
}

impl HostConfig {
    /// Defaults replicate the vk01 proof-of-concept layers.
    pub fn default_config() -> Self {
        Self {
            layers: vec![
                HostLayer {
                    name: "Navigate".to_string(),
                    twist_l: HostAction::Scroll { lines: Some(-3) },
                    twist_r: HostAction::Scroll { lines: Some(3) },
                    hold_twist_l: HostAction::KeyChord {
                        key: 43,
                        mods: vec!["cmd".to_string()],
                        label: ",".to_string(),
                    },
                    hold_twist_r: HostAction::KeyChord {
                        key: 24,
                        mods: vec!["cmd".to_string()],
                        label: "=".to_string(),
                    },
                    press: HostAction::KeyChord {
                        key: 126,
                        mods: vec!["cmd".to_string()],
                        label: "Up".to_string(),
                    },
                },
                HostLayer {
                    name: "Media".to_string(),
                    twist_l: HostAction::Aux {
                        key: AuxKey::VolumeDown,
                    },
                    twist_r: HostAction::Aux {
                        key: AuxKey::VolumeUp,
                    },
                    hold_twist_l: HostAction::Aux {
                        key: AuxKey::BrightnessDown,
                    },
                    hold_twist_r: HostAction::Aux {
                        key: AuxKey::BrightnessUp,
                    },
                    press: HostAction::Aux { key: AuxKey::Mute },
                },
            ],
            double_tap_switch: true,
            double_tap_window: 0.25,
            layer_hotkey: None,
            layer_hotkey_back: None,
            scroll_lines_per_detent: 3,
        }
    }

    /// Defensive load: empty layers or parse failure fall back to defaults
    /// (the live daemon must never die on a bad config file).
    pub fn load_json(text: &str) -> Self {
        Self::try_load_json(text).unwrap_or_else(Self::default_config)
    }

    /// Strict load for watchers: `None` means "keep the last-good config"
    /// (e.g. a half-written file mid-save), never silent defaults.
    pub fn try_load_json(text: &str) -> Option<Self> {
        match serde_json::from_str::<HostConfig>(text) {
            Ok(cfg) if !cfg.layers.is_empty() => Some(cfg),
            _ => None,
        }
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// One-line human description of a host action for UI surfaces (GUI Host
/// tab, logs). Labels are display-only throughout the schema.
pub fn describe_host_action(action: &HostAction) -> String {
    match action {
        HostAction::None => "None".to_string(),
        HostAction::Scroll { lines } => {
            format!("Scroll {:+}", lines.unwrap_or(default_scroll_lines()))
        }
        HostAction::KeyChord { mods, label, .. } => {
            let prefix = if mods.is_empty() {
                String::new()
            } else {
                format!("{}+", mods.join("+"))
            };
            format!("Key {}{}", prefix, label)
        }
        HostAction::Sequence { steps } => format!("Sequence ({} steps)", steps.len()),
        HostAction::Aux { key } => format!("Media {:?}", key),
        HostAction::MouseClick { button } => format!("Click {:?}", button),
        HostAction::LaunchApp { bundle_id } => format!("Launch {}", bundle_id),
        HostAction::OpenUrl { url } => format!("Open {}", url),
        HostAction::OpenPath { path } => format!("Open {}", path),
        HostAction::QuitApp { bundle_id, force } => {
            format!("Quit {} (force={})", bundle_id, force)
        }
        HostAction::HotkeySwitch { first, second } => format!(
            "Hotkey switch ({} / {})",
            first.as_ref().map(|c| c.label.as_str()).unwrap_or("-"),
            second.as_ref().map(|c| c.label.as_str()).unwrap_or("-"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_chords_match_vk01_ground_truth() {
        assert_eq!(slot_chord(Gesture::TwistL).0, 106);
        assert_eq!(slot_chord(Gesture::Press).0, 64);
        assert_eq!(slot_chord(Gesture::TwistR).0, 79);
        assert_eq!(slot_chord(Gesture::HoldTwistL).0, 80);
        assert_eq!(slot_chord(Gesture::HoldTwistR).0, 90);
        for g in [
            Gesture::TwistL,
            Gesture::Press,
            Gesture::TwistR,
            Gesture::HoldTwistL,
            Gesture::HoldTwistR,
        ] {
            let (key, mods) = slot_chord(g);
            assert_eq!(mods, ["ctrl", "alt"]);
            assert_eq!(gesture_for_chord(key, &["ctrl", "alt"]), Some(g));
        }
    }

    #[test]
    fn chord_matching_rejects_wrong_mods_and_keys() {
        assert!(!is_slot_chord(106, &["ctrl"]));
        assert!(!is_slot_chord(106, &["ctrl", "alt", "cmd"]));
        assert!(!is_slot_chord(106, &["cmd", "alt"]));
        assert!(!is_slot_chord(105, &["ctrl", "alt"]));
        assert_eq!(gesture_for_chord(106, &["ctrl"]), None);
        // Modifier order must not matter.
        assert_eq!(
            gesture_for_chord(79, &["alt", "ctrl"]),
            Some(Gesture::TwistR)
        );
    }

    #[test]
    fn bad_or_empty_config_falls_back_to_defaults() {
        let defaults = HostConfig::default_config();
        assert_eq!(HostConfig::load_json("not json"), defaults);
        assert_eq!(HostConfig::load_json(r#"{"layers": []}"#), defaults);
        let ok = HostConfig::load_json(&defaults.to_json_pretty());
        assert_eq!(ok, defaults);
    }

    #[test]
    fn strict_load_distinguishes_failure_from_defaults() {
        assert!(HostConfig::try_load_json("not json").is_none());
        assert!(HostConfig::try_load_json(r#"{"layers": []}"#).is_none());
        let defaults = HostConfig::default_config();
        assert_eq!(
            HostConfig::try_load_json(&defaults.to_json_pretty()),
            Some(defaults)
        );
    }

    #[test]
    fn describe_covers_every_action_variant() {
        assert_eq!(describe_host_action(&HostAction::None), "None");
        assert_eq!(
            describe_host_action(&HostAction::Scroll { lines: Some(-3) }),
            "Scroll -3"
        );
        assert_eq!(
            describe_host_action(&HostAction::Scroll { lines: None }),
            "Scroll +3"
        );
        let chord = HostAction::KeyChord {
            key: 8,
            mods: vec!["cmd".to_string()],
            label: "C".to_string(),
        };
        assert_eq!(describe_host_action(&chord), "Key cmd+C");
        let seq = HostAction::Sequence { steps: vec![] };
        assert_eq!(describe_host_action(&seq), "Sequence (0 steps)");
        assert!(describe_host_action(&HostAction::Aux { key: AuxKey::Mute }).contains("Mute"));
    }

    #[test]
    fn action_json_tags_match_vk01_schema() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"type": "hotkeySwitch", "first": null}"#).unwrap();
        assert_eq!(v["type"], "hotkeySwitch");
        let a: HostAction = serde_json::from_value(v).unwrap();
        assert!(matches!(a, HostAction::HotkeySwitch { .. }));
        let seq: HostAction = serde_json::from_str(r#"{"type": "sequence", "steps": []}"#).unwrap();
        assert!(matches!(seq, HostAction::Sequence { .. }));
    }
}
