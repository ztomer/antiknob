use anyhow::{anyhow, Context, Result};
use hidapi::{HidApi, HidDevice};
use serde::{Deserialize, Serialize};

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
/// hidapi's IOHIDManager backend, which must run on the main thread of a
/// process whose CFRunLoop is NOT currently dispatching: hid_enumerate
/// pumps the runloop reentrantly, which aborts inside a running GUI event
/// loop (SIGTRAP off-main-thread, SIGABRT reentrantly on it; both observed
/// on macOS 26).
///
/// Consequences: the CLI (plain main thread, no runloop) calls these
/// directly. The GUI must NEVER call them in-process; it shells out to the
/// CLI binary instead (see `gui::clihid`). Pinned structurally by
/// `tests/hid_main_thread.rs`.

#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Open every interface of every supported device (keyboard, mouse,
/// consumer, vendor) for passive snooping. Opens NON-exclusively so the
/// OS keeps receiving input while we watch; never steals the keyboard.
pub struct SnoopIface {
    pub label: String,
    pub usage_page: u16,
    pub usage: u16,
    pub device: HidDevice,
}

pub fn open_all_interfaces() -> Result<Vec<SnoopIface>> {
    let api = HidApi::new().context("Failed to initialize HIDAPI")?;
    #[cfg(target_os = "macos")]
    api.set_open_exclusive(false);
    let mut out = Vec::new();
    for dev in api.device_list() {
        let vid = dev.vendor_id();
        let pid = dev.product_id();
        if !SUPPORTED_DEVICES
            .iter()
            .any(|(v, p, _)| *v == vid && *p == pid)
        {
            continue;
        }
        let usage_page = dev.usage_page();
        let usage = dev.usage();
        let label = format!(
            "{:04x}:{:04x} up={:#06x} use={:#04x}",
            vid, pid, usage_page, usage
        );
        match dev.open_device(&api) {
            Ok(handle) => {
                if let Err(e) = handle.set_blocking_mode(false) {
                    eprintln!("note: nonblocking failed for {}: {}", label, e);
                    continue;
                }
                out.push(SnoopIface {
                    label,
                    usage_page,
                    usage,
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

/// Slot-table read query, reverse-engineered from the vendor app's
/// `Widget::read_Hidkey_Data`: write `[FA group 00 counter]` (report 0x03)
/// and read back the 64-byte slot dump. Read-only; changes no device state.
/// `group` selects the slot bank (`0x0F` / `0x19` observed), `counter`
/// walks entries 1..=3 within the bank.
pub fn read_slot(dev: &HidDevice, group: u8, counter: u8) -> Result<Vec<u8>> {
    let mut payload = [0u8; 64];
    payload[0] = 0xFA;
    payload[1] = group;
    payload[2] = 0x00;
    payload[3] = counter;
    send_report(dev, &payload)?;

    let mut buf = [0u8; 64];
    let n = dev
        .read_timeout(&mut buf, 500)
        .context("Timed out reading slot dump from device")?;
    Ok(buf[..n].to_vec())
}

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
