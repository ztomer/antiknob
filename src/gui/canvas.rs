use crate::gui::state::{GuiState, KnobTarget, Selection};
use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Stroke, Ui, Vec2};

pub fn render_canvas(ui: &mut Ui, state: &mut GuiState) {
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            render_knob_unit(ui, state, 0);

            ui.add_space(32.0);
            render_button_unit(ui, state, 0, 0);
        });
    });
}

fn led_color_rgb(color_name: &str) -> (u8, u8, u8) {
    match color_name.to_lowercase().as_str() {
        "red" => (255, 65, 65),
        "green" => (45, 230, 115),
        "blue" => (45, 140, 255),
        "cyan" => (0, 235, 235),
        "magenta" => (245, 50, 245),
        "yellow" => (255, 220, 45),
        "off" => (40, 40, 45),
        _ => (240, 245, 255), // white
    }
}

fn compute_led_alpha(mode: u8, time: f64) -> f32 {
    match mode {
        1 => 0.85_f32, // Static / Backlight
        2 => {
            // Breathing / Fade
            let s = (time * 2.2).sin() as f32;
            (s * 0.42 + 0.58).clamp(0.15, 1.0)
        }
        3 => {
            // Shock / Reactive pulse
            let s = (time * 4.5).sin().abs() as f32;
            (s * 0.65 + 0.35).clamp(0.2, 1.0)
        }
        _ => 0.75_f32,
    }
}

fn render_knob_unit(ui: &mut Ui, state: &mut GuiState, knob_idx: usize) {
    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(290.0, 290.0), egui::Sense::hover());
    let center = rect.center();
    let time = ui.input(|i| i.time);

    // Interactive targets
    let press_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::Press,
    };
    let ccw_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::Ccw,
    };
    let cw_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::Cw,
    };
    let pccw_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::PressCcw,
    };
    let pcw_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::PressCw,
    };

    // Allocate interaction rects
    let press_rect = Rect::from_center_size(center, Vec2::new(88.0, 88.0));
    let press_resp = ui.allocate_rect(press_rect, egui::Sense::click());
    if press_resp.clicked() {
        state.selected = press_sel;
    }

    let ccw_rect = Rect::from_min_size(rect.min + Vec2::new(14.0, 16.0), Vec2::new(100.0, 68.0));
    let ccw_resp = ui.allocate_rect(ccw_rect, egui::Sense::click());
    if ccw_resp.clicked() {
        state.selected = ccw_sel;
    }

    let cw_rect = Rect::from_min_size(
        rect.min + Vec2::new(rect.width() - 114.0, 16.0),
        Vec2::new(100.0, 68.0),
    );
    let cw_resp = ui.allocate_rect(cw_rect, egui::Sense::click());
    if cw_resp.clicked() {
        state.selected = cw_sel;
    }

    let pccw_rect = Rect::from_min_size(
        rect.min + Vec2::new(14.0, rect.height() - 84.0),
        Vec2::new(100.0, 68.0),
    );
    let pccw_resp = ui.allocate_rect(pccw_rect, egui::Sense::click());
    if pccw_resp.clicked() {
        state.selected = pccw_sel;
    }

    let pcw_rect = Rect::from_min_size(
        rect.min + Vec2::new(rect.width() - 114.0, rect.height() - 84.0),
        Vec2::new(100.0, 68.0),
    );
    let pcw_resp = ui.allocate_rect(pcw_rect, egui::Sense::click());
    if pcw_resp.clicked() {
        state.selected = pcw_sel;
    }

    let painter = ui.painter();

    // 1. Chassis container
    painter.rect(
        rect,
        Rounding::same(18.0),
        Color32::from_rgb(32, 32, 36),
        Stroke::new(1.5_f32, Color32::from_rgb(55, 55, 62)),
    );

    // 2. Animated Live RGB LED Ring
    let (r, g, b) = led_color_rgb(&state.led_color);
    let alpha = compute_led_alpha(state.led_mode, time);
    let led_glow_color = Color32::from_rgba_premultiplied(
        ((r as f32) * alpha * 0.35) as u8,
        ((g as f32) * alpha * 0.35) as u8,
        ((b as f32) * alpha * 0.35) as u8,
        (255.0 * alpha) as u8,
    );
    let led_core_color = Color32::from_rgba_premultiplied(
        ((r as f32) * alpha) as u8,
        ((g as f32) * alpha) as u8,
        ((b as f32) * alpha) as u8,
        (255.0 * alpha) as u8,
    );

    // Outer ambient glow ring
    painter.circle_stroke(center, 98.0, Stroke::new(6.0_f32, led_glow_color));
    // Sharp neon ring
    painter.circle_stroke(center, 98.0, Stroke::new(2.5_f32, led_core_color));

    // 3. Main Knob Rotary Body
    painter.circle(
        center,
        92.0,
        Color32::from_rgb(42, 42, 48),
        Stroke::new(2.0_f32, Color32::from_rgb(68, 68, 76)),
    );
    painter.circle(
        center,
        78.0,
        Color32::from_rgb(48, 48, 55),
        Stroke::new(1.0_f32, Color32::from_rgb(76, 76, 86)),
    );

    // 4. Center: Press Button
    let is_press_sel = state.selected == press_sel;
    let press_bg = if is_press_sel {
        Color32::from_rgb(0, 120, 215)
    } else if press_resp.hovered() {
        Color32::from_rgb(72, 72, 82)
    } else {
        Color32::from_rgb(56, 56, 64)
    };

    let press_stroke = if is_press_sel {
        Stroke::new(2.5_f32, Color32::from_rgb(0, 180, 255))
    } else {
        Stroke::new(1.5_f32, Color32::from_rgb(95, 95, 105))
    };

    painter.circle(center, 40.0, press_bg, press_stroke);
    let press_bind = state
        .get_binding(press_sel)
        .unwrap_or_else(|| "mute".into());
    painter.text(
        center - Vec2::new(0.0, 8.0),
        egui::Align2::CENTER_CENTER,
        "PRESS",
        egui::FontId::proportional(11.0),
        Color32::from_rgb(220, 220, 230),
    );
    painter.text(
        center + Vec2::new(0.0, 10.0),
        egui::Align2::CENTER_CENTER,
        format!("[{}]", press_bind),
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );

    // 5. Draw 4 Directional Sectors
    draw_zone(
        painter,
        ccw_rect,
        state.selected == ccw_sel,
        ccw_resp.hovered(),
        "<- CCW",
        &state.get_binding(ccw_sel).unwrap_or_else(|| "none".into()),
    );
    draw_zone(
        painter,
        cw_rect,
        state.selected == cw_sel,
        cw_resp.hovered(),
        "CW ->",
        &state.get_binding(cw_sel).unwrap_or_else(|| "none".into()),
    );
    draw_zone(
        painter,
        pccw_rect,
        state.selected == pccw_sel,
        pccw_resp.hovered(),
        "<- Press+CCW",
        &state.get_binding(pccw_sel).unwrap_or_else(|| "none".into()),
    );
    draw_zone(
        painter,
        pcw_rect,
        state.selected == pcw_sel,
        pcw_resp.hovered(),
        "Press+CW ->",
        &state.get_binding(pcw_sel).unwrap_or_else(|| "none".into()),
    );
}

fn draw_zone(
    painter: &egui::Painter,
    rect: Rect,
    is_selected: bool,
    is_hovered: bool,
    title: &str,
    binding: &str,
) {
    let bg_color = if is_selected {
        Color32::from_rgb(0, 105, 185)
    } else if is_hovered {
        Color32::from_rgb(58, 58, 66)
    } else {
        Color32::from_rgb(44, 44, 50)
    };

    let stroke = if is_selected {
        Stroke::new(2.0_f32, Color32::from_rgb(0, 165, 255))
    } else {
        Stroke::new(1.0_f32, Color32::from_rgb(72, 72, 80))
    };

    painter.rect(rect, Rounding::same(10.0), bg_color, stroke);

    let title_pos = Pos2::new(rect.center().x, rect.min.y + 16.0);
    painter.text(
        title_pos,
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(11.0),
        Color32::from_rgb(200, 205, 215),
    );

    let pill_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.max.y - 20.0),
        Vec2::new(rect.width() - 14.0, 20.0),
    );
    painter.rect(
        pill_rect,
        Rounding::same(6.0),
        Color32::from_rgb(26, 26, 30),
        Stroke::new(1.0_f32, Color32::from_rgb(60, 60, 68)),
    );
    painter.text(
        pill_rect.center(),
        egui::Align2::CENTER_CENTER,
        binding,
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );
}

fn render_button_unit(ui: &mut Ui, state: &mut GuiState, row: usize, col: usize) {
    let sel = Selection::Button { row, col };
    let is_selected = state.selected == sel;
    let binding = state.get_binding(sel).unwrap_or_else(|| "space".into());

    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(150.0, 290.0), egui::Sense::hover());
    let btn_rect = Rect::from_center_size(rect.center(), Vec2::new(118.0, 118.0));
    let btn_resp = ui.allocate_rect(btn_rect, egui::Sense::click());
    if btn_resp.clicked() {
        state.selected = sel;
    }

    let bg_color = if is_selected {
        Color32::from_rgb(0, 120, 215)
    } else if btn_resp.hovered() {
        Color32::from_rgb(68, 68, 76)
    } else {
        Color32::from_rgb(48, 48, 55)
    };

    let stroke = if is_selected {
        Stroke::new(2.5_f32, Color32::from_rgb(0, 185, 255))
    } else {
        Stroke::new(1.5_f32, Color32::from_rgb(82, 82, 92))
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        Rounding::same(18.0),
        Color32::from_rgb(32, 32, 36),
        Stroke::new(1.5_f32, Color32::from_rgb(55, 55, 62)),
    );

    painter.rect(btn_rect, Rounding::same(14.0), bg_color, stroke);

    let key_cap = Rect::from_center_size(btn_rect.center(), Vec2::new(104.0, 104.0));
    painter.rect(
        key_cap,
        Rounding::same(10.0),
        Color32::from_rgba_premultiplied(255, 255, 255, 8),
        Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(255, 255, 255, 25)),
    );

    painter.text(
        key_cap.center() - Vec2::new(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        "KEY 1",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(210, 215, 225),
    );

    let pill_rect = Rect::from_center_size(
        key_cap.center() + Vec2::new(0.0, 16.0),
        Vec2::new(key_cap.width() - 16.0, 22.0),
    );
    painter.rect(
        pill_rect,
        Rounding::same(6.0),
        Color32::from_rgb(22, 22, 26),
        Stroke::new(1.0_f32, Color32::from_rgb(65, 65, 75)),
    );
    painter.text(
        pill_rect.center(),
        egui::Align2::CENTER_CENTER,
        binding,
        egui::FontId::proportional(12.0),
        Color32::WHITE,
    );
}
