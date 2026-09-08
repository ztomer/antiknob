//! Unified API Command types and metadata.
//!
//! Serves as the single source of truth for both the Unix domain socket RPC
//! interface and the MCP (Model Context Protocol) server.

use super::registry;
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
/// Every MCP tool, GENERATED from `registry::COMMANDS`.
///
/// This used to be eighteen hand-written `ToolDef` literals sitting a
/// thousand lines away from the CLI's own definition of the same commands.
/// They drifted, as two hand-written lists do. Now there is one table and
/// two renderers, so a command cannot exist on one surface and not the
/// other without the table saying why.
pub fn all_tools() -> Vec<ToolDef> {
    registry::for_surface(true).map(tool_def).collect()
}

fn tool_def(spec: &'static registry::CommandSpec) -> ToolDef {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<String> = Vec::new();
    for param in spec.params.iter().filter(|p| p.on_surface(true)) {
        let ty = match param.kind {
            registry::Kind::Flag => json!({ "type": "boolean" }),
            registry::Kind::Str => json!({ "type": "string" }),
            registry::Kind::Int => json!({ "type": "integer" }),
            registry::Kind::IntList => json!({
                "type": "array", "items": { "type": "integer" }
            }),
            registry::Kind::StrList => json!({
                "type": "array", "items": { "type": "string" }
            }),
        };
        let mut schema = ty.as_object().expect("type schema is an object").clone();
        schema.insert("description".into(), json!(param.about));
        if let Some(default) = param.default {
            schema.insert("default".into(), json!(default));
        }
        properties.insert(param.name.to_string(), serde_json::Value::Object(schema));
        if param.required {
            required.push(param.name.to_string());
        }
    }
    let mut input_schema = json!({ "type": "object", "properties": properties });
    if !required.is_empty() {
        input_schema["required"] = json!(required);
    }
    ToolDef {
        name: spec.name,
        description: spec.about,
        input_schema,
    }
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

    /// Describe every command this build has. The settings app renders it
    /// instead of a hand-kept list of its own, which had drifted to eleven
    /// entries against seventeen real ones -- naming a `list_devices` that
    /// does not exist and three parameters by the wrong name.
    #[serde(rename = "list_commands")]
    ListCommands {},

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

    /// Report the active layer's variants, which one is live, and why.
    #[serde(rename = "get_virtual_layer")]
    GetVirtualLayer {},

    /// Pin a variant by name, or clear the pin with `null`.
    #[serde(rename = "set_virtual_variant")]
    SetVirtualVariant {
        #[serde(default)]
        variant: Option<String>,
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
        /// Read the WHOLE table in three burst queries instead of walking
        /// slots one at a time. Sees every key, not just the ones the
        /// declared layout admits to.
        #[serde(default)]
        full: bool,
    },

    #[serde(rename = "send_raw")]
    SendRaw { bytes: Vec<String> },

    #[serde(rename = "bind_sequence")]
    BindSequence {
        key: u8,
        #[serde(default)]
        layer: u8,
        actions: Vec<String>,
        #[serde(default)]
        delay_ms: u16,
    },

    #[serde(rename = "show_keys")]
    ShowKeys {},
}

/// Format human-readable LED mode string.
/// Mode names for the `514c:8850`, from hardware testing in
/// kriomant/ch57x-keyboard-tool#173 and confirmed on this knob by watching
/// each one. The old table came from the `1189:884x` family and named mode 4
/// "press" -- it is the rainbow, which is the multicoloured effect this
/// device ships in.
pub fn led_mode_name(mode: u8) -> &'static str {
    match mode {
        0 => "off",
        // Modes 1 and 2 are fixed colours on this device, not the effects
        // the reference project's table names them for.
        1 => "red",
        2 => "green",
        3 => "ripple",
        4 => "rainbow",
        // Sent by the vendor app like any other mode, captured on this
        // hardware. It was reported here as unsupported on the strength of
        // one wedged LED renderer that mode 5 did not cause.
        5 => "rgb",
        _ => "unknown",
    }
}
