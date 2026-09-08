//! Central Command Dispatcher.
//!
//! Executes `Command` requests against the device, host config, and tap engine.

use super::types::{led_mode_name, Command, PowerStatus};
use crate::apps;
use crate::config::DeviceConfig;
use crate::device;
use crate::host::bind::{flash_slot_bindings, BIND_LAYERS};
use crate::host::HostConfig;
use crate::protocol::{self, Action};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

/// Execute an API command and return a JSON result.
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
use device_cmds::{bind_sequence, read_slots, vocabulary_reply};

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

            // WHICH interface the commands go to, named alongside the full
            // list. Every caller that wanted "the device" was taking
            // `devices[0]`, which is whichever HID interface enumerated
            // first -- a keyboard endpoint on this hardware, not the vendor
            // endpoint the tool actually drives.
            let primary_device = device::primary_device(&devices);

            Ok(json!({
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
            }))
        }

        Command::Ping {} => Ok(json!({})),

        Command::ListCommands {} => Ok(json!({ "commands": describe_commands() })),

        Command::GetConfig {} => {
            let cfg = match &ctx.tap_engine {
                Some(engine) => engine.lock().unwrap().config().clone(),
                None => read_config_or_default(&ctx.config_path),
            };
            Ok(serde_json::to_value(&cfg)?)
        }

        Command::SetConfig { config } => {
            // Keeps the version it replaces, and writes through a rename.
            // `fs::write` truncated first and kept nothing, so a bad writer
            // -- a UI bug, a half-finished edit, a crash mid-save -- took
            // the config with it and left no way back. One did.
            crate::host::config_backup::write_with_backup(
                &ctx.config_path,
                &config.to_json_pretty(),
            )?;

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
        },

        Command::GetVirtualLayer {} => match &ctx.tap_engine {
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
                    crate::device::DEVICE_LAYERS,
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
        },

        Command::SetVirtualVariant { variant } => match &ctx.tap_engine {
            Some(engine) => {
                let mut lock = engine.lock().unwrap();
                let idx = lock.layer_idx();
                // A name that matches nothing is refused, not stored. The
                // resolver would fall back to the layer's own bindings and
                // the caller would be told "ok" for a pin that never took.
                if let Some(name) = &variant {
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
                lock.set_variant_override(variant.clone());
                let resolution = lock.resolution();
                Ok(json!({
                    "ok": true,
                    "override": variant,
                    "active_variant": resolution.variant_name(),
                    "resolution": resolution,
                }))
            }
            None => anyhow::bail!("Daemon tap engine not running; cannot pin a variant"),
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
            // Written and then READ BACK on the same device handle. This
            // firmware accepts an LED write, stores it, and changes nothing
            // when the init packet is missing, so "the bytes went out" has
            // never been evidence that the light changed. The reply carries
            // what the device holds now, and the settings app renders that
            // instead of the word "OK" it used to invent.
            let applied = device::with_device(move |dev| {
                device::send_led(dev, &packet)?;
                device::read_led_mode(dev, layer)
            })
            .context("Cannot drive the Anticater USB device")?;
            Ok(json!({
                "ok": true,
                "layer": layer,
                "spec": spec,
                "mode": applied,
                "mode_name": led_mode_name(applied)
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

        Command::GetKnobMode { buttons } => {
            // Falls back to the installed layout rather than a constant:
            // the button count decides which slots are read, so guessing it
            // would misreport the mode exactly as it would misflash it.
            let buttons = match buttons {
                Some(n) => n,
                None => installed_button_count()?,
            };
            // Five slots per knob, not three. Reading `buttons + 3` stopped
            // the walk two slots short of the hold+twist pair, so the two
            // gestures the classifier now examines were never in the table
            // it examined them in.
            let slots =
                u8::try_from(buttons + crate::protocol::GESTURES_PER_KNOB).unwrap_or(u8::MAX);
            let table = device::with_device(move |dev| {
                Ok(device::read_slot_table(dev, slots, device::DEVICE_LAYERS))
            })
            .context("Cannot read the slot table from the Anticater USB device")?;
            let mode = device::mode::classify(&table, buttons);
            // WHICH device layer carries the host chords, alongside whether
            // any does. The mode above is read from the hardware; this is
            // read from what `bind-slots` recorded, and the two answer
            // different questions -- "can a host layer fire at all" versus
            // "which of your three layers is the daemon involved in". The
            // GUI presented three host layers as though all three were live.
            let bound = crate::host::HostConfig::try_load_json(
                &std::fs::read_to_string(&ctx.config_path).unwrap_or_default(),
            )
            .map(|c| c.bound_device_layers)
            .unwrap_or_default();
            let arrangement =
                crate::host::device_binding::arrangement(&bound, device::DEVICE_LAYERS);
            Ok(json!({
                "mode": mode.as_str(),
                "buttons": buttons,
                // The slots this layout puts the knob's gestures on. The
                // declared button count places them, and a layout that
                // declares the wrong number moves every gesture along
                // without any surface saying so -- so the surfaces say so.
                "knob_key_ids": crate::protocol::KnobEvent::ALL
                    .iter()
                    .map(|e| crate::protocol::key_id_for_knob(buttons, 0, *e))
                    .collect::<Vec<u8>>(),
                "slots_read": table.len(),
                "host_layers_can_fire": mode == device::mode::KnobMode::HostTranslate,
                "device_binding": arrangement,
                "device_binding_summary": arrangement.describe()
            }))
        }

        Command::BindSlots { layers, buttons } => {
            let target_layers = layers.unwrap_or_else(|| BIND_LAYERS.to_vec());
            let flash_layers = target_layers.clone();
            // Knob slots follow the buttons; see `protocol::key_id_for_knob`.
            // The fallback is the installed layout, never a constant: a
            // wrong count writes bindings the firmware never reads and
            // nothing reports a failure.
            let buttons = match buttons {
                Some(n) => n,
                None => installed_button_count().context(
                    "bind_slots needs the device's button count and no layout was \
                     readable; pass `buttons` explicitly",
                )?,
            };
            // The key IDs this flash targets, worked out before the write so
            // they can be reported whether or not it succeeds. A layout that
            // declares the wrong number of buttons writes a well-formed run
            // of packets into slots the knob never reads, and the device
            // reports no error -- so the only way anyone finds out is if the
            // reply says WHICH keys were written. It used to say only how
            // many, which is exactly the count a misflash also produces.
            let key_ids: Vec<u8> = crate::protocol::KnobEvent::ALL
                .iter()
                .map(|e| crate::protocol::key_id_for_knob(buttons, 0, *e))
                .collect();
            let count =
                device::with_device(move |dev| flash_slot_bindings(dev, buttons, &flash_layers))
                    .context("Cannot flash slot bindings to the Anticater USB device")?;
            Ok(json!({
                "ok": true,
                "flashed_slots": count,
                "layers": target_layers,
                "buttons": buttons,
                "key_ids": key_ids
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
                // Knob slots continue after the buttons; `button_count` is
                // what places them (see `protocol::key_id_for_knob`).
                for (knob_idx, knob) in lcfg.knobs.iter().enumerate() {
                    for (event, binding) in knob.bindings() {
                        let key_id = protocol::key_id_for_knob(button_count, knob_idx, event);
                        packets.push(binding.to_packet(key_id, layer_u8)?);
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
