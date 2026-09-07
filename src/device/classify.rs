// classify.rs — is this HID device ours, and over which transport?
//
// Split from mod.rs so the decision is a pure function with its own tests:
// the Bluetooth branches cannot be reached by any hardware on hand, and this
// is the only way to pin them at all.

use super::{TransportType, SUPPORTED_DEVICES};

/// Does this product string look like an Anticater knob?
fn looks_like_anticater(product: &str) -> bool {
    let p = product.to_lowercase();
    p.contains("anticater") || p.contains("vk01") || p.contains("vk-01")
}

/// Decide whether a HID device is ours, and over which transport.
///
/// Pure so it can be tested: the Bluetooth branches cannot be reached by any
/// hardware on hand, and this is the only way to pin them at all. Extracting
/// it found a real defect -- the old inline version raised its `is_bt` flag on
/// a matching PRODUCT STRING as well as a Bluetooth bus, so an Anticater with
/// an unlisted PID plugged into USB was reported as Bluetooth, and the UI
/// showed it as battery-powered.
pub fn classify_device(
    vid: u16,
    pid: u16,
    bluetooth_bus: bool,
    product: &str,
) -> Option<(String, TransportType)> {
    if let Some((_, _, name, transport)) = SUPPORTED_DEVICES
        .iter()
        .find(|(v, p, _, _)| *v == vid && *p == pid)
    {
        // The bus is ground truth: a device in the table paired over
        // Bluetooth is on Bluetooth, whatever the table's default says.
        let actual = if bluetooth_bus {
            TransportType::Bluetooth
        } else {
            *transport
        };
        return Some((name.to_string(), actual));
    }

    if looks_like_anticater(product) {
        // Unknown VID/PID but the name matches. Trust the BUS for the
        // transport, never the name.
        let transport = if bluetooth_bus {
            TransportType::Bluetooth
        } else {
            TransportType::Usb
        };
        let name = if product.is_empty() {
            format!("Anticater ({})", transport.display_name())
        } else {
            format!("Anticater ({})", product)
        };
        return Some((name, transport));
    }

    None
}

#[cfg(test)]
mod classify_tests {
    use super::*;

    const VK01_USB: (u16, u16) = (0x514C, 0x8850);
    const RECEIVER_24G: (u16, u16) = (0x25A7, 0xFA11);

    #[test]
    fn a_listed_device_takes_the_table_transport() {
        let (name, transport) =
            classify_device(VK01_USB.0, VK01_USB.1, false, "Anticater VK01").expect("listed");
        assert_eq!(transport, TransportType::Usb);
        assert!(name.contains("Anticater"), "unhelpful name: {name}");

        let (_, transport) =
            classify_device(RECEIVER_24G.0, RECEIVER_24G.1, false, "").expect("listed receiver");
        assert_eq!(transport, TransportType::Wireless24G);
    }

    /// The bus is ground truth. The same knob paired over Bluetooth must not
    /// report itself as wired just because the table lists it under USB --
    /// the transport drives the power readout, so getting this wrong tells
    /// the user a battery-powered device is bus-powered.
    #[test]
    fn a_listed_device_on_a_bluetooth_bus_is_bluetooth() {
        let (_, transport) =
            classify_device(VK01_USB.0, VK01_USB.1, true, "Anticater VK01").expect("listed");
        assert_eq!(transport, TransportType::Bluetooth);
    }

    /// The defect this extraction found. The old inline code set its
    /// `is_bt` flag from the product string as well as the bus, so an
    /// Anticater with an unlisted PID sitting on USB came back as Bluetooth.
    #[test]
    fn an_unlisted_anticater_on_usb_is_not_called_bluetooth() {
        let (_, transport) = classify_device(0xDEAD, 0xBEEF, false, "Anticater VK01 Rev B")
            .expect("name match should still be recognised");
        assert_eq!(
            transport,
            TransportType::Usb,
            "an unlisted device on a non-Bluetooth bus was reported as Bluetooth"
        );
    }

    #[test]
    fn an_unlisted_anticater_on_bluetooth_is_bluetooth() {
        let (_, transport) =
            classify_device(0xDEAD, 0xBEEF, true, "vk-01 knob").expect("name match");
        assert_eq!(transport, TransportType::Bluetooth);
    }

    #[test]
    fn the_name_match_is_case_insensitive_across_its_spellings() {
        for product in ["ANTICATER", "Anticater", "vk01", "VK-01", "my vk01 knob"] {
            assert!(
                classify_device(0xDEAD, 0xBEEF, false, product).is_some(),
                "{product:?} was not recognised"
            );
        }
    }

    /// A device that is neither in the table nor named like ours is not ours,
    /// on any bus. Without this the daemon would try to drive a random HID
    /// device that happened to be paired over Bluetooth.
    #[test]
    fn an_unrelated_device_is_never_claimed() {
        assert!(classify_device(0x05AC, 0x0265, false, "Apple Trackpad").is_none());
        assert!(classify_device(0x05AC, 0x0265, true, "Magic Keyboard").is_none());
        assert!(classify_device(0xDEAD, 0xBEEF, true, "").is_none());
    }

    #[test]
    fn a_nameless_match_still_gets_a_readable_name() {
        let (name, _) = classify_device(RECEIVER_24G.0, RECEIVER_24G.1, false, "").expect("listed");
        assert!(!name.is_empty());
    }
}
