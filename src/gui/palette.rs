use crate::gui::state::{ActiveTab, GuiState};
use eframe::egui::{self, Ui};

pub fn render_palette(ui: &mut Ui, state: &mut GuiState) {
    ui.vertical(|ui| match state.active_tab {
        ActiveTab::BaseKeys => render_base_keys(ui, state),
        ActiveTab::Modifiers => render_modifiers(ui, state),
        ActiveTab::Media => render_media(ui, state),
        ActiveTab::Led => render_led_panel(ui, state),
        ActiveTab::Mouse => render_mouse(ui, state),
        ActiveTab::Procreate => render_procreate(ui, state),
    });
}

fn key_btn(ui: &mut Ui, state: &mut GuiState, label: &str, action: &str) {
    let resp = ui.add_sized([75.0, 32.0], egui::Button::new(label));
    if resp.clicked() {
        state.set_binding(action);
    }
}

fn render_base_keys(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Standard & Typing Keys");
    ui.separator();

    let rows: &[&[(&str, &str)]] = &[
        &[
            ("ESC", "esc"),
            ("F1", "f1"),
            ("F2", "f2"),
            ("F3", "f3"),
            ("F4", "f4"),
            ("F5", "f5"),
            ("F6", "f6"),
            ("F7", "f7"),
            ("F8", "f8"),
            ("F9", "f9"),
            ("F10", "f10"),
            ("F11", "f11"),
            ("F12", "f12"),
        ],
        &[
            ("` ~", "grave"),
            ("1", "1"),
            ("2", "2"),
            ("3", "3"),
            ("4", "4"),
            ("5", "5"),
            ("6", "6"),
            ("7", "7"),
            ("8", "8"),
            ("9", "9"),
            ("0", "0"),
            ("-", "minus"),
            ("=", "equal"),
            ("Back", "backspace"),
        ],
        &[
            ("Tab", "tab"),
            ("Q", "q"),
            ("W", "w"),
            ("E", "e"),
            ("R", "r"),
            ("T", "t"),
            ("Y", "y"),
            ("U", "u"),
            ("I", "i"),
            ("O", "o"),
            ("P", "p"),
            ("[", "leftbracket"),
            ("]", "rightbracket"),
            ("\\", "backslash"),
        ],
        &[
            ("Caps", "capslock"),
            ("A", "a"),
            ("S", "s"),
            ("D", "d"),
            ("F", "f"),
            ("G", "g"),
            ("H", "h"),
            ("J", "j"),
            ("K", "k"),
            ("L", "l"),
            (";", "semicolon"),
            ("'", "quote"),
            ("Enter", "enter"),
        ],
        &[
            ("Shift", "shift"),
            ("Z", "z"),
            ("X", "x"),
            ("C", "c"),
            ("V", "v"),
            ("B", "b"),
            ("N", "n"),
            ("M", "m"),
            (",", "comma"),
            (".", "period"),
            ("/", "slash"),
            ("Space", "space"),
        ],
        &[
            ("Ins", "insert"),
            ("Del", "delete"),
            ("Home", "home"),
            ("End", "end"),
            ("PgUp", "pageup"),
            ("PgDn", "pagedown"),
            ("←", "left"),
            ("↑", "up"),
            ("↓", "down"),
            ("→", "right"),
        ],
    ];

    for row in rows {
        ui.horizontal(|ui| {
            for (label, action) in *row {
                key_btn(ui, state, label, action);
            }
        });
    }
}

fn render_modifiers(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Modifiers & Extended Function Keys");
    ui.separator();

    let combos: &[&[(&str, &str)]] = &[
        &[
            ("Ctrl+", "ctrl"),
            ("Shift+", "shift"),
            ("Alt / Opt+", "alt"),
            ("Cmd / Win+", "cmd"),
            ("Right Ctrl+", "rctrl"),
            ("Right Shift+", "rshift"),
            ("Right Alt+", "ralt"),
            ("Right Cmd+", "rcmd"),
        ],
        &[
            ("Ctrl+Shift+", "ctrl+shift"),
            ("Ctrl+Alt+", "ctrl+alt"),
            ("Ctrl+Cmd+", "ctrl+cmd"),
            ("Alt+Shift+", "alt+shift"),
            ("Cmd+Shift+", "cmd+shift"),
            ("Ctrl+Alt+Shift+", "ctrl+alt+shift"),
        ],
        &[
            ("F13", "f13"),
            ("F14", "f14"),
            ("F15", "f15"),
            ("F16", "f16"),
            ("F17", "f17"),
            ("F18", "f18"),
            ("F19", "f19"),
            ("F20", "f20"),
            ("F21", "f21"),
            ("F22", "f22"),
            ("F23", "f23"),
            ("F24", "f24"),
        ],
    ];

    for row in combos {
        ui.horizontal(|ui| {
            for (label, action) in *row {
                key_btn(ui, state, label, action);
            }
        });
    }
}

fn render_media(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Multimedia & Volume Controls");
    ui.separator();

    let media: &[&[(&str, &str)]] = &[
        &[
            ("Volume +", "volumeup"),
            ("Volume -", "volumedown"),
            ("Mute", "mute"),
            ("Play / Pause", "play"),
            ("Next Track", "next"),
            ("Prev Track", "prev"),
            ("Stop", "stop"),
        ],
        &[
            ("Brightness +", "brightnessup"),
            ("Brightness -", "brightnessdown"),
        ],
    ];

    for row in media {
        ui.horizontal(|ui| {
            for (label, action) in *row {
                key_btn(ui, state, label, action);
            }
        });
    }
}

fn render_led_panel(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("RGB Lighting Modes");
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Lighting Mode:");
        for mode in 0..=5 {
            let label = format!("Mode {}", mode);
            if ui.selectable_label(state.led_mode == mode, label).clicked() {
                state.led_mode = mode;
            }
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Color:");
        let colors = &[
            "white", "red", "green", "blue", "cyan", "magenta", "yellow", "off",
        ];
        for c in colors {
            if ui.selectable_label(state.led_color == *c, *c).clicked() {
                state.led_color = c.to_string();
            }
        }
    });

    ui.add_space(10.0);
    if ui.button("Apply Lighting to Device Now").clicked() {
        state.apply_led_to_device();
    }
}

fn render_mouse(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Mouse Clicks & Scroll Wheels");
    ui.separator();

    let mouse: &[&[(&str, &str)]] = &[&[
        ("Left Click", "lclick"),
        ("Right Click", "rclick"),
        ("Middle Click", "mclick"),
        ("Wheel Up", "wheelup"),
        ("Wheel Down", "wheeldown"),
    ]];

    for row in mouse {
        ui.horizontal(|ui| {
            for (label, action) in *row {
                key_btn(ui, state, label, action);
            }
        });
    }
}

fn render_procreate(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Procreate & Creative Shortcuts");
    ui.separator();

    let shortcuts: &[&[(&str, &str)]] = &[
        &[
            ("Undo", "cmd+z"),
            ("Redo", "cmd+shift+z"),
            ("Copy", "cmd+c"),
            ("Paste", "cmd+v"),
            ("Cut", "cmd+x"),
        ],
        &[
            ("Brush Tool", "b"),
            ("Eraser Tool", "e"),
            ("Brush Size +", "rightbracket"),
            ("Brush Size -", "leftbracket"),
            ("Color Picker", "alt"),
        ],
    ];

    for row in shortcuts {
        ui.horizontal(|ui| {
            for (label, action) in *row {
                key_btn(ui, state, label, action);
            }
        });
    }
}
