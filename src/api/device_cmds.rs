//! The commands that talk to the device directly.
//!
//! Split from `dispatch.rs` for the file-length gate, along a real seam:
//! these reach the hardware, while what remains in `dispatch` routes and
//! validates. Two of them exist so an agent has the same reach as the CLI,
//! which is the drift that motivated `tests/surface_parity.rs`: `bind-seq`
//! was a CLI command for a day with no MCP tool at all, so an agent could
//! not bind a sequence, and `show-keys` was CLI-only, leaving one to guess
//! names against a parser that refuses unknown ones.

use super::*;

use crate::firmware::BIND_LAYERS;
use crate::host::bind::flash_slot_bindings;

/// Which device layers a `bind_slots` request targets.
///
/// The wire carries two spellings: `layers` (plural list) and `layer`
/// (singular, from the registry/CLI/MCP schema and the UI). The plural wins
/// when both are present; neither means all layers. One function so the
/// socket and MCP paths cannot disagree about it again.
pub(crate) fn resolve_bind_layers(layers: Option<Vec<u8>>, layer: Option<u8>) -> Vec<u8> {
    if let Some(ls) = layers {
        return ls;
    }
    if let Some(l) = layer {
        return vec![l];
    }
    BIND_LAYERS.to_vec()
}

/// Flash the one-time host-translate slot bindings, record them, and sync
/// the backlight -- the daemon half of what the CLI's `bind-slots` does.
pub fn bind_slots(
    ctx: &mut ApiContext,
    layers: Option<Vec<u8>>,
    layer: Option<u8>,
    buttons: Option<usize>,
) -> Result<serde_json::Value> {
    let target_layers = resolve_bind_layers(layers, layer);
    let flash_layers = target_layers.clone();
    // Knob slots follow the buttons; see `protocol::key_id_for_knob`.
    // An explicit count is distrusted like any remote input: past the
    // slot space it would wrap onto someone else's slot.
    let buttons = match buttons {
        Some(n) => {
            if n > crate::firmware::SLOT_MAX_KEY_ID as usize {
                anyhow::bail!(
                    "button count {} is past the addressable slots (max {})",
                    n,
                    crate::firmware::SLOT_MAX_KEY_ID
                );
            }
            n
        }
        None => installed_button_count().context(
            "bind_slots needs the device's button count and no layout was readable; pass `buttons` explicitly",
        )?,
    };
    let key_ids: Vec<u8> = crate::protocol::KnobEvent::ALL
        .iter()
        .map(|e| crate::protocol::key_id_for_knob(buttons, 0, *e))
        .collect::<Result<Vec<_>, _>>()
        .context("knob slots out of range for the button count")?;
    let to_flash = flash_layers.clone();
    let count = device::with_device(move |dev| flash_slot_bindings(dev, buttons, &to_flash))
        .context("Cannot flash slot bindings to the Anticater USB device")?;

    if let Some(engine) = &ctx.tap_engine {
        let mut lock = engine.lock().unwrap();
        let mut new_cfg = lock.config().clone();
        new_cfg.bound_device_layers = flash_layers.clone();
        lock.apply_config(new_cfg);
        let _ = crate::host::device_binding::record_bound_layers(&ctx.config_path, &flash_layers);
        let active = lock.layer_idx();
        crate::host::led_sync::sync_led(lock.config(), active);
    }

    Ok(json!({
        "ok": true,
        "flashed_slots": count,
        "layers": target_layers,
        "buttons": buttons,
        "key_ids": key_ids
    }))
}

/// Just the names from a vocabulary table, for the wire.
fn names<T: Copy>(table: &[(&'static str, T)]) -> Vec<&'static str> {
    table.iter().map(|(n, _)| *n).collect()
}

/// Everything the device understands, from the tables the parser reads.
pub fn vocabulary_reply() -> serde_json::Value {
    json!({
        "keys": names(crate::vocabulary::KEY_NAMES),
        "media": names(crate::vocabulary::MEDIA_NAMES),
        "modifiers": names(crate::vocabulary::MODIFIER_NAMES),
        "mouse": crate::vocabulary::MOUSE_NAMES,
        "note": "Chords join a modifier and a key with '-' or '+', e.g. cmd-c. \
                 Knob gestures are keys 2=CCW, 3=press, 4=CW, 5=hold+twist L, \
                 6=hold+twist R.",
    })
}

/// Bind one gesture to a sequence, and read it back before saying so.
pub fn bind_sequence(
    key: u8,
    layer: u8,
    actions: &[String],
    delay_ms: u16,
) -> Result<serde_json::Value> {
    if layer >= DEVICE_LAYERS {
        anyhow::bail!(
            "layer {} does not exist; this firmware has layers 0..{}",
            layer,
            DEVICE_LAYERS - 1
        );
    }
    let mut steps = crate::fd::parse_sequence(actions)?;
    for step in steps.iter_mut().skip(1) {
        step.delay_ms = delay_ms;
    }
    let packet = crate::fd::build_packet(key, layer, &steps)?;
    let wipe = crate::fd::wipe_packet(key, layer);
    let sent = packet.clone();
    // Read the whole table back: a write that landed nowhere must
    // report as unconfirmed, not as success.
    let table = device::with_device(move |dev| {
        device::send_report(dev, &wipe)?;
        std::thread::sleep(Duration::from_millis(crate::firmware::SLOT_WRITE_GAP_MS));
        device::send_commit(dev)?;
        device::send_report(dev, &packet)?;
        std::thread::sleep(Duration::from_millis(crate::firmware::SLOT_WRITE_GAP_MS));
        device::send_commit(dev)?;
        Ok(device::read_full_table(dev))
    })?;
    let confirmed = table.iter().any(|r| crate::fd::record_matches(&sent, r));
    if !confirmed {
        anyhow::bail!(
            "slot {} layer {} did not read back as written; the binding is UNCONFIRMED",
            key,
            layer
        );
    }
    Ok(json!({
        "key": key,
        "layer": layer,
        "actions": actions,
        "entries": sent[6],
        "delay_ms": delay_ms,
        "confirmed": true,
    }))
}

/// Dump the slot table, whole or slot by slot.
pub fn read_slots(
    group: Option<u8>,
    counters: Option<Vec<u8>>,
    full: bool,
) -> Result<serde_json::Value> {
    // The burst read: every key on every layer in three queries.
    // The per-slot walk below stops at whatever width it is given,
    // which is why bindings above key 6 went unseen for months.
    if full {
        let table = device::with_device(|dev| Ok(device::read_full_table(dev)))?;
        let slots: Vec<serde_json::Value> = table
            .iter()
            .map(|r| {
                json!({
                    "hex": r.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
                    "key": r.get(2),
                    "layer": r.get(3),
                    "is_firmware_default": device::verify::is_synthetic_default(r),
                })
            })
            .collect();
        return Ok(json!({ "slots": slots, "count": slots.len() }));
    }
    let ctrs = counters.unwrap_or_else(|| vec![1, 2, 3]);
    if ctrs.len() > crate::policy::READ_SLOTS_COUNTERS_MAX {
        anyhow::bail!(
            "Slot counters query exceeds limit of {} entries",
            crate::policy::READ_SLOTS_COUNTERS_MAX
        );
    }
    let grp = group.unwrap_or(crate::firmware::SLOT_DEFAULT_GROUP);
    let results = device::with_device(move |dev| {
        let mut results = Vec::new();
        for c in ctrs {
            match device::read_slot(dev, grp, c) {
                Ok(bytes) => {
                    let hex: Vec<String> = bytes.iter().map(|b| format!("{:02x}", b)).collect();
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
            sleep(Duration::from_millis(crate::firmware::SLOT_READ_GAP_MS));
        }
        Ok(results)
    })
    .context("Cannot read slots from the Anticater USB device")?;

    Ok(json!({
        "group": grp,
        "slots": results
    }))
}

/// Flash a standalone keymap's slot packets (committed via `send_commit`),
/// followed by its LED mode packets (sent via `send_led` with the LED init packet).
pub fn flash_keymap_hardware(slot_packets: Vec<Vec<u8>>, led_packets: Vec<Vec<u8>>) -> Result<()> {
    // Pace against other LED jobs in this process: a keymap flash lands
    // its LED writes in one burst, the same shape that wedges the render.
    if !led_packets.is_empty() {
        crate::host::led_sync::pace_led();
    }
    device::with_device(move |dev| {
        for packet in &slot_packets {
            device::send_report(dev, packet)?;
            std::thread::sleep(Duration::from_millis(crate::firmware::UPLOAD_PACKET_GAP_MS));
        }
        if !slot_packets.is_empty() {
            device::send_commit(dev)?;
        }
        for (i, packet) in led_packets.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(Duration::from_millis(
                    crate::firmware::LED_INTER_LAYER_SETTLE_MS,
                ));
            }
            device::send_led(dev, packet)?;
        }
        Ok(())
    })
}

/// Set one layer's backlight mode, paced and read-before-write like
/// every other LED path, and report the read-back -- never "OK".
pub fn set_led(layer: u8, mode: String, color: Option<String>) -> Result<serde_json::Value> {
    if mode.len() > crate::policy::SET_LED_SPEC_MAX_LEN {
        anyhow::bail!(
            "LED mode string exceeds {} characters",
            crate::policy::SET_LED_SPEC_MAX_LEN
        );
    }
    if layer > crate::policy::SET_LED_LAYER_MAX {
        anyhow::bail!(
            "Layer index {} exceeds maximum of {}",
            layer,
            crate::policy::SET_LED_LAYER_MAX
        );
    }
    let spec = match color {
        Some(c) if !c.is_empty() => format!("{mode} {c}"),
        _ => mode,
    };
    let packet = protocol::build_led_packet(layer, &spec)?;
    // Written and then READ BACK on the same device handle. This
    // firmware accepts an LED write, stores it, and changes nothing
    // when the init packet is missing, so "the bytes went out" has
    // never been evidence that the light changed. The reply carries
    // what the device holds now, and the settings app renders that
    // instead of the word "OK" it used to invent.
    //
    // Two guards against the renderer's freeze (it wedges on mode
    // changes until a replug; read-back cannot see it): pacing, so
    // a lighting click's three RPCs cannot burst, and
    // read-before-write, so re-setting the stored mode sends
    // nothing. A skipped write still reads back the truth.
    let want = protocol::led_mode_number(&spec);
    crate::host::led_sync::pace_led();
    let applied = device::with_device(move |dev| {
        if let (Some(w), Ok(current)) = (want, device::read_led_mode(dev, layer)) {
            if current == w {
                return Ok(current);
            }
        }
        device::send_led(dev, &packet)?;
        device::read_led_mode(dev, layer)
    })
    .context("Cannot drive the Anticater USB device")?;
    // Layers past the firmware's own are storage without a measured
    // render effect: layer 9 holds whatever was last written (green
    // was stored and read back on hardware) while layers 0-2 sit
    // untouched beside it. Say so, or "stored" will be read as
    // "showing" for layers nobody has watched.
    let mut note = String::from(
        "If the ring doesn't match, unplug/replug the knob -- its renderer can wedge after a mode change.",
    );
    if layer >= crate::firmware::DEVICE_LAYERS {
        note.push_str(&format!(
            " Layer {layer} is past the firmware's {} layers: it stores, render unmeasured.",
            crate::firmware::DEVICE_LAYERS
        ));
    }
    Ok(json!({
        "ok": true,
        "layer": layer,
        "spec": spec,
        "mode": applied,
        "mode_name": led_mode_name(applied),
        // Read-back proves the mode is STORED, never that the ring
        // renders it: a wedged renderer holds its last effect while
        // every query answers correctly. Every surface that shows
        // this reply should say where to look when they disagree.
        "note": note
    }))
}

#[cfg(test)]
mod bind_slots_tests {
    use super::*;
    use crate::api::Command;

    #[test]
    fn singular_layer_is_honored() {
        // The UI and the registry/MCP schema send `layer` (singular); the
        // dispatcher used to read only `layers` (plural), so a single-layer
        // request silently flashed ALL layers.
        let cmd: Command = serde_json::from_value(serde_json::json!({
            "method": "bind_slots",
            "params": { "layer": 1 }
        }))
        .expect("singular layer must deserialize");
        match cmd {
            Command::BindSlots { layers, layer, .. } => {
                assert_eq!(layer, Some(1));
                assert_eq!(resolve_bind_layers(layers, layer), vec![1]);
            }
            other => panic!("wrong command: {other:?}"),
        }
    }

    #[test]
    fn plural_layers_wins_and_empty_means_all() {
        let cmd: Command = serde_json::from_value(serde_json::json!({
            "method": "bind_slots",
            "params": { "layers": [0, 2], "layer": 1 }
        }))
        .expect("plural layers must deserialize");
        match cmd {
            Command::BindSlots { layers, layer, .. } => {
                assert_eq!(resolve_bind_layers(layers, layer), vec![0, 2]);
            }
            other => panic!("wrong command: {other:?}"),
        }

        let cmd: Command = serde_json::from_value(serde_json::json!({
            "method": "bind_slots",
            "params": {}
        }))
        .expect("empty params must deserialize");
        match cmd {
            Command::BindSlots { layers, layer, .. } => {
                assert_eq!(
                    resolve_bind_layers(layers, layer),
                    crate::firmware::BIND_LAYERS.to_vec()
                );
            }
            other => panic!("wrong command: {other:?}"),
        }
    }
}
