//! Menu-bar presence for the daemon (Phase 5).
//!
//! Title shows the 1-based current layer (the vk01 SF-Symbol badge,
//! transliterated to text); the menu lists layers with a checkmark prefix
//! on the active one, plus Reload Config and Quit. All state lives in the
//! shared TapEngine; this module only renders and translates menu events.
//! Must run on the main thread (AppKit); the event tap lives on a worker
//! thread (proven safe by probe). Construction failure (e.g. headless) is
//! a normal `Err`, and the daemon continues without a tray.

use antiknob::host::tap::TapEngine;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use std::sync::{Arc, Mutex};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Embedded app artwork, downscaled for the menu bar. Decode failure
/// degrades to the title-only tray (the layer number stays primary).
const TRAY_ICON_PNG: &[u8] = include_bytes!("../../../assets/ak12-1024.png");
const TRAY_ICON_PX: u32 = 64;

fn tray_icon() -> Option<Icon> {
    let img = image::load_from_memory(TRAY_ICON_PNG).ok()?;
    let small = img.resize(
        TRAY_ICON_PX,
        TRAY_ICON_PX,
        image::imageops::FilterType::Lanczos3,
    );
    Icon::from_rgba(small.into_rgba8().into_raw(), TRAY_ICON_PX, TRAY_ICON_PX).ok()
}

pub enum TrayAction {
    SwitchLayer(usize),
    ReloadNow,
    Quit,
}

pub struct TrayUi {
    tray: TrayIcon,
    layer_ids: Vec<muda::MenuId>,
    reload_id: muda::MenuId,
    quit_id: muda::MenuId,
    last_title: String,
    last_names: Vec<String>,
}

fn layer_label(idx: usize, name: &str, active: bool) -> String {
    if active {
        format!("{} {} — {}", idx + 1, name, "active")
    } else {
        format!("{} {}", idx + 1, name)
    }
}

fn build_menu(
    names: &[String],
    active: usize,
) -> anyhow::Result<(Menu, Vec<muda::MenuId>, muda::MenuId, muda::MenuId)> {
    let menu = Menu::new();
    let mut layer_ids = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let item = MenuItem::new(layer_label(i, name, i == active), true, None);
        layer_ids.push(item.id().clone());
        menu.append(&item)?;
    }
    menu.append(&PredefinedMenuItem::separator())?;
    let reload = MenuItem::new("Reload Config", true, None);
    let reload_id = reload.id().clone();
    menu.append(&reload)?;
    let quit = PredefinedMenuItem::quit(None);
    let quit_id = quit.id().clone();
    menu.append(&quit)?;
    Ok((menu, layer_ids, reload_id, quit_id))
}

impl TrayUi {
    pub fn new(tap: &Arc<Mutex<TapEngine>>) -> anyhow::Result<Self> {
        let (names, idx) = {
            let t = tap.lock().map_err(|_| anyhow::anyhow!("tap lock"))?;
            (t.layer_names(), t.layer_idx())
        };
        let title = format!("{}", idx + 1);
        let (menu, layer_ids, reload_id, quit_id) = build_menu(&names, idx)?;
        let mut builder = TrayIconBuilder::new()
            .with_title(title.clone())
            .with_tooltip(format!("Antiknob — layer {}", title))
            .with_menu(Box::new(menu));
        if let Some(icon) = tray_icon() {
            builder = builder.with_icon(icon);
        }
        let tray = builder.build()?;
        Ok(Self {
            tray,
            layer_ids,
            reload_id,
            quit_id,
            last_title: title,
            last_names: names,
        })
    }

    /// Refresh title and menu when layer state or layer names changed.
    pub fn refresh(&mut self, tap: &Arc<Mutex<TapEngine>>) {
        let Ok(t) = tap.lock() else { return };
        let (names, idx) = (t.layer_names(), t.layer_idx());
        let title = format!("{}", idx + 1);
        if title != self.last_title {
            self.tray.set_title(Some(title.clone()));
            let _ = self
                .tray
                .set_tooltip(Some(format!("Antiknob — layer {}", title)));
            self.last_title = title;
        }
        if names != self.last_names {
            if let Ok((menu, layer_ids, reload_id, quit_id)) = build_menu(&names, idx) {
                self.tray.set_menu(Some(Box::new(menu)));
                self.layer_ids = layer_ids;
                self.reload_id = reload_id;
                self.quit_id = quit_id;
                self.last_names = names;
            }
        }
    }

    /// Drain pending menu selections into actions.
    pub fn poll(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.quit_id {
                out.push(TrayAction::Quit);
            } else if ev.id == self.reload_id {
                out.push(TrayAction::ReloadNow);
            } else if let Some(i) = self.layer_ids.iter().position(|id| *id == ev.id) {
                out.push(TrayAction::SwitchLayer(i));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_artwork_decodes_to_tray_icon() {
        assert!(tray_icon().is_some(), "bundled AK12 PNG must decode");
        let img = image::load_from_memory(TRAY_ICON_PNG).expect("valid PNG");
        assert_eq!((img.width(), img.height()), (1024, 1024));
    }
}
