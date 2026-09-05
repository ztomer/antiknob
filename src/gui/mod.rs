pub mod canvas;
pub mod palette;
pub mod state;
pub mod tabs;

use anyhow::Result;
use eframe::egui::{self, Color32, Margin, Rounding, Stroke};
use state::GuiState;

pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("Antiknob - Anticater VK01 Configurator"),
        ..Default::default()
    };

    eframe::run_native(
        "Antiknob",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(AntiknobApp::new()))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Failed to run eframe: {}", e))
}

pub struct AntiknobApp {
    pub state: GuiState,
}

impl AntiknobApp {
    pub fn new() -> Self {
        Self {
            state: GuiState::new(),
        }
    }
}

impl Default for AntiknobApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for AntiknobApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.state.poll_events();
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        let mut visuals = egui::Visuals::dark();
        visuals.window_fill = Color32::from_rgb(26, 26, 30);
        visuals.panel_fill = Color32::from_rgb(24, 24, 28);
        ctx.set_visuals(visuals);

        // 1. Top Panel: Title, Status, Layer Selector
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("Antiknob");
                ui.label("•");

                let status_color = if self.state.device.is_some() {
                    Color32::from_rgb(46, 204, 113)
                } else {
                    Color32::from_rgb(230, 126, 34)
                };
                ui.colored_label(status_color, &self.state.status_message);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Refresh USB").clicked() {
                        self.state.refresh_device();
                    }

                    ui.separator();

                    for l in (0..=2).rev() {
                        let is_active = self.state.active_layer == l;
                        let text = format!("Layer {}", l);
                        if ui.selectable_label(is_active, text).clicked() {
                            self.state.active_layer = l;
                        }
                    }
                    ui.label("Active Layer:");
                });
            });
            ui.add_space(8.0);
        });

        // 2. Bottom Panel: Status bar & selection info
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Selected Target:");
                match self.state.selected {
                    state::Selection::Knob { target, .. } => {
                        ui.strong(format!("Knob [{}]", target.label()));
                    }
                    state::Selection::Button { .. } => {
                        ui.strong("Key 1 (Push Button)");
                    }
                }

                ui.separator();
                let cur = self
                    .state
                    .get_binding(self.state.selected)
                    .unwrap_or_else(|| "none".into());
                ui.label("Current Assignment:");
                ui.colored_label(Color32::from_rgb(0, 180, 255), cur);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Pure Rust • Unprivileged IOHIDManager • macOS arm64");
                });
            });
            ui.add_space(6.0);
        });

        // 3. Left Sidebar: Category Tabs
        egui::SidePanel::left("tabs_panel")
            .exact_width(170.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("Categories");
                ui.separator();

                for (tab, name) in tabs::TABS {
                    let is_selected = self.state.active_tab == *tab;
                    let resp = ui.add_sized(
                        [155.0, 36.0],
                        egui::SelectableLabel::new(is_selected, *name),
                    );
                    if resp.clicked() {
                        self.state.active_tab = *tab;
                    }
                }
            });

        // 4. Right Sidebar: Action Buttons
        egui::SidePanel::right("actions_panel")
            .exact_width(160.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("Actions");
                ui.separator();

                if ui
                    .add_sized([140.0, 36.0], egui::Button::new("Clear Selected"))
                    .clicked()
                {
                    self.state.clear_current();
                }

                ui.add_space(6.0);
                if ui
                    .add_sized([140.0, 36.0], egui::Button::new("Clear All Keys"))
                    .clicked()
                {
                    self.state.clear_all_on_layer();
                }

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(14.0);

                let save_btn = egui::Button::new("Save to Device")
                    .fill(Color32::from_rgb(0, 120, 215))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0, 160, 255)));

                if ui.add_sized([140.0, 48.0], save_btn).clicked() {
                    if let Err(e) = self.state.save_to_device() {
                        self.state.status_message = format!("Flash failed: {}", e);
                        self.state.status_is_ok = false;
                    }
                }
            });

        // 5. Central Panel: Hardware Canvas (Top) & Key Palette (Bottom)
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgb(20, 20, 24))
                .rounding(Rounding::same(8.0))
                .inner_margin(Margin::same(12.0))
                .show(ui, |ui| {
                    canvas::render_canvas(ui, &mut self.state);
                });

            ui.add_space(12.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    palette::render_palette(ui, &mut self.state);
                });
        });
    }
}
