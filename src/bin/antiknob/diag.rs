//! Reverse-engineering and diagnostic commands.
//!
//! Split out of `cmds` for the file-length gate, along a real seam: these
//! three read or poke the device without changing a binding. `listen` snoops
//! input reports, `read-slots` dumps the slot table, `raw` sends a payload
//! for protocol work. Nothing here interprets a config.

use antiknob::device;

use anyhow::{Context, Result};
use std::thread::sleep;
use std::time::Duration;

/// Parse hex byte strings ("FC", "0xfc") into a payload. Pure for testing.
fn parse_hex_bytes(parts: &[String]) -> Result<Vec<u8>> {
    parts
        .iter()
        .map(|s| {
            let s = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            u8::from_str_radix(s, 16).map_err(|_| anyhow::anyhow!("Bad hex byte '{}'", s))
        })
        .collect()
}

pub fn run_listen(timeout_secs: u64, devices: Vec<String>) -> Result<()> {
    use std::time::Instant;
    let filters = devices
        .iter()
        .map(|d| device::snoop::parse_device_filter(d).map_err(|e| anyhow::anyhow!(e)))
        .collect::<Result<Vec<_>>>()?;
    if filters.is_empty() {
        println!("[ ==> ] Opening every supported device for snooping (non-exclusive, no sudo)...");
    } else {
        println!(
            "[ ==> ] Opening {} filtered device(s) for snooping...",
            filters.len()
        );
    }
    // The whole snoop loop runs on the HID thread: `SnoopIface` holds live
    // `HidDevice` handles, which must never cross a thread boundary.
    device::with_hid(move |api| {
        let ifaces = device::open_all_interfaces_on(api, &filters)?;
        if ifaces.is_empty() {
            if filters.is_empty() {
                println!("[ Wrn ] No supported devices detected on USB.");
            } else {
                println!("[ Wrn ] No supported device matched the given VID:PID filter(s).");
            }
            return Ok(());
        }
        for iface in &ifaces {
            println!("        watching {} ({})", iface.label, iface.detail);
        }
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        println!(
            "[ ==> ] Snooping for {}s: twist / press / hold the knob (mouse stays usable)...",
            timeout_secs
        );
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            for iface in &ifaces {
                match iface.device.read_timeout(&mut buf, 20) {
                    Ok(0) => {}
                    Ok(n) => {
                        if buf[..n].iter().any(|&b| b != 0) {
                            let hex: Vec<String> =
                                buf[..n].iter().map(|b| format!("{:02x}", b)).collect();
                            let note = device::snoop::decode_input(&iface.usages, &buf[..n]);
                            println!(
                                "        [{}] +{}B: {}   {}",
                                iface.label,
                                n,
                                hex.join(" "),
                                note
                            );
                        }
                    }
                    Err(e) => {
                        println!("[ Wrn ] Read error on {}: {}", iface.label, e);
                    }
                }
            }
        }
        Ok(())
    })?;
    println!("[ Ok  ] Listen window closed.");
    Ok(())
}
pub fn run_read_slots(slots_per_layer: u8, wide: bool) -> Result<()> {
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    // `wide` keeps the old exploratory sweep for protocol work; the default
    // asks the device for its actual table using the addressing scheme in
    // `device::slot_table_addresses`.
    let groups: Vec<u8> = if wide {
        (0x00u8..=0x40u8).collect()
    } else {
        vec![slots_per_layer]
    };
    let counters: u8 = if wide {
        8
    } else {
        slots_per_layer.saturating_mul(device::DEVICE_LAYERS)
    };
    println!(
        "[ ==> ] Reading slot table ({} group(s), counters 1-{})...",
        groups.len(),
        counters
    );
    device::with_device(move |dev| {
        for group in groups {
            for counter in 1u8..=counters {
                match device::read_slot(dev, group, counter) {
                    Ok(bytes) => {
                        let hex: Vec<String> = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                        println!(
                            "        group=0x{:02x} counter={} ({}B): {}",
                            group,
                            counter,
                            bytes.len(),
                            hex.join(" ")
                        );
                    }
                    Err(e) => {
                        println!(
                            "        group=0x{:02x} counter={}: READ FAILED: {}",
                            group, counter, e
                        );
                    }
                }
                sleep(Duration::from_millis(50));
            }
        }
        Ok(())
    })?;
    println!("[ Ok  ] Slot dump complete (device state unchanged).");
    Ok(())
}
pub fn run_raw(bytes: Vec<String>) -> Result<()> {
    let payload = parse_hex_bytes(&bytes)?;
    let sent = payload.len();
    println!("[ ==> ] Opening Anticater device via native IOHIDManager (no sudo)...");
    device::with_device(move |dev| device::send_report(dev, &payload))?;
    println!("[ Ok  ] Raw {}-byte payload sent.", sent);
    Ok(())
}

/// Walk every LED mode on one layer, holding each long enough to see it.
///
/// The request was a breathing colour per layer, and nothing in the mapped
/// mode table is known to breathe -- `led_mode_name` even reports a mode 5
/// ("custom") that no name here produces. Rather than pick a mode and hope,
/// this shows each one in turn and says what the device reads back, so the
/// answer comes from looking at the knob.
///
/// The layer's original mode is restored at the end, including when a mode
/// fails to apply: a diagnostic that leaves the hardware changed is a
/// diagnostic nobody runs twice.
pub fn run_led_probe(layer: u8, dwell_secs: u64, color: &str) -> Result<()> {
    use antiknob::protocol::LED_MODE_NAMES;

    let before = device::with_device(move |dev| device::read_led_mode(dev, layer))
        .context("cannot read the layer's current LED mode; refusing to change it")?;
    println!(
        "[ ==> ] Layer {} is on mode {} now; it will be restored at the end.",
        layer, before
    );
    println!("        Watch the knob. Report which mode, if any, BREATHES.");

    for (idx, name) in LED_MODE_NAMES.iter().enumerate() {
        let spec = if *name == "off" {
            (*name).to_string()
        } else {
            format!("{name} {color}")
        };
        let packet = match antiknob::protocol::build_led_packet(layer, &spec) {
            Ok(p) => p,
            Err(e) => {
                println!("        mode {idx} ({name}): cannot build packet: {e}");
                continue;
            }
        };
        let readback = device::with_device(move |dev| {
            device::send_report(dev, &packet)?;
            device::send_commit(dev)?;
            sleep(Duration::from_millis(150));
            device::read_led_mode(dev, layer)
        });
        match readback {
            Ok(got) => println!("        mode {idx} ({name}) -> device reads back {got}"),
            Err(e) => println!("        mode {idx} ({name}) -> FAILED: {e}"),
        }
        sleep(Duration::from_secs(dwell_secs));
    }

    let restore = format!("mode{before}");
    let restored = antiknob::protocol::build_led_packet(layer, &restore).and_then(|p| {
        device::with_device(move |dev| {
            device::send_report(dev, &p)?;
            device::send_commit(dev)
        })
    });
    match restored {
        Ok(()) => println!("[ Ok  ] Restored layer {layer} to mode {before}."),
        Err(e) => println!(
            "[ Wrn ] Could not restore layer {layer} to mode {before}: {e}\n\
                     Set it by hand with: antiknob led {layer} mode{before}"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_bytes_parse_with_and_without_prefix() {
        assert_eq!(
            parse_hex_bytes(&["FC".to_string(), "0xfc".to_string(), "00".to_string()]).unwrap(),
            vec![0xFC, 0xFC, 0x00]
        );
        assert!(parse_hex_bytes(&["zz".to_string()]).is_err());
        assert!(parse_hex_bytes(&["123".to_string()]).is_err());
    }
}
