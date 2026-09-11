//! Which device, and which transport, out of everything attached.
//!
//! Split from `mod.rs` for the file-length cap, along the seam these two
//! share: both answer "of the many interfaces enumerated, which one is THE
//! one", and neither touches hidapi. Pure functions of a device list, so
//! both are testable with no hardware -- which matters, because the defect
//! they exist to prevent is a pane naming one device while every command on
//! it drives another.

use super::DeviceMatch;
use crate::firmware::{TransportType, VENDOR_USAGE_PAGE};

pub fn primary_transport(matches: &[DeviceMatch]) -> Option<TransportType> {
    primary_device(matches).map(|d| d.transport)
}

/// The interface every device command actually drives.
///
/// `list_devices` returns one entry per HID INTERFACE, and a knob publishes
/// several -- keyboard, consumer, mouse, and the vendor configuration
/// endpoint on usage page 0xFF00. Only the last of those accepts the
/// commands this tool sends, and it is the one `open_device_on` picks.
///
/// If no vendor endpoint is present (e.g. knob operating wirelessly via a
/// 2.4GHz USB receiver or Bluetooth), prefer the wireless receiver/link so
/// telemetry and status reflect the actual active knob transport rather
/// than an arbitrary first interface.
///
/// Same predicate as `open_device_on`, in the same order, including its
/// fallback for backends that do not report a usage page.
/// `the_primary_is_the_interface_commands_are_sent_to` pins the pairing.
pub fn primary_device(matches: &[DeviceMatch]) -> Option<&DeviceMatch> {
    matches
        .iter()
        .find(|m| m.usage_page == VENDOR_USAGE_PAGE)
        .or_else(|| {
            matches
                .iter()
                .find(|m| m.transport == TransportType::Wireless24G)
        })
        .or_else(|| {
            matches
                .iter()
                .find(|m| m.transport == TransportType::Bluetooth)
        })
        .or_else(|| matches.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(pid: u16, usage_page: u16, name: &str) -> DeviceMatch {
        DeviceMatch {
            vendor_id: 0x514C,
            product_id: pid,
            name: name.to_string(),
            serial_number: None,
            path: format!("DevSrvsID:{pid}{usage_page}"),
            usage_page,
            usage: 6,
            interface_number: 0,
            transport: TransportType::Usb,
        }
    }

    /// The interface every command is sent to, not the one that enumerated
    /// first. This is the real shape read off the machine on 2026-09-08: a
    /// `0x4155` keyboard published four interfaces ahead of the `0x8850`
    /// knob, so `devices[0]` named a device nothing was configuring.
    #[test]
    fn the_primary_is_the_interface_commands_are_sent_to() {
        let attached = vec![
            iface(0x4155, 0x0001, "Anticater / LiQi"),
            iface(0x4155, 0x000C, "Anticater / LiQi"),
            iface(0x8850, 0x0001, "Anticater / LQKJ VK01"),
            iface(0x8850, VENDOR_USAGE_PAGE, "Anticater / LQKJ VK01"),
        ];
        let picked = primary_device(&attached).expect("a vendor endpoint is attached");
        assert_eq!(picked.usage_page, VENDOR_USAGE_PAGE);
        assert_eq!(picked.product_id, 0x8850);
    }

    /// `open_device_on` falls back to any supported device when no backend
    /// reports a usage page, and so does this -- otherwise the pane would
    /// say "no device" for hardware the tool is happily driving.
    #[test]
    fn without_a_vendor_endpoint_the_first_supported_interface_stands_in() {
        let attached = vec![iface(0x8850, 0x0001, "Anticater / LQKJ VK01")];
        assert_eq!(
            primary_device(&attached).map(|d| d.product_id),
            Some(0x8850)
        );
    }

    /// Nothing attached is nothing to name. A stand-in here would be a
    /// device model printed beside a disconnected dot.
    #[test]
    fn nothing_attached_names_nothing() {
        assert!(primary_device(&[]).is_none());
        assert!(primary_transport(&[]).is_none());
    }

    /// When the knob is unplugged from USB, no vendor endpoint is present.
    /// The 2.4GHz wireless receiver must be picked over any non-knob interface,
    /// and primary_transport must report Wireless24G so the UI shows 2.4GHz
    /// rather than claiming a disconnected cable is still wired.
    #[test]
    fn wireless_receiver_is_primary_when_no_vendor_endpoint() {
        let attached = vec![DeviceMatch {
            vendor_id: 0x25A7,
            product_id: 0xFA11,
            name: "Anticater 2.4G Receiver".to_string(),
            serial_number: None,
            path: "DevSrvsID:fa11".to_string(),
            usage_page: 0x0001,
            usage: 6,
            interface_number: 0,
            transport: TransportType::Wireless24G,
        }];
        let dev = primary_device(&attached).expect("2.4g receiver present");
        assert_eq!(dev.product_id, 0xFA11);
        assert_eq!(
            primary_transport(&attached),
            Some(TransportType::Wireless24G)
        );
    }

    /// When both a wired knob (with vendor config endpoint) and a 2.4G receiver
    /// dongle are attached, the wired knob takes precedence because it can
    /// be flashed and configured directly.
    #[test]
    fn wired_knob_preferred_over_wireless_receiver_when_both_present() {
        let attached = vec![
            DeviceMatch {
                vendor_id: 0x25A7,
                product_id: 0xFA11,
                name: "Anticater 2.4G Receiver".to_string(),
                serial_number: None,
                path: "DevSrvsID:fa11".to_string(),
                usage_page: 0x0001,
                usage: 6,
                interface_number: 0,
                transport: TransportType::Wireless24G,
            },
            iface(0x8850, VENDOR_USAGE_PAGE, "Anticater / LQKJ VK01"),
        ];
        let dev = primary_device(&attached).expect("device present");
        assert_eq!(dev.product_id, 0x8850);
        assert_eq!(primary_transport(&attached), Some(TransportType::Usb));
    }
}
