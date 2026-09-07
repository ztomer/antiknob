use anyhow::{anyhow, Context, Result};
use hidapi::{HidApi, HidDevice};

pub const SUPPORTED_DEVICES: &[(u16, u16, &str)] = &[
    (0x514C, 0x8850, "Anticater / LQKJ VK01 (0x514c:0x8850)"),
    (0x514C, 0x8851, "Anticater / LQKJ (0x514c:0x8851)"),
    (0x1189, 0x8840, "Anticater / CH57x (0x1189:0x8840)"),
    (0x1189, 0x8842, "Anticater / CH57x (0x1189:0x8842)"),
    (0x1189, 0x8850, "Anticater / CH57x (0x1189:0x8850)"),
    (0x1189, 0x8890, "Anticater / CH57x (0x1189:0x8890)"),
];

pub const VENDOR_USAGE_PAGE: u16 = 0xFF00;
pub const REPORT_ID: u8 = 0x03;

/// Thread-affinity contract (macOS): every function in this module drives
/// hidapi's IOHIDManager backend, which must run on the main thread.
/// Calling `HidApi::new()` from a worker thread without a CFRunLoop traps
/// inside `hid_enumerate` (`__CFCheckCFInfoPACSignature`, SIGTRAP) and kills
/// the process. The GUI therefore performs all HID work synchronously on
/// the main thread and never spawns threads around these calls.

#[derive(Debug, Clone)]
pub struct DeviceMatch {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    pub serial_number: Option<String>,
    pub path: String,
    pub usage_page: u16,
    pub usage: u16,
}

pub fn list_devices() -> Result<Vec<DeviceMatch>> {
    let api = HidApi::new().context("Failed to initialize HIDAPI")?;
    let mut matches = Vec::new();

    for dev in api.device_list() {
        let vid = dev.vendor_id();
        let pid = dev.product_id();

        if let Some((_, _, name)) = SUPPORTED_DEVICES
            .iter()
            .find(|(v, p, _)| *v == vid && *p == pid)
        {
            matches.push(DeviceMatch {
                vendor_id: vid,
                product_id: pid,
                name: name.to_string(),
                serial_number: dev.serial_number().map(|s| s.to_string()),
                path: dev.path().to_string_lossy().to_string(),
                usage_page: dev.usage_page(),
                usage: dev.usage(),
            });
        }
    }

    Ok(matches)
}

pub fn open_device() -> Result<HidDevice> {
    let api = HidApi::new().context("Failed to initialize HIDAPI")?;

    // On macOS, the vendor configuration endpoint has UsagePage 0xFF00
    // This interface does NOT require sudo or root privileges.
    let target = api
        .device_list()
        .find(|d| {
            let vid = d.vendor_id();
            let pid = d.product_id();
            let is_supported = SUPPORTED_DEVICES.iter().any(|(v, p, _)| *v == vid && *p == pid);
            is_supported && d.usage_page() == VENDOR_USAGE_PAGE
        })
        .or_else(|| {
            // Fallback for non-macOS or devices without UsagePage reporting
            api.device_list().find(|d| {
                let vid = d.vendor_id();
                let pid = d.product_id();
                SUPPORTED_DEVICES.iter().any(|(v, p, _)| *v == vid && *p == pid)
            })
        })
        .ok_or_else(|| anyhow!("No supported Anticater/CH57x keyboard found on USB. Please ensure device is plugged in."))?;

    let dev = target.open_device(&api).context(
        "Failed to open device interface. If permission is denied, ensure you have access to USB HID devices."
    )?;

    Ok(dev)
}

/// Send a 64-byte payload to the device using Report ID 0x03
pub fn send_report(dev: &HidDevice, payload: &[u8]) -> Result<()> {
    let mut buf = [0u8; 65];
    buf[0] = REPORT_ID;
    let len = payload.len().min(64);
    buf[1..1 + len].copy_from_slice(&payload[..len]);

    dev.write(&buf)
        .context("Failed to write HID report to device")?;
    Ok(())
}
