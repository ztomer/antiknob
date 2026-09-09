use anyhow::{anyhow, Context, Result};
use hidapi::HidApi;
use serde::{Deserialize, Serialize};

mod classify;
mod led_state;
pub mod mode;
mod select;
#[cfg(test)]
mod slot_address_tests;
mod slot_read;
pub mod snoop;
mod thread;
pub mod verify;
pub use classify::classify_device;
pub use led_state::{read_led_mode, send_led};
pub use select::{primary_device, primary_transport};
pub use slot_read::{read_full_table, read_slot, BURST_QUERIES, BURST_WIDTH};
pub use thread::{enumeration_refreshes, with_device, with_hid, HidDevice};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    #[serde(rename = "usb")]
    Usb,
    #[serde(rename = "wireless_2_4g")]
    Wireless24G,
    #[serde(rename = "bluetooth")]
    Bluetooth,
}

impl TransportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Usb => "usb",
            Self::Wireless24G => "wireless_2_4g",
            Self::Bluetooth => "bluetooth",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Usb => "USB (Wired)",
            Self::Wireless24G => "2.4GHz Wireless",
            Self::Bluetooth => "Bluetooth Wireless",
        }
    }
}

fn default_transport() -> TransportType {
    TransportType::Usb
}

pub const SUPPORTED_DEVICES: &[(u16, u16, &str, TransportType)] = &[
    (
        0x514C,
        0x8850,
        "Anticater / LQKJ VK01 (0x514c:0x8850)",
        TransportType::Usb,
    ),
    (
        0x514C,
        0x8851,
        "Anticater / LQKJ 2.4G (0x514c:0x8851)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8840,
        "Anticater / CH57x (0x1189:0x8840)",
        TransportType::Usb,
    ),
    (
        0x1189,
        0x8842,
        "Anticater / CH57x (0x1189:0x8842)",
        TransportType::Usb,
    ),
    (
        0x1189,
        0x8850,
        "Anticater / CH57x (0x1189:0x8850)",
        TransportType::Usb,
    ),
    (
        0x1189,
        0x8851,
        "Anticater / CH57x 2.4G (0x1189:0x8851)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8890,
        "Anticater / CH57x (0x1189:0x8890)",
        TransportType::Usb,
    ),
    (
        0x1189,
        0x8830,
        "Anticater / CH57x 2.4G (0x1189:0x8830)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8831,
        "Anticater / CH57x 2.4G (0x1189:0x8831)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8832,
        "Anticater / CH57x 2.4G (0x1189:0x8832)",
        TransportType::Wireless24G,
    ),
    (
        0x1189,
        0x8833,
        "Anticater / CH57x 2.4G (0x1189:0x8833)",
        TransportType::Wireless24G,
    ),
    (
        0x514C,
        0x8830,
        "Anticater / LQKJ 2.4G (0x514c:0x8830)",
        TransportType::Wireless24G,
    ),
    (
        0x514C,
        0x8831,
        "Anticater / LQKJ 2.4G (0x514c:0x8831)",
        TransportType::Wireless24G,
    ),
    (
        0x514C,
        0x8832,
        "Anticater / LQKJ 2.4G (0x514c:0x8832)",
        TransportType::Wireless24G,
    ),
    (
        0x514C,
        0x8833,
        "Anticater / LQKJ 2.4G (0x514c:0x8833)",
        TransportType::Wireless24G,
    ),
    (
        0x25A7,
        0xFA11,
        "Anticater 2.4G Receiver (0x25a7:0xfa11)",
        TransportType::Wireless24G,
    ),
];

pub const VENDOR_USAGE_PAGE: u16 = 0xFF00;
pub const REPORT_ID: u8 = 0x03;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMatch {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    pub serial_number: Option<String>,
    pub path: String,
    pub usage_page: u16,
    pub usage: u16,
    /// USB interface number (-1 when the backend does not report one).
    /// Defaults for forward compatibility with older status JSON.
    #[serde(default = "default_iface_no")]
    pub interface_number: i32,
    #[serde(default = "default_transport")]
    pub transport: TransportType,
}

fn default_iface_no() -> i32 {
    -1
}

/// Enumerates every attached Anticater/CH57x interface, across transports.
pub fn list_devices() -> Result<Vec<DeviceMatch>> {
    with_hid(list_devices_on)
}

/// Enumeration proper. Runs on the HID thread; see `with_hid`.
pub fn list_devices_on(api: &HidApi) -> Result<Vec<DeviceMatch>> {
    let mut matches = Vec::new();

    for dev in api.device_list() {
        let vid = dev.vendor_id();
        let pid = dev.product_id();
        let bluetooth_bus = matches!(dev.bus_type(), hidapi::BusType::Bluetooth);
        let match_info =
            classify_device(vid, pid, bluetooth_bus, dev.product_string().unwrap_or(""));

        if let Some((name, transport)) = match_info {
            matches.push(DeviceMatch {
                vendor_id: vid,
                product_id: pid,
                name,
                serial_number: dev.serial_number().map(|s| s.to_string()),
                path: dev.path().to_string_lossy().to_string(),
                usage_page: dev.usage_page(),
                usage: dev.usage(),
                interface_number: dev.interface_number(),
                transport,
            });
        }
    }

    Ok(matches)
}

/// Opens the vendor configuration interface. Private: the returned handle
/// is only valid on the HID thread, so callers go through `with_device`.
fn open_device_on(api: &HidApi) -> Result<HidDevice> {
    // On macOS, the vendor configuration endpoint has UsagePage 0xFF00
    // This interface does NOT require sudo or root privileges.
    let target = api.device_list().find(|d| {
        let vid = d.vendor_id();
        let pid = d.product_id();
        let is_supported = SUPPORTED_DEVICES
            .iter()
            .any(|(v, p, _, _)| *v == vid && *p == pid);
        is_supported && d.usage_page() == VENDOR_USAGE_PAGE
    });

    if let Some(target) = target {
        let dev = target.open_device(api).context(
            "Failed to open device interface. If permission is denied, ensure you have access to USB HID devices."
        )?;
        return Ok(dev);
    }

    // If a supported device is present (e.g. 2.4GHz wireless receiver) but lacks
    // VENDOR_USAGE_PAGE, fail with a clear, actionable error rather than attempting
    // to open a standard HID mouse/keyboard interface and reporting a misleading
    // permission-denied error.
    let has_supported = api.device_list().any(|d| {
        let vid = d.vendor_id();
        let pid = d.product_id();
        SUPPORTED_DEVICES
            .iter()
            .any(|(v, p, _, _)| *v == vid && *p == pid)
    });

    if has_supported {
        return Err(anyhow!(
            "Device is connected wirelessly. Hardware configuration (flashing slot bindings and LED modes) requires a direct USB-C wired connection."
        ));
    }

    Err(anyhow!(
        "No supported Anticater/CH57x device found. Please ensure device or receiver is connected."
    ))
}

/// Send a 64-byte payload to the device using Report ID 0x03.
/// If the passed payload already starts with Report ID 0x03 (e.g. legacy 64B or 65B packets),
/// the redundant prefix is stripped so the command byte lands at wire byte 1.
pub fn send_report(dev: &HidDevice, payload: &[u8]) -> Result<()> {
    let payload = if !payload.is_empty() && payload[0] == REPORT_ID {
        &payload[1..]
    } else {
        payload
    };
    let mut buf = [0u8; 65];
    buf[0] = REPORT_ID;
    let len = payload.len().min(64);
    buf[1..1 + len].copy_from_slice(&payload[..len]);

    dev.write(&buf)
        .context("Failed to write HID report to device")?;
    Ok(())
}

/// Open every interface of every supported device (keyboard, mouse,
/// consumer, vendor) for passive snooping. Opens NON-exclusively so the
/// OS keeps receiving input while we watch; never steals the keyboard.
pub struct SnoopIface {
    /// `vid:pid` -- short enough to prefix every report line.
    pub label: String,
    /// The collections this handle covers, named. Header only.
    pub detail: String,
    /// Every top-level collection this physical device declares. One handle
    /// carries all of them, so the decoder needs the whole set, not the one
    /// the handle happened to be enumerated under.
    pub usages: Vec<(u16, u16)>,
    pub device: HidDevice,
}

/// Runs on the HID thread; the handles must not outlive the job.
///
/// One handle per PHYSICAL device, not per usage. hidapi enumerates a macOS
/// device once per top-level collection, but `open_device` opens the whole
/// IOHIDDevice, so opening all five entries for the VK01 made every report
/// arrive five times -- once per handle, each labelled with a different
/// usage page, as though five interfaces had spoken. A capture that turns
/// one twist into five lines, four of them attributed to interfaces that
/// said nothing, is not a diagnostic. Deduplicate on the device path, which
/// is what identifies the IOHIDDevice being opened.
///
/// `filters` narrows to the given `VID:PID` pairs; empty watches every
/// supported device.
pub fn open_all_interfaces_on(
    api: &HidApi,
    filters: &[snoop::DeviceFilter],
) -> Result<Vec<SnoopIface>> {
    #[cfg(target_os = "macos")]
    api.set_open_exclusive(false);
    let mut out = Vec::new();
    let mut opened_paths = std::collections::HashSet::new();
    for dev in api.device_list() {
        let vid = dev.vendor_id();
        let pid = dev.product_id();
        if !SUPPORTED_DEVICES
            .iter()
            .any(|(v, p, _, _)| *v == vid && *p == pid)
        {
            continue;
        }
        if !snoop::wanted(filters, vid, pid) {
            continue;
        }
        if !opened_paths.insert(dev.path().to_owned()) {
            continue;
        }
        let path = dev.path();
        let usages: Vec<(u16, u16)> = api
            .device_list()
            .filter(|d| d.path() == path)
            .map(|d| (d.usage_page(), d.usage()))
            .collect();
        // The per-line prefix is the device alone; the collections it
        // carries are header material, printed once. Repeating four hex
        // pairs on every report made a capture unreadable.
        let label = format!("{vid:04x}:{pid:04x}");
        let detail = snoop::collections_label(&usages);
        match dev.open_device(api) {
            Ok(handle) => {
                if let Err(e) = handle.set_blocking_mode(false) {
                    eprintln!("note: nonblocking failed for {}: {}", label, e);
                    continue;
                }
                out.push(SnoopIface {
                    label,
                    detail,
                    usages,
                    device: handle,
                });
            }
            Err(e) => {
                eprintln!("note: cannot snoop {}: {}", label, e);
            }
        }
    }
    Ok(out)
}

/// Addresses covering the whole slot table, exactly once each.
///
/// `read_slot`'s first parameter is not an address. It is how many slots the
/// device should treat a layer as having, and `counter` then walks the
/// resulting table layer-major: with `slots_per_layer = 6`, counters 1-6 are
/// layer 1's six slots, 7-12 layer 2's, 13-18 layer 3's. Established against
/// hardware by sweeping groups 0x00-0x40 x counters 1-8 and reading the
/// pattern out of which (group, counter) pairs returned which (layer, key).
///
/// The first version of this walked groups 0x01..=0x24 x counters 1..=3 --
/// 108 reads that still missed a slot, because counters were capped at 3 for
/// no reason beyond the original `read-slots` loop having been written that
/// way. This is 18 reads and complete.
pub fn slot_table_addresses(slots_per_layer: u8, layers: u8) -> Vec<(u8, u8)> {
    (1..=slots_per_layer.saturating_mul(layers))
        .map(|counter| (slots_per_layer, counter))
        .collect()
}

/// Read every slot on the device. Used to confirm a flash landed; a write on
/// its own proves only that the packet was well-formed.
pub fn read_slot_table(dev: &HidDevice, slots_per_layer: u8, layers: u8) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (group, counter) in slot_table_addresses(slots_per_layer, layers) {
        if let Ok(bytes) = read_slot(dev, group, counter) {
            out.push(bytes);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    out
}

/// How many device layers the firmware holds.
pub const DEVICE_LAYERS: u8 = 3;

/// Commit staged key writes, mirroring the vendor app's `HID_write`
/// tail (`[FD FE FF]` + sleep): without it the firmware may ignore
/// flashed packets. Call once after a batch of key writes, never for
/// LED-only updates (uncharted; LED path untouched).
pub fn send_commit(dev: &HidDevice) -> Result<()> {
    let mut payload = [0u8; 64];
    payload[0] = 0xFD;
    payload[1] = 0xFE;
    payload[2] = 0xFF;
    send_report(dev, &payload)?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}
