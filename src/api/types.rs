//! Unified API Command types and metadata.
//!
//! Serves as the single source of truth for both the Unix domain socket RPC
//! interface and the MCP (Model Context Protocol) server.

use crate::host::HostConfig;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Tool/Command definition containing schema and documentation.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// All available API tools / RPC commands generated from single source of truth.
pub fn all_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_status",
            description: "Get current Anticater VK01 hardware status, active host layer, and LED mode.",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "get_config",
            description: "Retrieve the current host-side layers configuration JSON.",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "set_config",
            description: "Update the host layers configuration JSON, applying changes immediately and persisting them.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "config": {
                        "type": "object",
                        "description": "Full HostConfig object"
                    }
                },
                "required": ["config"]
            }),
        },
        ToolDef {
            name: "set_layer",
            description: "Switch the active host layer immediately.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "layer": {
                        "type": "integer",
                        "description": "0-based layer index"
                    }
                },
                "required": ["layer"]
            }),
        },
        ToolDef {
            name: "set_led",
            description: "Set hardware LED lighting mode and color on the Anticater knob (e.g. mode 'backlight' color 'cyan', or mode 'off').",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "layer": {
                        "type": "integer",
                        "description": "0-based device layer (0..=2)",
                        "default": 0
                    },
                    "mode": {
                        "type": "string",
                        "description": "LED mode: 'off', 'backlight'/'mode1', 'shock'/'mode2', 'shock2'/'mode3', 'press'/'mode4'"
                    },
                    "color": {
                        "type": "string",
                        "description": "Optional color name: 'white', 'red', 'orange', 'yellow', 'green', 'cyan', 'blue', 'purple'"
                    }
                },
                "required": ["mode"]
            }),
        },
        ToolDef {
            name: "get_led",
            description: "Read back the current hardware LED mode and status for a device layer.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "layer": {
                        "type": "integer",
                        "description": "0-based device layer (0..=2)",
                        "default": 0
                    }
                }
            }),
        },
        ToolDef {
            name: "bind_slots",
            description: "Flash one-time host slot bindings (ctrl-alt-F16..F18) to the knob firmware so the host daemon can translate gestures.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "layers": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Optional list of layers to bind. Defaults to all [0, 1, 2]."
                    }
                }
            }),
        },
        ToolDef {
            name: "upload_keymap",
            description: "Flash a hardware keymap YAML configuration to the device firmware.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "yaml": {
                        "type": "string",
                        "description": "Valid DeviceConfig YAML string"
                    },
                    "layer": {
                        "type": "integer",
                        "description": "Optional single layer to flash"
                    }
                },
                "required": ["yaml"]
            }),
        },
        ToolDef {
            name: "list_apps",
            description: "List installed macOS applications and their bundle IDs for launch/quit actions.",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

/// Strongly-typed command parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Command {
    #[serde(rename = "get_status")]
    GetStatus {},

    #[serde(rename = "get_config")]
    GetConfig {},

    #[serde(rename = "set_config")]
    SetConfig { config: HostConfig },

    #[serde(rename = "set_layer")]
    SetLayer { layer: usize },

    #[serde(rename = "set_led")]
    SetLed {
        #[serde(default)]
        layer: u8,
        mode: String,
        color: Option<String>,
    },

    #[serde(rename = "get_led")]
    GetLed {
        #[serde(default)]
        layer: u8,
    },

    #[serde(rename = "bind_slots")]
    BindSlots { layers: Option<Vec<u8>> },

    #[serde(rename = "upload_keymap")]
    UploadKeymap { yaml: String, layer: Option<u8> },

    #[serde(rename = "list_apps")]
    ListApps {},
}

/// Format human-readable LED mode string.
pub fn led_mode_name(mode: u8) -> &'static str {
    match mode {
        0 => "off",
        1 => "backlight",
        2 => "shock",
        3 => "shock2",
        4 => "press",
        5 => "custom",
        _ => "unknown",
    }
}
