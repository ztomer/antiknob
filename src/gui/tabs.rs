use crate::gui::state::ActiveTab;

pub const TABS: &[(ActiveTab, &str)] = &[
    (ActiveTab::Presets, "1-Click Presets"),
    (ActiveTab::Recorder, "Record Shortcut"),
    (ActiveTab::BaseKeys, "Keys & Typing"),
    (ActiveTab::Modifiers, "Combos & Modifiers"),
    (ActiveTab::Media, "Multimedia"),
    (ActiveTab::Led, "RGB LED Lighting"),
    (ActiveTab::Mouse, "Mouse & Scroll"),
    (ActiveTab::Procreate, "Creative & Procreate"),
];
