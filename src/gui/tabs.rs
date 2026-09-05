use crate::gui::state::ActiveTab;

pub const TABS: &[(ActiveTab, &str)] = &[
    (ActiveTab::BaseKeys, "BaseKeys"),
    (ActiveTab::Modifiers, "Ctrl Shift Alt"),
    (ActiveTab::Media, "MutiMedia"),
    (ActiveTab::Led, "RGB LED"),
    (ActiveTab::Mouse, "Mouse / Gestures"),
    (ActiveTab::Procreate, "Procreate"),
];
