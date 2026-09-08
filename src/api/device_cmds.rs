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
    if layer >= device::DEVICE_LAYERS {
        anyhow::bail!(
            "layer {} does not exist; this firmware has layers 0..{}",
            layer,
            device::DEVICE_LAYERS - 1
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
        std::thread::sleep(Duration::from_millis(15));
        device::send_commit(dev)?;
        device::send_report(dev, &packet)?;
        std::thread::sleep(Duration::from_millis(15));
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
    if ctrs.len() > 16 {
        anyhow::bail!("Slot counters query exceeds limit of 16 entries");
    }
    let grp = group.unwrap_or(0x0F);
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
