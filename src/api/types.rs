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
                    },
                    "buttons": {
                        "type": "integer",
                        "description": "The device's physical button count (VK01 = 3). Knob slot IDs follow the buttons, so a wrong count writes bindings the firmware never reads without reporting an error. Defaults to the installed layout's count."
                    }
                }
            }),
        },
        ToolDef {
            name: "get_knob_mode",
            description: "Read the knob's firmware slot table and report whether its gestures send host slot chords (host-translate, so host layers run) or ordinary actions (standalone, so host layers cannot fire). Returns 'unknown' when the table says nothing about the knob rather than guessing.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "buttons": {
                        "type": "integer",
                        "description": "The device's physical button count (VK01 = 3). Decides which slots are the knob's; defaults to the installed layout's."
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
        ToolDef {
            name: "read_slots",
            description: "Read slot table memory dump from the Anticater VK01 device.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "group": {
                        "type": "integer",
                        "description": "Optional slot table group (default 0x0F = 15; alternate 0x19 = 25)"
                    },
                    "counters": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Optional list of slot counters to read (defaults to [1, 2, 3])"
                    }
                }
            }),
        },
        ToolDef {
            name: "send_raw",
            description: "Send a raw 64-byte HID report payload to the Anticater VK01 device.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "bytes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Hex byte strings (e.g. ['0xFD', '0xFE', '0xFF'])"
                    }
                },
                "required": ["bytes"]
            }),
        },
        ToolDef {
            name: "ping",
            description: "Application-level liveness probe confirming the daemon is responsive.",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

/// Device power and battery status telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PowerStatus {
    pub source: String,
    pub transport: String,
    pub battery_percent: Option<u8>,
    pub description: String,
}

impl PowerStatus {
    pub fn current(devices: &[crate::device::DeviceMatch]) -> Self {
        if let Some(transport) = crate::device::primary_transport(devices) {
            match transport {
                crate::device::TransportType::Usb => Self {
                    source: "usb_bus".to_string(),
                    transport: "usb".to_string(),
                    battery_percent: None,
                    description: "Wired (USB Bus Powered)".to_string(),
                },
                crate::device::TransportType::Wireless24G => Self {
                    source: "battery".to_string(),
                    transport: "wireless_2_4g".to_string(),
                    battery_percent: None,
                    description: "2.4GHz Wireless (Battery Powered)".to_string(),
                },
                crate::device::TransportType::Bluetooth => Self {
                    source: "battery".to_string(),
                    transport: "bluetooth".to_string(),
                    battery_percent: None,
                    description: "Bluetooth Wireless (Battery Powered)".to_string(),
                },
            }
        } else {
            Self {
                source: "disconnected".to_string(),
                transport: "disconnected".to_string(),
                battery_percent: None,
                description: "Disconnected".to_string(),
            }
        }
    }
}

/// Strongly-typed command parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Command {
    #[serde(rename = "get_status")]
    GetStatus {},

    #[serde(rename = "ping")]
    Ping {},

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
    BindSlots {
        layers: Option<Vec<u8>>,
        /// Physical button count; knob slot IDs follow the buttons. No
        /// default: a wrong count writes bindings nothing reads.
        #[serde(default)]
        buttons: Option<usize>,
    },

    /// Read the firmware's slot table and say whether the knob's gestures
    /// can reach the host layers at all.
    #[serde(rename = "get_knob_mode")]
    GetKnobMode {
        #[serde(default)]
        buttons: Option<usize>,
    },

    #[serde(rename = "upload_keymap")]
    UploadKeymap { yaml: String, layer: Option<u8> },

    #[serde(rename = "list_apps")]
    ListApps {},

    #[serde(rename = "read_slots")]
    ReadSlots {
        group: Option<u8>,
        counters: Option<Vec<u8>>,
    },

    #[serde(rename = "send_raw")]
    SendRaw { bytes: Vec<String> },
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
