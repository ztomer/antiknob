//! Making the knob's light follow the ACTIVE HOST LAYER.
//!
//! The firmware stores one backlight mode per DEVICE layer and has never
//! heard of host layers. So a per-layer colour is not something the hardware
//! does on its own -- it is something the daemon has to write on every
//! switch, to whichever device layer carries the slot chords.
//!
//! Until this existed, `EngineEvent::LayerChanged` was printed and nothing
//! else, and the settings app's lighting controls addressed the firmware's
//! three fixed layers. Those are a different axis from the host layers: only
//! ONE device layer is host-translated, so while the daemon is driving, the
//! knob sits on that one and shows its one colour whatever host layer is
//! active. Offering a colour per host layer without this would have been a
//! control that changes a value nothing reads.
//!
//! The decision is pure and the effect is one function, so what a switch
//! IMPLIES can be tested without a knob attached.

use super::HostConfig;

/// The LED write a switch to `layer_idx` implies: `(device layer, mode)`.
///
/// `None` in three real cases, none of which is an error:
///   * nothing is bound, so no device layer is host-translated and there is
///     no light the daemon is entitled to drive;
///   * the layer names no mode, which means "leave it alone";
///   * the index is out of range, which a caller can produce during a
///     config reload and must not be turned into a write to layer 0.
pub fn led_writes_for(cfg: &HostConfig, layer_idx: usize) -> Vec<(u8, String)> {
    let Some(mode) = cfg.layers.get(layer_idx).and_then(|l| l.led.as_deref()) else {
        return Vec::new();
    };
    let mode = mode.trim();
    if mode.is_empty() {
        return Vec::new();
    }
    // EVERY bound layer, not one. `bind-slots` with no `--layer` binds all
    // three, and nothing on this firmware reports which one the knob is
    // currently on -- a calibrated 512-query sweep found no such query. So
    // the only way to be sure the active layer shows the right colour is to
    // set them all. With one layer bound this is the single write it always
    // was.
    cfg.bound_device_layers
        .iter()
        .map(|l| (*l, mode.to_string()))
        .collect()
}

/// Apply that write, OFF the calling thread.
///
/// Detached deliberately. Both callers are latency-critical: one is the
/// CGEventTap callback, and macOS disables a tap whose callback runs long.
/// An LED write is two HID reports with a 20 ms settle between them, plus
/// however long the HID thread's queue already is -- easily 100 ms, on the
/// thread that is supposed to be forwarding the user's keystrokes.
///
/// Failures are dropped on purpose: the knob may be unplugged, and a switch
/// whose light did not follow is still a switch that happened. The layer
/// change must not fail because the light did not.
pub fn sync_led(cfg: &HostConfig, layer_idx: usize) {
    let writes = led_writes_for(cfg, layer_idx);
    if writes.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for (device_layer, mode) in writes {
            let Ok(packet) = crate::protocol::build_led_packet(device_layer, &mode) else {
                continue;
            };
            let _ = crate::device::with_device(move |dev| crate::device::send_led(dev, &packet));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostConfig, HostLayer};

    fn layer(name: &str, led: Option<&str>) -> HostLayer {
        HostLayer {
            name: name.to_string(),
            led: led.map(str::to_string),
            ..HostLayer::empty(name)
        }
    }

    fn cfg(bound: Vec<u8>, layers: Vec<HostLayer>) -> HostConfig {
        HostConfig {
            layers,
            bound_device_layers: bound,
            ..HostConfig::default_config()
        }
    }

    #[test]
    fn a_layer_with_a_mode_writes_it_to_the_bound_device_layer() {
        let c = cfg(
            vec![1],
            vec![layer("Media", Some("red")), layer("Nav", Some("green"))],
        );
        assert_eq!(led_writes_for(&c, 0), vec![(1, "red".to_string())]);
        assert_eq!(led_writes_for(&c, 1), vec![(1, "green".to_string())]);
    }

    /// THE CASE THAT SHIPPED BROKEN. `bind-slots` with no `--layer` binds
    /// every device layer, and nothing on this firmware reports which one
    /// the knob is currently on -- so the only way the active layer shows
    /// the right colour is to write all of them.
    ///
    /// Before the recorded value became a SET this could not even be
    /// expressed: binding all three recorded `None`, the same value as
    /// binding nothing, so the backlight silently never fired after the most
    /// common flash there is.
    #[test]
    fn every_bound_layer_is_written_when_all_of_them_are_bound() {
        let c = cfg(vec![0, 1, 2], vec![layer("Media", Some("green"))]);
        assert_eq!(
            led_writes_for(&c, 0),
            vec![
                (0, "green".to_string()),
                (1, "green".to_string()),
                (2, "green".to_string())
            ],
            "a fully bound knob must have every layer set, or the colour \
             depends on which layer it happens to be on"
        );
    }

    /// Nothing bound means no device layer is host-translated, so there is
    /// no light this daemon is entitled to drive.
    #[test]
    fn nothing_bound_writes_nothing() {
        let c = cfg(Vec::new(), vec![layer("Media", Some("red"))]);
        assert!(led_writes_for(&c, 0).is_empty());
    }

    /// A layer that names no mode leaves the light as it is. This is the
    /// state every layer written before the field existed loads in.
    #[test]
    fn a_layer_without_a_mode_leaves_the_light_alone() {
        let c = cfg(
            vec![0],
            vec![layer("Media", None), layer("Blank", Some("   "))],
        );
        assert!(led_writes_for(&c, 0).is_empty());
        assert!(led_writes_for(&c, 1).is_empty(), "whitespace is not a mode");
    }

    /// An index past the end is producible during a config reload. Falling
    /// back to layer 0's mode would light the knob for a layer that is not
    /// active.
    #[test]
    fn an_out_of_range_layer_writes_nothing() {
        let c = cfg(vec![0], vec![layer("Media", Some("red"))]);
        assert!(led_writes_for(&c, 7).is_empty());
    }

    /// Whatever the mode string is, it must be one `build_led_packet`
    /// accepts -- otherwise `sync_led` silently drops every switch and the
    /// colours never change, with nothing anywhere reporting why.
    #[test]
    fn every_mode_a_layer_can_carry_builds_a_packet() {
        for name in crate::led::LED_MODE_NAMES {
            let c = cfg(vec![0], vec![layer("L", Some(name))]);
            let writes = led_writes_for(&c, 0);
            assert_eq!(writes.len(), 1, "{name}");
            assert!(
                crate::protocol::build_led_packet(writes[0].0, &writes[0].1).is_ok(),
                "{name} is offered but cannot be sent"
            );
        }
    }
}
