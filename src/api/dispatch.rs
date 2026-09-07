//! Central Command Dispatcher.
//!
//! Executes `Command` requests against the device, host config, and tap engine.

use super::types::{led_mode_name, Command, PowerStatus};
use crate::apps;
use crate::config::DeviceConfig;
use crate::device;
use crate::host::bind::{flash_slot_bindings, BIND_LAYERS};
use crate::host::tap::TapEngine;
use crate::host::HostConfig;
use crate::protocol::{self, Action};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

/// Tap health status for diagnostics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TapHealth {
    pub active: bool,
    pub error: Option<String>,
}

/// Execution context for API commands.
pub struct ApiContext {
    pub config_path: PathBuf,
    pub tap_engine: Option<Arc<Mutex<TapEngine>>>,
    pub tap_health: Arc<Mutex<TapHealth>>,
}

impl ApiContext {
    pub fn new(config_path: PathBuf, tap_engine: Option<Arc<Mutex<TapEngine>>>) -> Self {
        Self {
            config_path,
            tap_engine,
            tap_health: Arc::new(Mutex::new(TapHealth::default())),
        }
    }

    pub fn with_health(
        config_path: PathBuf,
        tap_engine: Option<Arc<Mutex<TapEngine>>>,
        tap_health: Arc<Mutex<TapHealth>>,
    ) -> Self {
        Self {
            config_path,
            tap_engine,
            tap_health,
        }
    }
}

/// Execute an API command and return a JSON result.
pub fn execute_command(ctx: &mut ApiContext, cmd: Command) -> Result<Value> {
    match cmd {
        Command::GetStatus {} => {
            let devices = device::list_devices().unwrap_or_default();
            let connected = !devices.is_empty();
            let primary = device::primary_transport(&devices);
            let transport_str = primary.map(|t| t.as_str()).unwrap_or("disconnected");
            let mut led_mode = None;
            let mut led_mode_str = None;
            if connected {
                if let Ok(m) = device::with_device(|dev| device::read_led_mode(dev, 0)) {
                    led_mode = Some(m);
                    led_mode_str = Some(led_mode_name(m).to_string());
                }
            }

            let (active_layer, layers_count) = match &ctx.tap_engine {
                Some(engine) => {
                    let lock = engine.lock().unwrap();
                    (lock.layer_idx(), lock.config().layers.len())
                }
                None => {
                    let cfg = read_config_or_default(&ctx.config_path);
                    (0, cfg.layers.len())
                }
            };

            let health = ctx.tap_health.lock().unwrap().clone();
            let power = PowerStatus::current(&devices);

            Ok(json!({
                "connected": connected,
                "transport": transport_str,
                "device_count": devices.len(),
                "devices": devices,
                "power": power,
                "active_layer": active_layer,
                "layers_count": layers_count,
                "led_mode": led_mode,
                "led_mode_name": led_mode_str,
                "tap_active": health.active,
                "tap_error": health.error,
            }))
        }

        Command::Ping {} => Ok(json!({})),

        Command::GetConfig {} => {
            let cfg = match &ctx.tap_engine {
                Some(engine) => engine.lock().unwrap().config().clone(),
                None => read_config_or_default(&ctx.config_path),
            };
            Ok(serde_json::to_value(&cfg)?)
        }

        Command::SetConfig { config } => {
            if let Some(parent) = ctx.config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&ctx.config_path, config.to_json_pretty())?;

            if let Some(engine) = &ctx.tap_engine {
                let mut lock = engine.lock().unwrap();
                lock.apply_config(config.clone());
            }

            Ok(json!({
                "ok": true,
                "layers_count": config.layers.len(),
                "message": format!("Config updated with {} layer(s)", config.layers.len())
            }))
        }

        Command::SetLayer { layer } => match &ctx.tap_engine {
            Some(engine) => {
                let mut lock = engine.lock().unwrap();
                let len = lock.config().layers.len();
                if layer >= len {
                    anyhow::bail!(
                        "Layer index {} out of range (max is {})",
                        layer,
                        len.saturating_sub(1)
                    );
                }
                let _events = lock.set_layer(layer);
                Ok(json!({
                    "ok": true,
                    "active_layer": layer,
                    "layer_name": lock.config().layers[layer].name
                }))
            }
            None => {
                anyhow::bail!("Daemon tap engine not running; cannot switch layer");
            }
        },

        Command::SetLed { layer, mode, color } => {
            if mode.len() > 64 {
                anyhow::bail!("LED mode string exceeds 64 characters");
            }
            if layer >= 16 {
                anyhow::bail!("Layer index {} exceeds maximum of 15", layer);
            }
            let spec = match color {
                Some(c) if !c.is_empty() => format!("{} {}", mode, c),
                _ => mode,
            };
            let packet = protocol::build_led_packet(layer, &spec)?;
            device::with_device(move |dev| {
                device::send_report(dev, &packet)?;
                device::send_commit(dev)
            })
            .context("Cannot drive the Anticater USB device")?;
            Ok(json!({
                "ok": true,
                "layer": layer,
                "spec": spec
            }))
        }

        Command::GetLed { layer } => {
            let mode = device::with_device(move |dev| device::read_led_mode(dev, layer))
                .context("Cannot read LED state from the Anticater USB device")?;
            Ok(json!({
                "layer": layer,
                "mode": mode,
                "mode_name": led_mode_name(mode)
            }))
        }

        Command::BindSlots { layers } => {
            let target_layers = layers.unwrap_or_else(|| BIND_LAYERS.to_vec());
            let flash_layers = target_layers.clone();
            let count = device::with_device(move |dev| flash_slot_bindings(dev, &flash_layers))
                .context("Cannot flash slot bindings to the Anticater USB device")?;
            Ok(json!({
                "ok": true,
                "flashed_slots": count,
                "layers": target_layers
            }))
        }

        Command::UploadKeymap { yaml, layer } => {
            if yaml.len() > 65_536 {
                anyhow::bail!("Keymap YAML exceeds 64KB size limit");
            }
            let cfg: DeviceConfig =
                serde_yaml::from_str(&yaml).context("Invalid keymap YAML configuration")?;
            cfg.validate()?;

            // Every packet is built up front so the HID job owns plain bytes
            // and borrows nothing from `cfg`.
            let selected: Vec<usize> = match layer {
                Some(l) => {
                    let idx = l as usize;
                    if idx >= cfg.layers.len() {
                        anyhow::bail!("Layer {} out of range", idx);
                    }
                    vec![idx]
                }
                None => (0..cfg.layers.len()).collect(),
            };

            let mut packets: Vec<Vec<u8>> = Vec::new();
            for layer_idx in selected {
                let lcfg = &cfg.layers[layer_idx];
                let layer_u8 = layer_idx as u8;
                let mut button_count = 0;
                for row in &lcfg.buttons {
                    for key_str in row {
                        let action = Action::parse(key_str)?;
                        let key_id = protocol::key_id_for_button(button_count);
                        packets.push(action.to_packet(key_id, layer_u8));
                        button_count += 1;
                    }
                }
                for (knob_idx, knob) in lcfg.knobs.iter().enumerate() {
                    if let Some(ref ccw_str) = knob.ccw {
                        let action = Action::parse(ccw_str)?;
                        let key_id =
                            protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::RotateCCW);
                        packets.push(action.to_packet(key_id, layer_u8));
                    }
                    if let Some(ref press_str) = knob.press {
                        let action = Action::parse(press_str)?;
                        let key_id =
                            protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::Press);
                        packets.push(action.to_packet(key_id, layer_u8));
                    }
                    if let Some(ref cw_str) = knob.cw {
                        let action = Action::parse(cw_str)?;
                        let key_id =
                            protocol::key_id_for_knob(knob_idx, protocol::KnobEvent::RotateCW);
                        packets.push(action.to_packet(key_id, layer_u8));
                    }
                }
                if let Some(ref led_mode) = lcfg.led {
                    packets.push(protocol::build_led_packet(layer_u8, led_mode)?);
                }
            }

            device::with_device(move |dev| {
                for packet in &packets {
                    device::send_report(dev, packet)?;
                    sleep(Duration::from_millis(10));
                }
                device::send_commit(dev)
            })
            .context("Cannot flash the keymap to the Anticater USB device")?;
            Ok(json!({
                "ok": true,
                "message": "Keymap flashed to hardware successfully"
            }))
        }

        Command::ListApps {} => {
            let dirs = apps::default_app_dirs();
            let found = apps::scan_app_dirs(&dirs);
            Ok(json!({
                "count": found.len(),
                "apps": found
            }))
        }

        Command::ReadSlots { group, counters } => {
            let ctrs = counters.unwrap_or_else(|| vec![1, 2, 3]);
            if ctrs.len() > 16 {
                anyhow::bail!("Slot counters query exceeds limit of 16 entries");
            }
            let grp = group.unwrap_or(0x0F);
            let results = device::with_device(move |dev| {
                let mut results = Vec::new();
                for c in ctrs {
                    match device::read_slot(dev, grp, c) {
                        Ok(bytes) => {
                            let hex: Vec<String> =
                                bytes.iter().map(|b| format!("{:02x}", b)).collect();
                            results.push(json!({
                                "group": grp,
                                "counter": c,
                                "length": bytes.len(),
                                "hex": hex.join(" "),
                                "ok": true
                            }));
                        }
                        Err(e) => {
                            results.push(json!({
                                "group": grp,
                                "counter": c,
                                "error": e.to_string(),
                                "ok": false
                            }));
                        }
                    }
                    sleep(Duration::from_millis(30));
                }
                Ok(results)
            })
            .context("Cannot read slots from the Anticater USB device")?;

            Ok(json!({
                "group": grp,
                "slots": results
            }))
        }

        Command::SendRaw { bytes } => {
            if bytes.len() > 64 {
                anyhow::bail!(
                    "Raw payload exceeds 64-byte limit (received {} elements)",
                    bytes.len()
                );
            }
            let payload: Result<Vec<u8>, _> = bytes
                .iter()
                .map(|s| {
                    let s = s
                        .strip_prefix("0x")
                        .or_else(|| s.strip_prefix("0X"))
                        .unwrap_or(s);
                    u8::from_str_radix(s, 16).map_err(|_| anyhow::anyhow!("Bad hex byte '{}'", s))
                })
                .collect();
            let payload = payload?;
            let sent = payload.len();
            device::with_device(move |dev| device::send_report(dev, &payload))
                .context("Cannot write to the Anticater USB device")?;
            Ok(json!({
                "ok": true,
                "bytes_sent": sent
            }))
        }
    }
}

fn read_config_or_default(path: &Path) -> HostConfig {
    if let Ok(text) = std::fs::read_to_string(path) {
        HostConfig::load_json(&text)
    } else {
        HostConfig::default_config()
    }
}
