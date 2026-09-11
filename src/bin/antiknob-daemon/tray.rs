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

/// Embedded knob artwork for the menu bar. Decode failure
/// degrades to the title-only tray (the layer number stays primary).
const TRAY_ICON_PNG: &[u8] = include_bytes!("../../../assets/knob-tray.png");
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
    OpenSettings,
    ReloadNow,
    OpenAccessibility,
    Quit,
}

pub struct TrayUi {
    tray: TrayIcon,
    layer_ids: Vec<muda::MenuId>,
    settings_id: muda::MenuId,
    reload_id: muda::MenuId,
    fix_id: Option<muda::MenuId>,
    quit_id: muda::MenuId,
    last_title: String,
    last_names: Vec<String>,
    last_tap_ok: bool,
    has_icon: bool,
}

/// What the menu says when macOS is not letting the daemon read the
/// keyboard. The menu bar is where the user is already looking when the
/// knob does nothing, so it carries the way out rather than a log line
/// they would have to know to go and read.
const FIX_LABEL: &str = "Knob gestures off — open Accessibility settings";

/// A tray that says nothing while the daemon is deaf is the reason the
/// user has to go looking for a log; the tooltip carries the state.
fn tooltip(title: &str, tap_ok: bool) -> String {
    if tap_ok {
        format!("Antiknob — layer {}", title)
    } else {
        format!("Antiknob — layer {} (knob gestures off)", title)
    }
}

fn layer_label(idx: usize, name: &str, active: bool) -> String {
    if active {
        format!("{} {} — {}", idx + 1, name, "active")
    } else {
        format!("{} {}", idx + 1, name)
    }
}

struct BuiltMenu {
    menu: Menu,
    layer_ids: Vec<muda::MenuId>,
    settings_id: muda::MenuId,
    reload_id: muda::MenuId,
    fix_id: Option<muda::MenuId>,
    quit_id: muda::MenuId,
}

fn build_menu(names: &[String], active: usize, tap_ok: bool) -> anyhow::Result<BuiltMenu> {
    let menu = Menu::new();
    let mut layer_ids = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let item = MenuItem::new(layer_label(i, name, i == active), true, None);
        layer_ids.push(item.id().clone());
        menu.append(&item)?;
    }
    menu.append(&PredefinedMenuItem::separator())?;
    let settings = MenuItem::new("Open Antiknob Settings…", true, None);
    let settings_id = settings.id().clone();
    menu.append(&settings)?;
    menu.append(&PredefinedMenuItem::separator())?;
    let fix_id = if tap_ok {
        None
    } else {
        let fix = MenuItem::new(FIX_LABEL, true, None);
        let id = fix.id().clone();
        menu.append(&fix)?;
        menu.append(&PredefinedMenuItem::separator())?;
        Some(id)
    };
    let reload = MenuItem::new("Reload Config", true, None);
    let reload_id = reload.id().clone();
    menu.append(&reload)?;
    let quit = PredefinedMenuItem::quit(None);
    let quit_id = quit.id().clone();
    menu.append(&quit)?;
    Ok(BuiltMenu {
        menu,
        layer_ids,
        settings_id,
        reload_id,
        fix_id,
        quit_id,
    })
}

impl TrayUi {
    pub fn new(tap: &Arc<Mutex<TapEngine>>, tap_ok: bool) -> anyhow::Result<Self> {
        let (names, idx) = {
            let t = tap.lock().map_err(|_| anyhow::anyhow!("tap lock"))?;
            (t.layer_names(), t.layer_idx())
        };
        let title = format!("{}", idx + 1);
        let built = build_menu(&names, idx, tap_ok)?;
        let icon_opt = tray_icon();
        let has_icon = icon_opt.is_some();
        let mut builder = TrayIconBuilder::new()
            .with_tooltip(tooltip(&title, tap_ok))
            .with_menu(Box::new(built.menu));
        if let Some(icon) = icon_opt {
            builder = builder.with_icon(icon).with_icon_as_template(true);
        } else {
            builder = builder.with_title(title.clone());
        }
        let tray = builder.build()?;
        Ok(Self {
            tray,
            layer_ids: built.layer_ids,
            settings_id: built.settings_id,
            reload_id: built.reload_id,
            fix_id: built.fix_id,
            quit_id: built.quit_id,
            last_title: title,
            last_names: names,
            last_tap_ok: tap_ok,
            has_icon,
        })
    }

    /// Refresh title and menu when layer state or layer names changed.
    pub fn refresh(&mut self, tap: &Arc<Mutex<TapEngine>>, tap_ok: bool) {
        let Ok(t) = tap.lock() else { return };
        let (names, idx) = (t.layer_names(), t.layer_idx());
        let title = format!("{}", idx + 1);
        if title != self.last_title || tap_ok != self.last_tap_ok {
            if !self.has_icon {
                self.tray.set_title(Some(title.clone()));
            }
            let _ = self.tray.set_tooltip(Some(tooltip(&title, tap_ok)));
            self.last_title = title;
        }
        if names != self.last_names || tap_ok != self.last_tap_ok {
            if let Ok(built) = build_menu(&names, idx, tap_ok) {
                self.tray.set_menu(Some(Box::new(built.menu)));
                self.layer_ids = built.layer_ids;
                self.settings_id = built.settings_id;
                self.reload_id = built.reload_id;
                self.fix_id = built.fix_id;
                self.quit_id = built.quit_id;
                self.last_names = names;
            }
        }
        self.last_tap_ok = tap_ok;
    }

    /// Drain pending menu selections into actions.
    pub fn poll(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.quit_id {
                out.push(TrayAction::Quit);
            } else if ev.id == self.settings_id {
                out.push(TrayAction::OpenSettings);
            } else if ev.id == self.reload_id {
                out.push(TrayAction::ReloadNow);
            } else if self.fix_id.as_ref() == Some(&ev.id) {
                out.push(TrayAction::OpenAccessibility);
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
    fn the_tooltip_says_when_gestures_are_off() {
        assert_eq!(tooltip("2", true), "Antiknob — layer 2");
        assert!(tooltip("2", false).contains("off"));
    }

    #[test]
    fn embedded_artwork_decodes_to_tray_icon() {
        assert!(tray_icon().is_some(), "bundled knob tray PNG must decode");
        let img = image::load_from_memory(TRAY_ICON_PNG).expect("valid PNG");
        assert_eq!((img.width(), img.height()), (64, 64));
    }
}
