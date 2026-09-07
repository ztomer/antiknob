use crate::gui::presets::{apply_preset, ALL_PRESETS};
use crate::gui::state::{ActiveTab, GuiState};
use eframe::egui::{self, Color32, Rounding, Stroke, Ui};

pub fn render_palette(ui: &mut Ui, state: &mut GuiState) {
    ui.vertical(|ui| match state.active_tab {
        ActiveTab::Presets => render_presets(ui, state),
        ActiveTab::Recorder => render_recorder(ui, state),
        ActiveTab::BaseKeys => render_base_keys(ui, state),
        ActiveTab::Modifiers => render_modifiers(ui, state),
        ActiveTab::Media => render_media(ui, state),
        ActiveTab::Led => render_led_panel(ui, state),
        ActiveTab::Mouse => render_mouse(ui, state),
        ActiveTab::Procreate => render_procreate(ui, state),
        ActiveTab::Slots => render_slots(ui, state),
        ActiveTab::Host => crate::gui::host_view::render_host_view(ui, state),
    });
}

fn render_presets(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Curated 1-Click Workflow Presets");
    ui.label("Select a workflow preset to instantly map the knob, button, and LED lighting.");
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Host daemon:");
        if ui
            .button("Export Presets to Host Config")
            .on_hover_text("Write the six presets as daemon host layers (never overwrites)")
            .clicked()
        {
            match crate::host::default_config_path() {
                Ok(path) => state.export_presets_to_host(&path),
                Err(e) => {
                    state.status_message = format!("Export failed (HOME): {}", e);
                    state.status_is_ok = false;
                }
            }
        }
    });
    ui.separator();
    ui.add_space(6.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for preset in ALL_PRESETS {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(preset.name);
                    ui.label("•");
                    ui.colored_label(Color32::from_rgb(0, 180, 255), preset.category);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new("Apply Preset")
                            .fill(Color32::from_rgb(0, 120, 215))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0, 160, 255)));
                        if ui.add_sized([110.0, 26.0], btn).clicked() {
                            apply_preset(preset, state);
                            state.active_preset_id = Some(preset.id.to_string());
                        }
                    });
                });

                ui.label(preset.description);
                ui.horizontal(|ui| {
                    ui.small(format!(
                        "Knob: [<- {}]  [Press: {}]  [{} ->]  |  Key 1: [{}]  |  LED: {} {}",
                        preset.ccw,
                        preset.press,
                        preset.cw,
                        preset.button,
                        preset.led_mode,
                        preset.led_color
                    ));
                });
            });
            ui.add_space(4.0);
        }
    });
}

fn render_recorder(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Direct Keyboard Shortcut Recorder");
    ui.label("Press any key combination directly on your Mac keyboard to bind it instantly.");
    ui.separator();
    ui.add_space(10.0);

    let target_label = match state.selected {
        crate::gui::state::Selection::Knob { target, .. } => {
            format!("Knob [{}]", target.label())
        }
        crate::gui::state::Selection::Button { .. } => "Key 1 (Push Button)".to_string(),
    };

    ui.horizontal(|ui| {
        ui.label("Target to bind:");
        ui.colored_label(Color32::from_rgb(0, 180, 255), &target_label);
    });

    let current_bind = state
        .get_binding(state.selected)
        .unwrap_or_else(|| "none".into());
    ui.horizontal(|ui| {
        ui.label("Current Assignment:");
        ui.strong(format!("[{}]", current_bind));
    });

    ui.add_space(14.0);

    let (rec_text, rec_color) = if state.is_recording {
        (
            "Listening... Press any key combination (or click to Cancel)",
            Color32::from_rgb(220, 50, 50),
        )
    } else {
        (
            "Click to Record Keystroke / Shortcut",
            Color32::from_rgb(0, 120, 215),
        )
    };

    let btn = egui::Button::new(rec_text)
        .fill(rec_color)
        .rounding(Rounding::same(8.0));

    if ui
        .add_sized([ui.available_width() - 20.0, 48.0], btn)
        .clicked()
    {
        state.is_recording = !state.is_recording;
        if state.is_recording {
            state.status_message = "Press any key combo on your keyboard...".to_string();
        } else {
            state.status_message = "Shortcut recording cancelled".to_string();
        }
    }

    ui.add_space(14.0);
    ui.group(|ui| {
        ui.label("Tips:");
        ui.label("• Supports combinations: Cmd, Option, Ctrl, Shift + any key.");
        ui.label("• Function keys F1-F12, navigation keys, and space/enter are supported.");
        ui.label("• To clear a binding, use the 'Clear Selected' button in the sidebar.");
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
    ui.heading("Modifier Combinations");
    ui.separator();

    let combos: &[&[(&str, &str)]] = &[
        &[
            ("Ctrl+C (Copy)", "ctrl+c"),
            ("Ctrl+V (Paste)", "ctrl+v"),
            ("Ctrl+X (Cut)", "ctrl+x"),
            ("Ctrl+Z (Undo)", "ctrl+z"),
        ],
        &[
            ("Cmd+C (Mac Copy)", "cmd+c"),
            ("Cmd+V (Mac Paste)", "cmd+v"),
            ("Cmd+X (Mac Cut)", "cmd+x"),
            ("Cmd+Z (Mac Undo)", "cmd+z"),
            ("Cmd+Shift+Z", "cmd+shift+z"),
        ],
        &[
            ("Ctrl+A (Select All)", "ctrl+a"),
            ("Cmd+A (Mac Select)", "cmd+a"),
            ("Cmd+S (Save)", "cmd+s"),
            ("Cmd+W (Close Tab)", "cmd+w"),
        ],
        &[
            ("Alt+Tab", "alt+tab"),
            ("Cmd+Tab", "cmd+tab"),
            ("Ctrl+Shift+Esc", "ctrl+shift+esc"),
        ],
    ];

    for row in combos {
        ui.horizontal(|ui| {
            for (label, action) in *row {
                let resp = ui.add_sized([135.0, 32.0], egui::Button::new(*label));
                if resp.clicked() {
                    state.set_binding(action);
                }
            }
        });
    }
}

fn render_media(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Multimedia Controls");
    ui.separator();

    let media: &[&[(&str, &str)]] = &[
        &[
            ("Mute", "mute"),
            ("Vol Up", "volumeup"),
            ("Vol Down", "volumedown"),
            ("Play / Pause", "play"),
        ],
        &[
            ("Prev Track", "prev"),
            ("Next Track", "next"),
            ("Stop", "stop"),
            ("Brightness +", "brightnessup"),
            ("Brightness -", "brightnessdown"),
        ],
    ];

    for row in media {
        ui.horizontal(|ui| {
            for (label, action) in *row {
                let resp = ui.add_sized([115.0, 32.0], egui::Button::new(*label));
                if resp.clicked() {
                    state.set_binding(action);
                }
            }
        });
    }
}

fn render_led_panel(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("RGB LED Lighting");
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Mode:");
        if ui
            .selectable_label(state.led_mode == 1, "Mode 1 (Backlight)")
            .clicked()
        {
            state.led_mode = 1;
        }
        if ui
            .selectable_label(state.led_mode == 2, "Mode 2 (Breathing)")
            .clicked()
        {
            state.led_mode = 2;
        }
        if ui
            .selectable_label(state.led_mode == 3, "Mode 3 (Shock / Reactive)")
            .clicked()
        {
            state.led_mode = 3;
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
                let resp = ui.add_sized([105.0, 32.0], egui::Button::new(*label));
                if resp.clicked() {
                    state.set_binding(action);
                }
            }
        });
    }
}

fn render_procreate(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Procreate & Creative Shortcuts");
    ui.separator();

    let creative: &[&[(&str, &str)]] = &[
        &[
            ("Brush Tool (B)", "b"),
            ("Eraser Tool (E)", "e"),
            ("Eyedropper (I)", "i"),
            ("Selection (S)", "s"),
        ],
        &[
            ("Brush Size - ([)", "leftbracket"),
            ("Brush Size + (])", "rightbracket"),
            ("Zoom In (Cmd+=)", "cmd+equal"),
            ("Zoom Out (Cmd+-)", "cmd+minus"),
        ],
        &[
            ("Undo (Cmd+Z)", "cmd+z"),
            ("Redo (Cmd+Shift+Z)", "cmd+shift+z"),
            ("Fit Screen (Cmd+0)", "cmd+0"),
        ],
    ];

    for row in creative {
        ui.horizontal(|ui| {
            for (label, action) in *row {
                let resp = ui.add_sized([130.0, 32.0], egui::Button::new(*label));
                if resp.clicked() {
                    state.set_binding(action);
                }
            }
        });
    }
}

fn render_slots(ui: &mut Ui, state: &mut GuiState) {
    ui.heading("Host-Translate Slot Bindings");
    ui.label("One-time setup: binds the knob slots to fixed chords (CCW = ctrl-alt-F16, Press = ctrl-alt-F17, CW = ctrl-alt-F18) on all device layers, so a host-side translator can swallow them and run layered actions.");
    ui.separator();
    ui.add_space(6.0);

    ui.strong("Bound slots (3 per layer):");
    ui.label("Rotate CCW  ->  ctrl-alt-F16");
    ui.label("Press Down  ->  ctrl-alt-F17");
    ui.label("Rotate CW   ->  ctrl-alt-F18");
    ui.add_space(4.0);
    ui.colored_label(
        Color32::from_rgb(230, 126, 34),
        "Hold+twist slots are NOT bound here (key IDs unverified). Bind those with the vendor app.",
    );

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui
            .add_sized([210.0, 34.0], egui::Button::new("Restore Slot Bindings"))
            .clicked()
        {
            state.restore_slot_bindings();
        }
        if ui
            .add_sized([170.0, 34.0], egui::Button::new("Sync Layer LED"))
            .clicked()
        {
            state.sync_layer_led();
        }
    });

    ui.add_space(8.0);
    ui.small("Verify what the knob actually sends: antiknob listen --timeout-secs 10");
}
