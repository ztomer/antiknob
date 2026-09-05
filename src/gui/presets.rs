use crate::config::KnobConfig;
use crate::gui::state::GuiState;

#[derive(Debug, Clone)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub ccw: &'static str,
    pub press: &'static str,
    pub cw: &'static str,
    pub pccw: Option<&'static str>,
    pub pcw: Option<&'static str>,
    pub button: &'static str,
    pub led_mode: u8,
    pub led_color: &'static str,
}

pub const ALL_PRESETS: &[Preset] = &[
    Preset {
        id: "media_master",
        name: "Media Master",
        description: "Smooth volume control, mute on press, play/pause on button.",
        category: "Entertainment",
        ccw: "volumedown",
        press: "mute",
        cw: "volumeup",
        pccw: Some("prev"),
        pcw: Some("next"),
        button: "play",
        led_mode: 1,
        led_color: "white",
    },
    Preset {
        id: "video_scrubbing",
        name: "Video Scrubbing (FCP / Premiere)",
        description: "Frame-by-frame jog dial, play/pause toggle, blade cut tool.",
        category: "Video Editing",
        ccw: "left",
        press: "space",
        cw: "right",
        pccw: Some("shift+left"),
        pcw: Some("shift+right"),
        button: "cmd+b",
        led_mode: 1,
        led_color: "red",
    },
    Preset {
        id: "digital_art",
        name: "Digital Art (Photoshop / Procreate)",
        description: "Brush size resize, undo on press, quick brush tool select.",
        category: "Creative",
        ccw: "leftbracket",
        press: "cmd+z",
        cw: "rightbracket",
        pccw: Some("cmd+minus"),
        pcw: Some("cmd+equal"),
        button: "b",
        led_mode: 1,
        led_color: "cyan",
    },
    Preset {
        id: "spaces_window",
        name: "Spaces & Window Tiling",
        description: "Switch virtual desktops, Mission Control on press, hide active window.",
        category: "Productivity",
        ccw: "ctrl+left",
        press: "ctrl+up",
        cw: "ctrl+right",
        pccw: None,
        pcw: None,
        button: "cmd+h",
        led_mode: 1,
        led_color: "magenta",
    },
    Preset {
        id: "developer_git",
        name: "Developer & Terminal",
        description: "Code navigation back/forward, enter on press, interrupt/kill on button.",
        category: "Development",
        ccw: "cmd+leftbracket",
        press: "enter",
        cw: "cmd+rightbracket",
        pccw: None,
        pcw: None,
        button: "ctrl+c",
        led_mode: 1,
        led_color: "green",
    },
    Preset {
        id: "browser_reading",
        name: "Web & Reading",
        description: "Smooth page scroll, middle-click autoscroll, close tab on button.",
        category: "Browsing",
        ccw: "wheelup",
        press: "mclick",
        cw: "wheeldown",
        pccw: None,
        pcw: None,
        button: "cmd+w",
        led_mode: 1,
        led_color: "yellow",
    },
];

pub fn apply_preset(preset: &Preset, state: &mut GuiState) {
    if state.active_layer >= state.config.layers.len() {
        return;
    }
    let layer = &mut state.config.layers[state.active_layer];

    if layer.knobs.is_empty() {
        layer.knobs.push(KnobConfig {
            ccw: None,
            press: None,
            cw: None,
        });
    }

    layer.knobs[0].ccw = Some(preset.ccw.to_string());
    layer.knobs[0].press = Some(preset.press.to_string());
    layer.knobs[0].cw = Some(preset.cw.to_string());

    if layer.buttons.is_empty() {
        layer.buttons.push(vec![preset.button.to_string()]);
    } else if layer.buttons[0].is_empty() {
        layer.buttons[0].push(preset.button.to_string());
    } else {
        layer.buttons[0][0] = preset.button.to_string();
    }

    state.led_mode = preset.led_mode;
    state.led_color = preset.led_color.to_string();
    layer.led = Some(format!("mode{} {}", preset.led_mode, preset.led_color));

    state.status_message = format!(
        "Applied '{}' preset to Layer {}",
        preset.name, state.active_layer
    );
    state.status_is_ok = true;
}
