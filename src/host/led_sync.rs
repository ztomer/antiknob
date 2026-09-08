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
pub fn led_write_for(cfg: &HostConfig, layer_idx: usize) -> Option<(u8, String)> {
    let device_layer = cfg.bound_device_layer?;
    let mode = cfg.layers.get(layer_idx)?.led.as_deref()?.trim();
    if mode.is_empty() {
        return None;
    }
    Some((device_layer, mode.to_string()))
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
    let Some((device_layer, mode)) = led_write_for(cfg, layer_idx) else {
        return;
    };
    std::thread::spawn(move || {
        let Ok(packet) = crate::protocol::build_led_packet(device_layer, &mode) else {
            return;
        };
        let _ = crate::device::with_device(move |dev| crate::device::send_led(dev, &packet));
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

    fn cfg(bound: Option<u8>, layers: Vec<HostLayer>) -> HostConfig {
        HostConfig {
            layers,
            bound_device_layer: bound,
            ..HostConfig::default_config()
        }
    }

    #[test]
    fn a_layer_with_a_mode_writes_it_to_the_bound_device_layer() {
        let c = cfg(
            Some(1),
            vec![layer("Media", Some("red")), layer("Nav", Some("green"))],
        );
        assert_eq!(led_write_for(&c, 0), Some((1, "red".to_string())));
        assert_eq!(led_write_for(&c, 1), Some((1, "green".to_string())));
    }

    /// Nothing bound means no device layer is host-translated, so there is
    /// no light this daemon is entitled to drive. Writing to layer 0 anyway
    /// would recolour a layer the user never asked about.
    #[test]
    fn nothing_bound_writes_nothing() {
        let c = cfg(None, vec![layer("Media", Some("red"))]);
        assert_eq!(led_write_for(&c, 0), None);
    }

    /// A layer that names no mode leaves the light as it is. This is the
    /// state every layer written before the field existed loads in, and
    /// those configs must not start changing the backlight on switch.
    #[test]
    fn a_layer_without_a_mode_leaves_the_light_alone() {
        let c = cfg(
            Some(0),
            vec![layer("Media", None), layer("Blank", Some("   "))],
        );
        assert_eq!(led_write_for(&c, 0), None);
        assert_eq!(led_write_for(&c, 1), None, "whitespace is not a mode");
    }

    /// An index past the end is producible during a config reload. Falling
    /// back to layer 0's mode would light the knob for a layer that is not
    /// active.
    #[test]
    fn an_out_of_range_layer_writes_nothing() {
        let c = cfg(Some(0), vec![layer("Media", Some("red"))]);
        assert_eq!(led_write_for(&c, 7), None);
    }

    /// Whatever the mode string is, it must be one `build_led_packet`
    /// accepts -- otherwise `sync_led` silently drops every switch and the
    /// colours never change, with nothing anywhere reporting why.
    #[test]
    fn every_mode_a_layer_can_carry_builds_a_packet() {
        for name in crate::led::LED_MODE_NAMES {
            let c = cfg(Some(0), vec![layer("L", Some(name))]);
            let (device_layer, mode) = led_write_for(&c, 0).expect(name);
            assert!(
                crate::protocol::build_led_packet(device_layer, &mode).is_ok(),
                "{name} is offered but cannot be sent"
            );
        }
    }
}
