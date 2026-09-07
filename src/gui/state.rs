use crate::config::{DeviceConfig, KnobConfig, LayerConfig};
use crate::device::{list_devices, open_device, send_report, DeviceMatch};
use crate::protocol::{build_led_packet, key_id_for_button, key_id_for_knob, Action, KnobEvent};
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobTarget {
    Ccw,
    Cw,
    Press,
    PressCcw,
    PressCw,
}

impl KnobTarget {
    pub fn label(&self) -> &'static str {
        match self {
            KnobTarget::Ccw => "Rotate CCW",
            KnobTarget::Cw => "Rotate CW",
            KnobTarget::Press => "Press Down",
            KnobTarget::PressCcw => "Press + Rotate CCW",
            KnobTarget::PressCw => "Press + Rotate CW",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Knob { index: usize, target: KnobTarget },
    Button { row: usize, col: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Presets,
    Recorder,
    BaseKeys,
    Modifiers,
    Media,
    Led,
    Mouse,
    Procreate,
    Slots,
    Host,
}

pub struct GuiState {
    pub device: Option<DeviceMatch>,
    pub config: DeviceConfig,
    pub active_layer: usize,
    pub selected: Selection,
    pub active_tab: ActiveTab,
    pub status_message: String,
    pub status_is_ok: bool,
    pub led_mode: u8,
    pub led_color: String,
    pub is_recording: bool,
    pub recorded_shortcut: Option<String>,
    pub active_preset_id: Option<String>,
    pub profile_path: String,
    pub host_view: Option<crate::host::HostConfig>,
    pub host_error: Option<String>,
    pub host_edit: Option<(usize, crate::host::Gesture)>,
    pub host_recording: bool,
}

impl GuiState {
    /// Pure constructor: performs no HID I/O so it is safe to call from
    /// any thread (including test runners). The GUI performs the first
    /// `refresh_device()` on the main thread right after construction.
    pub fn new() -> Self {
        Self {
            device: None,
            config: default_device_config(),
            active_layer: 0,
            selected: Selection::Knob {
                index: 0,
                target: KnobTarget::Press,
            },
            active_tab: ActiveTab::Presets,
            status_message: "Scanning for Anticater VK01 USB device...".to_string(),
            status_is_ok: true,
            led_mode: 1,
            led_color: "white".to_string(),
            is_recording: false,
            recorded_shortcut: None,
            active_preset_id: Some("media_master".to_string()),
            profile_path: "config.yaml".to_string(),
            host_view: None,
            host_error: None,
            host_edit: None,
            host_recording: false,
        }
    }
}

impl Default for GuiState {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiState {
    /// Re-scan USB for a supported device, synchronously on the calling
    /// thread. The caller is always the GUI main thread; this must stay that
    /// way (see the main-thread requirement on `crate::device`).
    pub fn refresh_device(&mut self) {
        match list_devices() {
            Ok(devs) => {
                if let Some(dev) = devs.into_iter().next() {
                    self.status_message = format!(
                        "Connected: {} (VID: 0x{:04x}, PID: 0x{:04x})",
                        dev.name, dev.vendor_id, dev.product_id
                    );
                    self.status_is_ok = true;
                    self.device = Some(dev);
                } else {
                    self.status_message =
                        "No device detected. Connect Anticater VK01 via USB.".to_string();
                    self.status_is_ok = false;
                    self.device = None;
                }
            }
            Err(e) => {
                self.status_message = format!("Error scanning USB devices: {}", e);
                self.status_is_ok = false;
            }
        }
    }

    pub fn get_binding(&self, sel: Selection) -> Option<String> {
        let layer = self.config.layers.get(self.active_layer)?;
        match sel {
            Selection::Knob { index, target } => {
                let knob = layer.knobs.get(index)?;
                match target {
                    KnobTarget::Ccw => knob.ccw.clone(),
                    KnobTarget::Cw => knob.cw.clone(),
                    KnobTarget::Press => knob.press.clone(),
                    KnobTarget::PressCcw => None,
                    KnobTarget::PressCw => None,
                }
            }
            Selection::Button { row, col } => {
                layer.buttons.get(row).and_then(|r| r.get(col).cloned())
            }
        }
    }

    pub fn set_binding(&mut self, action_str: &str) {
        if self.active_layer >= self.config.layers.len() {
            return;
        }
        let layer = &mut self.config.layers[self.active_layer];
        match self.selected {
            Selection::Knob { index, target } => {
                if index >= layer.knobs.len() {
                    layer.knobs.resize(
                        index + 1,
                        KnobConfig {
                            ccw: None,
                            press: None,
                            cw: None,
                        },
                    );
                }
                let knob = &mut layer.knobs[index];
                let val = Some(action_str.to_string());
                match target {
                    KnobTarget::Ccw => knob.ccw = val,
                    KnobTarget::Cw => knob.cw = val,
                    KnobTarget::Press => knob.press = val,
                    KnobTarget::PressCcw => {}
                    KnobTarget::PressCw => {}
                }
            }
            Selection::Button { row, col } => {
                if row >= layer.buttons.len() {
                    layer.buttons.resize(row + 1, Vec::new());
                }
                let r = &mut layer.buttons[row];
                if col >= r.len() {
                    r.resize(col + 1, "none".to_string());
                }
                r[col] = action_str.to_string();
            }
        }
        self.status_message = format!("Set binding to \"{}\"", action_str);
        self.status_is_ok = true;
    }

    pub fn clear_current(&mut self) {
        if self.active_layer >= self.config.layers.len() {
            return;
        }
        let layer = &mut self.config.layers[self.active_layer];
        match self.selected {
            Selection::Knob { index, target } => {
                if let Some(knob) = layer.knobs.get_mut(index) {
                    match target {
                        KnobTarget::Ccw => knob.ccw = None,
                        KnobTarget::Cw => knob.cw = None,
                        KnobTarget::Press => knob.press = None,
                        KnobTarget::PressCcw => {}
                        KnobTarget::PressCw => {}
                    }
                }
            }
            Selection::Button { row, col } => {
                if let Some(r) = layer.buttons.get_mut(row) {
                    if let Some(b) = r.get_mut(col) {
                        *b = "none".to_string();
                    }
                }
            }
        }
        self.status_message = "Cleared current key binding".to_string();
    }

    pub fn clear_all_on_layer(&mut self) {
        if let Some(layer) = self.config.layers.get_mut(self.active_layer) {
            for knob in &mut layer.knobs {
                knob.ccw = None;
                knob.cw = None;
                knob.press = None;
            }
            for row in &mut layer.buttons {
                for b in row {
                    *b = "none".to_string();
                }
            }
        }
        self.status_message = format!("Cleared all bindings on Layer {}", self.active_layer);
    }

    pub fn record_key(&mut self, key_str: &str) {
        self.set_binding(key_str);
        self.recorded_shortcut = Some(key_str.to_string());
        self.is_recording = false;
        self.status_message = format!("Assigned '{}' to selected target", key_str);
        self.status_is_ok = true;
    }

    pub fn load_profile(&mut self, path_str: &str) -> Result<()> {
        let cfg = crate::gui::profiles::load_profile_file(std::path::Path::new(path_str))?;
        self.config = cfg;
        self.profile_path = path_str.to_string();
        self.status_message = format!("Loaded profile from {}", path_str);
        self.status_is_ok = true;
        Ok(())
    }

    pub fn save_profile(&mut self, path_str: &str) -> Result<()> {
        crate::gui::profiles::save_profile_file(&self.config, std::path::Path::new(path_str))?;
        self.profile_path = path_str.to_string();
        self.status_message = format!("Saved profile to {}", path_str);
        self.status_is_ok = true;
        Ok(())
    }

    /// Export the six built-in presets as a daemon host config (see
    /// `profiles::export_presets_to_host_file`). Reports the outcome in
    /// the status line. No HID, no threads.
    pub fn export_presets_to_host(&mut self, path: &std::path::Path) {
        match crate::gui::profiles::export_presets_to_host_file(path) {
            Ok((layers, warnings)) => {
                self.status_message = format!(
                    "Exported {} preset layers to {} ({} button warning(s))",
                    layers,
                    path.display(),
                    warnings
                );
                self.status_is_ok = true;
            }
            Err(e) => {
                self.status_message = format!("Export failed: {:#}", e);
                self.status_is_ok = false;
            }
        }
    }

    /// Flash the active layer plus the LED spec to the device. Runs
    /// synchronously on the GUI main thread: hidapi's macOS backend
    /// (IOHIDManager) traps when driven from a worker thread without a
    /// CFRunLoop, which crashed the app (SIGTRAP in `hid_enumerate`).
    /// USB writes with pacing complete in well under a second, so no
    /// background thread is needed.
    pub fn save_to_device(&mut self) -> Result<()> {
        let layer_idx = self.active_layer as u8;
        let led_mode = self.led_mode;
        let led_color = self.led_color.clone();

        self.status_message = format!("Flashing Layer {} to device over USB...", layer_idx);
        self.status_is_ok = true;

        let res = (|| -> Result<usize> {
            let dev = open_device()?;
            let layer = self
                .config
                .layers
                .get(layer_idx as usize)
                .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
            let mut sent_count = 0;

            for (knob_idx, knob) in layer.knobs.iter().enumerate() {
                if let Some(ref s) = knob.ccw {
                    let action = Action::parse(s)?;
                    let key_id = key_id_for_knob(knob_idx, KnobEvent::RotateCCW);
                    send_report(&dev, &action.to_packet(key_id, layer_idx))?;
                    sent_count += 1;
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
                if let Some(ref s) = knob.press {
                    let action = Action::parse(s)?;
                    let key_id = key_id_for_knob(knob_idx, KnobEvent::Press);
                    send_report(&dev, &action.to_packet(key_id, layer_idx))?;
                    sent_count += 1;
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
                if let Some(ref s) = knob.cw {
                    let action = Action::parse(s)?;
                    let key_id = key_id_for_knob(knob_idx, KnobEvent::RotateCW);
                    send_report(&dev, &action.to_packet(key_id, layer_idx))?;
                    sent_count += 1;
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
            }

            let mut btn_idx = 0;
            for row in &layer.buttons {
                for btn in row {
                    if btn != "none" && !btn.is_empty() {
                        let action = Action::parse(btn)?;
                        let key_id = key_id_for_button(btn_idx);
                        send_report(&dev, &action.to_packet(key_id, layer_idx))?;
                        sent_count += 1;
                        std::thread::sleep(std::time::Duration::from_millis(15));
                    }
                    btn_idx += 1;
                }
            }

            let led_spec = format!("mode{} {}", led_mode, led_color);
            if let Ok(pkt) = build_led_packet(layer_idx, &led_spec) {
                let _ = send_report(&dev, &pkt);
            }

            Ok(sent_count)
        })();

        match res {
            Ok(count) => {
                self.status_message = format!(
                    "Successfully flashed {} actions to Layer {} (No sudo)!",
                    count, layer_idx
                );
                self.status_is_ok = true;
            }
            Err(e) => {
                self.status_message = format!("Flashing failed: {}", e);
                self.status_is_ok = false;
            }
        }

        Ok(())
    }

    /// Switch the active device layer and push that layer's configured
    /// LED to the ring, so lighting follows the layer. Synchronous,
    /// main-thread HID. A sync failure is reported in the status line but
    /// never blocks the layer switch itself.
    pub fn switch_layer(&mut self, layer: usize) {
        self.active_layer = layer;
        self.sync_layer_led();
    }

    /// Send the active layer's configured `led` string to the device.
    /// No-op when the layer has no LED configured.
    pub fn sync_layer_led(&mut self) {
        let spec = match self
            .config
            .layers
            .get(self.active_layer)
            .and_then(|l| l.led.clone())
        {
            Some(spec) => spec,
            None => return,
        };
        let layer_idx = self.active_layer as u8;
        match (|| -> Result<()> {
            let dev = open_device()?;
            let pkt = build_led_packet(layer_idx, &spec)?;
            send_report(&dev, &pkt)?;
            Ok(())
        })() {
            Ok(()) => {
                self.status_message = format!("Layer {} active, LED synced ({})", layer_idx, spec);
                self.status_is_ok = true;
            }
            Err(e) => {
                self.status_message = format!("Layer {} active, LED sync failed: {}", layer_idx, e);
                self.status_is_ok = false;
            }
        }
    }

    /// Flash the one-time host-translate slot bindings
    /// (CCW=ctrl-alt-F16, Press=ctrl-alt-F17, CW=ctrl-alt-F18) to all
    /// device layers. Synchronous, main-thread HID. Hold+twist slots are
    /// deliberately left untouched (key IDs unverified).
    pub fn restore_slot_bindings(&mut self) {
        self.status_message = "Flashing slot bindings to device over USB...".to_string();
        self.status_is_ok = true;
        match (|| -> Result<usize> {
            let dev = open_device()?;
            crate::host::bind::flash_slot_bindings(&dev, &crate::host::bind::BIND_LAYERS)
        })() {
            Ok(count) => {
                self.status_message = format!(
                    "Slot bindings restored ({} packets). Verify with: antiknob listen",
                    count
                );
                self.status_is_ok = true;
            }
            Err(e) => {
                self.status_message = format!("Slot binding flash failed: {}", e);
                self.status_is_ok = false;
            }
        }
    }

    /// Send the current LED mode/color to the device, synchronously on the
    /// GUI main thread (same IOHIDManager thread-affinity requirement as
    /// `save_to_device`; the old worker-thread version of this function is
    /// what crashed the app when changing lighting mode).
    pub fn apply_led_to_device(&mut self) {
        let layer_idx = self.active_layer as u8;
        let led_spec = format!("mode{} {}", self.led_mode, self.led_color);
        self.status_message = format!("Applying LED {}...", led_spec);
        let res = (|| -> Result<()> {
            let dev = open_device()?;
            let pkt = build_led_packet(layer_idx, &led_spec)?;
            send_report(&dev, &pkt)?;
            Ok(())
        })();
        match res {
            Ok(()) => {
                self.status_message = format!("LED lighting applied: {}", led_spec);
                self.status_is_ok = true;
            }
            Err(e) => {
                self.status_message = format!("Failed to set LED: {}", e);
                self.status_is_ok = false;
            }
        }
    }
}

fn default_device_config() -> DeviceConfig {
    DeviceConfig {
        model: "Anticater VK01".to_string(),
        orientation: "normal".to_string(),
        rows: 1,
        columns: 1,
        knobs: 1,
        layers: vec![
            LayerConfig {
                buttons: vec![vec!["space".to_string()]],
                knobs: vec![KnobConfig {
                    ccw: Some("volumedown".to_string()),
                    press: Some("mute".to_string()),
                    cw: Some("volumeup".to_string()),
                }],
                led: Some("backlight white".to_string()),
            },
            LayerConfig {
                buttons: vec![vec!["enter".to_string()]],
                knobs: vec![KnobConfig {
                    ccw: Some("wheeldown".to_string()),
                    press: Some("mclick".to_string()),
                    cw: Some("wheelup".to_string()),
                }],
                led: Some("shock blue".to_string()),
            },
            LayerConfig {
                buttons: vec![vec!["cmd+z".to_string()]],
                knobs: vec![KnobConfig {
                    ccw: Some("brightnessdown".to_string()),
                    press: Some("play".to_string()),
                    cw: Some("brightnessup".to_string()),
                }],
                led: Some("backlight green".to_string()),
            },
        ],
    }
}
