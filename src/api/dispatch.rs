//! Central Command Dispatcher.
//!
//! Executes `Command` requests against the device, host config, and tap engine.

use super::types::{led_mode_name, Command, PowerStatus};
use crate::apps;
use crate::config::DeviceConfig;
use crate::device;
use crate::firmware::DEVICE_LAYERS;
use crate::host::HostConfig;
use crate::protocol::{self, Action};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

/// The button count from the installed layout, which is where every other
/// command gets it. Not a constant: it decides which slots are the knob's.
fn installed_button_count() -> Result<usize> {
    let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
    let path = crate::config::resolve_device_config_path(&home, None)?;
    Ok(DeviceConfig::load_from_file(&path)?.button_count())
}

#[path = "context.rs"]
mod context;
pub use context::{ApiContext, TapHealth};

#[path = "describe.rs"]
mod describe;
use describe::describe_commands;

#[path = "device_cmds.rs"]
mod device_cmds;
use device_cmds::{bind_sequence, bind_slots, read_slots, set_led, vocabulary_reply};

/// Execute an API command and return a JSON result.
///
/// # Errors
///
/// Fails when the command's own handler fails: no device attached, a write
/// the device did not confirm, or an argument that does not validate.
///
/// # Panics
///
/// When the tap engine's or the tap health's mutex was poisoned by a panic
/// on another thread.
pub fn execute_command(ctx: &mut ApiContext, cmd: Command) -> Result<Value> {
    match cmd {
        Command::GetStatus {} => Ok(get_status(ctx)),

        Command::Ping {} => Ok(json!({})),

        Command::ListCommands {} => Ok(json!({ "commands": describe_commands() })),

        Command::GetConfig {} => {
            let cfg = match &ctx.tap_engine {
                Some(engine) => engine.lock().unwrap().config().clone(),
                None => read_config_or_default(&ctx.config_path),
            };
            Ok(serde_json::to_value(&cfg)?)
        }

        Command::SetConfig { config } => set_config(ctx, config),

        Command::SetLayer { layer } => set_layer(ctx, layer),

        Command::GetVirtualLayer {} => get_virtual_layer(ctx),

        Command::SetVirtualVariant { variant } => set_virtual_variant(ctx, variant.as_deref()),

        Command::SetLed { layer, mode, color } => set_led(layer, mode, color),

        Command::GetLed { layer } => {
            let mode = device::with_device(move |dev| device::read_led_mode(dev, layer))
                .context("Cannot read LED state from the Anticater USB device")?;
            Ok(json!({
                "layer": layer,
                "mode": mode,
                "mode_name": led_mode_name(mode)
            }))
        }

        Command::GetKnobMode { buttons } => get_knob_mode(ctx, buttons),

        Command::BindSlots {
            layers,
            layer,
            buttons,
        } => bind_slots(ctx, layers, layer, buttons),

        Command::UploadKeymap { yaml, layer } => upload_keymap(ctx, &yaml, layer),

        Command::ListApps {} => {
            let dirs = apps::default_app_dirs();
            let found = apps::scan_app_dirs(&dirs);
            Ok(json!({
                "count": found.len(),
                "apps": found
            }))
        }

        Command::ReadSlots {
            group,
            counters,
            full,
        } => read_slots(group, counters, full),

        Command::ShowKeys {} => Ok(vocabulary_reply()),

        Command::BindSequence {
            key,
            layer,
            actions,
            delay_ms,
        } => bind_sequence(key, layer, &actions, delay_ms),

        Command::SendRaw { bytes } => {
            if bytes.len() > crate::policy::RAW_PAYLOAD_MAX_LEN {
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
                    u8::from_str_radix(s, 16).map_err(|_| anyhow::anyhow!("Bad hex byte '{s}'"))
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

fn get_status(ctx: &ApiContext) -> Value {
    let devices = device::list_devices().unwrap_or_default();
    let connected = !devices.is_empty();
    let primary = device::primary_transport(&devices);
    let transport_str = primary.map_or("disconnected", |t| t.as_str());
    let mut led_mode = None;
    let mut led_mode_str = None;
    if connected {
        if let Ok(m) = device::with_device(|dev| device::read_led_mode(dev, 0)) {
            led_mode = Some(m);
            led_mode_str = Some(led_mode_name(m).to_string());
        }
    }

    let (active_layer, layers_count) = if let Some(engine) = &ctx.tap_engine {
        let lock = engine.lock().unwrap();
        (lock.layer_idx(), lock.config().layers.len())
    } else {
        let cfg = read_config_or_default(&ctx.config_path);
        (0, cfg.layers.len())
    };

    let health = ctx.tap_health.lock().unwrap().clone();
    let power = PowerStatus::current(&devices);

    // WHICH interface the commands go to, named alongside the full
    // list. Every caller that wanted "the device" was taking
    // `devices[0]`, which is whichever HID interface enumerated
    // first -- a keyboard endpoint on this hardware, not the vendor
    // endpoint the tool actually drives.
    let primary_device = device::primary_device(&devices);

    json!({
        "connected": connected,
        "transport": transport_str,
        "device_count": devices.len(),
        "devices": devices,
        "primary_device": primary_device,
        "power": power,
        "active_layer": active_layer,
        "layers_count": layers_count,
        "led_mode": led_mode,
        "led_mode_name": led_mode_str,
        "tap_active": health.active,
        "tap_error": health.error,
        // Reported beside `tap_active` because it is the half that
        // can falsify it: a tap inside its run loop always claims
        // active, and only this count distinguishes listening from
        // deaf.
        "tap_events_seen": health.events_seen,
    })
}

fn set_config(ctx: &ApiContext, mut config: HostConfig) -> Result<Value> {
    let active_sync = if let Some(engine) = &ctx.tap_engine {
        let mut lock = engine.lock().unwrap();
        if config.bound_device_layers.is_empty() && !lock.config().bound_device_layers.is_empty() {
            config
                .bound_device_layers
                .clone_from(&lock.config().bound_device_layers);
        }
        lock.apply_config(config.clone());
        Some((lock.config().clone(), lock.layer_idx()))
    } else {
        None
    };

    // Keeps the version it replaces, writing safely through a rename.
    crate::host::config_backup::write_with_backup(&ctx.config_path, &config.to_json_pretty())?;

    if let Some((cfg, layer)) = active_sync {
        crate::host::led_sync::sync_led(&cfg, layer);
    }

    Ok(json!({
        "ok": true,
        "layers_count": config.layers.len(),
        "message": format!("Config updated with {} layer(s)", config.layers.len())
    }))
}

fn set_layer(ctx: &ApiContext, layer: usize) -> Result<Value> {
    match &ctx.tap_engine {
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
            // The backlight follows the active host layer here too. A
            // switch from the menu bar and a switch from the knob are
            // the same switch, and only one of them changing the light
            // would be the app disagreeing with itself.
            crate::host::led_sync::sync_led(lock.config(), layer);
            Ok(json!({
                "ok": true,
                "active_layer": layer,
                "layer_name": lock.config().layers[layer].name
            }))
        }
        None => {
            anyhow::bail!("Daemon tap engine not running; cannot switch layer");
        }
    }
}

fn get_virtual_layer(ctx: &ApiContext) -> Result<Value> {
    match &ctx.tap_engine {
        Some(engine) => {
            let lock = engine.lock().unwrap();
            let idx = lock.layer_idx();
            let layer = lock
                .config()
                .layers
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("no layer at index {idx}"))?;
            let resolution = lock.resolution();
            // Which DEVICE layer the daemon actually hears. A virtual
            // layer on an unbound device is configured perfectly and
            // fires never, so the caller is told the arrangement rather
            // than left to infer it from silence.
            let arrangement = crate::host::device_binding::arrangement(
                &lock.config().bound_device_layers,
                crate::firmware::DEVICE_LAYERS,
            );
            Ok(json!({
                "layer": layer.name,
                "layer_index": idx,
                "is_virtual": layer.is_virtual(),
                "device_binding": arrangement,
                "device_binding_summary": arrangement.describe(),
                "can_fire": arrangement.daemon_can_hear_the_knob(),
                "variants": layer.variants.iter().map(|v| json!({
                    "name": v.name,
                    "apps": v.apps,
                })).collect::<Vec<_>>(),
                "active_variant": resolution.variant_name(),
                // The reason matters as much as the answer: "which
                // variant" alone leaves the caller guessing whether
                // their pin took or an app rule happened to agree.
                "resolution": resolution,
                "override": lock.variant_override(),
            }))
        }
        None => anyhow::bail!("Daemon tap engine not running; cannot read the virtual layer"),
    }
}

fn set_virtual_variant(ctx: &ApiContext, variant: Option<&str>) -> Result<Value> {
    match &ctx.tap_engine {
        Some(engine) => {
            let mut lock = engine.lock().unwrap();
            let idx = lock.layer_idx();
            // A name that matches nothing is refused, not stored. The
            // resolver would fall back to the layer's own bindings and
            // the caller would be told "ok" for a pin that never took.
            if let Some(name) = variant {
                let known: Vec<String> = lock
                    .config()
                    .layers
                    .get(idx)
                    .map(|l| l.variants.iter().map(|v| v.name.clone()).collect())
                    .unwrap_or_default();
                if !known.iter().any(|k| k == name) {
                    anyhow::bail!(
                        "layer {} has no variant named {:?} (has: {})",
                        idx,
                        name,
                        if known.is_empty() {
                            "none".to_string()
                        } else {
                            known.join(", ")
                        }
                    );
                }
            }
            lock.set_variant_override(variant.map(str::to_string));
            let resolution = lock.resolution();
            Ok(json!({
                "ok": true,
                "override": variant,
                "active_variant": resolution.variant_name(),
                "resolution": resolution,
            }))
        }
        None => anyhow::bail!("Daemon tap engine not running; cannot pin a variant"),
    }
}

fn get_knob_mode(ctx: &ApiContext, buttons: Option<usize>) -> Result<Value> {
    // Falls back to the installed layout rather than a constant:
    // the button count decides which slots are read, so guessing it
    // would misreport the mode exactly as it would misflash it.
    let buttons = match buttons {
        Some(n) => n,
        None => installed_button_count()?,
    };
    // Place the knob's slots BEFORE touching the device: an absurd
    // count refuses with the numbers quoted back instead of reading
    // a table it cannot interpret.
    let knob_key_ids = crate::protocol::KnobEvent::ALL
        .iter()
        .map(|e| crate::protocol::key_id_for_knob(buttons, 0, *e))
        .collect::<Result<Vec<u8>, _>>()
        .context("knob slots out of range")?;
    // The burst read, not the per-slot walk: the walk stopped at the
    // declared layout's width, so it could not see a key past it
    // (three queries here instead of eighteen, and complete).
    let table = device::with_device(|dev| Ok(device::read_full_table(dev)))
        .context("Cannot read the slot table from the Anticater USB device")?;
    let recorded = crate::host::HostConfig::try_load_json(
        &std::fs::read_to_string(&ctx.config_path).unwrap_or_default(),
    )
    .map(|c| c.bound_device_layers)
    .unwrap_or_default();
    Ok(device_cmds::knob_mode_report(
        &table,
        buttons,
        &knob_key_ids,
        &recorded,
    ))
}

fn upload_keymap(ctx: &ApiContext, yaml: &str, layer: Option<u8>) -> Result<Value> {
    if yaml.len() > crate::policy::KEYMAP_YAML_MAX_BYTES {
        anyhow::bail!("Keymap YAML exceeds 64KB size limit");
    }
    let cfg: DeviceConfig =
        crate::yaml::from_str(yaml).context("Invalid keymap YAML configuration")?;
    cfg.validate()?;

    let selected: Vec<usize> = match layer {
        Some(l) => {
            let idx = l as usize;
            if idx >= cfg.layers.len() {
                anyhow::bail!("Layer {idx} out of range");
            }
            vec![idx]
        }
        None => (0..cfg.layers.len()).collect(),
    };

    let mut slot_packets: Vec<Vec<u8>> = Vec::new();
    let mut led_packets: Vec<Vec<u8>> = Vec::new();
    for layer_idx in selected {
        let lcfg = &cfg.layers[layer_idx];
        let layer_u8 = u8::try_from(layer_idx).context("layer index out of range")?;
        let mut button_count = 0;
        for row in &lcfg.buttons {
            for key_str in row {
                let action = Action::parse(key_str)?;
                let key_id = protocol::key_id_for_button(button_count)?;
                slot_packets.push(action.to_packet(key_id, layer_u8));
                button_count += 1;
            }
        }
        for (knob_idx, knob) in lcfg.knobs.iter().enumerate() {
            for (event, binding) in knob.bindings() {
                let key_id = protocol::key_id_for_knob(button_count, knob_idx, event)?;
                slot_packets.push(binding.to_packet(key_id, layer_u8)?);
            }
        }
        if let Some(ref led_mode) = lcfg.led {
            led_packets.push(protocol::build_led_packet(layer_u8, led_mode)?);
        }
    }

    device_cmds::flash_keymap_hardware(slot_packets, led_packets)
        .context("Cannot flash the keymap to the Anticater USB device")?;

    if let Some(engine) = &ctx.tap_engine {
        let mut lock = engine.lock().unwrap();
        let mut new_cfg = lock.config().clone();
        new_cfg.bound_device_layers.clear();
        lock.apply_config(new_cfg);
        let _ = crate::host::device_binding::record_bound_layers(&ctx.config_path, &[]);
    }

    Ok(json!({
        "ok": true,
        "message": "Keymap flashed to hardware successfully"
    }))
}
