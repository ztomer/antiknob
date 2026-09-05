use crate::config::{DeviceConfig, KnobConfig, LayerConfig};
use crate::device::{list_devices, open_device, send_report, DeviceMatch};
use crate::protocol::{build_led_packet, key_id_for_button, key_id_for_knob, Action, KnobEvent};
use anyhow::Result;
use std::sync::mpsc::{channel, Receiver, Sender};

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
    BaseKeys,
    Modifiers,
    Media,
    Led,
    Mouse,
    Procreate,
}

pub enum DeviceEvent {
    Found(Vec<DeviceMatch>),
    Status(String, bool),
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
    tx: Sender<DeviceEvent>,
    rx: Receiver<DeviceEvent>,
}

impl GuiState {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let state = Self {
            device: None,
            config: default_device_config(),
            active_layer: 0,
            selected: Selection::Knob {
                index: 0,
                target: KnobTarget::Press,
            },
            active_tab: ActiveTab::BaseKeys,
            status_message: "Scanning for Anticater VK01 USB device...".to_string(),
            status_is_ok: true,
            led_mode: 1,
            led_color: "white".to_string(),
            tx,
            rx,
        };
        state.refresh_device();
        state
    }
}

impl Default for GuiState {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiState {
    pub fn refresh_device(&self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || match list_devices() {
            Ok(devs) => {
                let _ = tx.send(DeviceEvent::Found(devs));
            }
            Err(e) => {
                let _ = tx.send(DeviceEvent::Status(
                    format!("Error scanning USB devices: {}", e),
                    false,
                ));
            }
        });
    }

    pub fn poll_events(&mut self) {
        while let Ok(evt) = self.rx.try_recv() {
            match evt {
                DeviceEvent::Found(devs) => {
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
                DeviceEvent::Status(msg, is_ok) => {
                    self.status_message = msg;
                    self.status_is_ok = is_ok;
                }
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

    pub fn save_to_device(&mut self) -> Result<()> {
        let tx = self.tx.clone();
        let config = self.config.clone();
        let layer_idx = self.active_layer as u8;
        let led_mode = self.led_mode;
        let led_color = self.led_color.clone();

        self.status_message = format!("Flashing Layer {} to device over USB...", layer_idx);
        self.status_is_ok = true;

        std::thread::spawn(move || {
            let res = (|| -> Result<usize> {
                let dev = open_device()?;
                let layer = config
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
                    let _ = tx.send(DeviceEvent::Status(
                        format!(
                            "Successfully flashed {} actions to Layer {} (No sudo)!",
                            count, layer_idx
                        ),
                        true,
                    ));
                }
                Err(e) => {
                    let _ = tx.send(DeviceEvent::Status(
                        format!("Flashing failed: {}", e),
                        false,
                    ));
                }
            }
        });

        Ok(())
    }

    pub fn apply_led_to_device(&mut self) {
        let tx = self.tx.clone();
        let layer_idx = self.active_layer as u8;
        let led_spec = format!("mode{} {}", self.led_mode, self.led_color);
        self.status_message = format!("Applying LED {}...", led_spec);
        std::thread::spawn(move || {
            let res = (|| -> Result<()> {
                let dev = open_device()?;
                let pkt = build_led_packet(layer_idx, &led_spec)?;
                send_report(&dev, &pkt)?;
                Ok(())
            })();
            match res {
                Ok(()) => {
                    let _ = tx.send(DeviceEvent::Status(
                        format!("LED lighting applied: {}", led_spec),
                        true,
                    ));
                }
                Err(e) => {
                    let _ = tx.send(DeviceEvent::Status(
                        format!("Failed to set LED: {}", e),
                        false,
                    ));
                }
            }
        });
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
