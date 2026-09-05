use crate::gui::state::{GuiState, KnobTarget, Selection};
use eframe::egui::{self, Color32, Rect, Rounding, Stroke, Ui, Vec2};

pub fn render_canvas(ui: &mut Ui, state: &mut GuiState) {
    ui.vertical_centered(|ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            render_knob_unit(ui, state, 0);

            ui.add_space(40.0);
            render_button_unit(ui, state, 0, 0);
        });
    });
}

fn render_knob_unit(ui: &mut Ui, state: &mut GuiState, knob_idx: usize) {
    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(260.0, 260.0), egui::Sense::hover());
    let center = rect.center();

    // 1. Center: Press Down
    let press_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::Press,
    };
    let is_press_sel = state.selected == press_sel;
    let press_binding = state
        .get_binding(press_sel)
        .unwrap_or_else(|| "Press".into());

    let press_rect = Rect::from_center_size(center, Vec2::new(90.0, 90.0));
    let press_resp = ui.allocate_rect(press_rect, egui::Sense::click());
    if press_resp.clicked() {
        state.selected = press_sel;
    }

    // Allocate zones before acquiring painter
    let ccw_rect = Rect::from_min_size(rect.min + Vec2::new(14.0, 14.0), Vec2::new(90.0, 60.0));
    let ccw_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::Ccw,
    };
    let ccw_resp = ui.allocate_rect(ccw_rect, egui::Sense::click());
    if ccw_resp.clicked() {
        state.selected = ccw_sel;
    }

    let cw_rect = Rect::from_min_size(
        rect.min + Vec2::new(rect.width() - 104.0, 14.0),
        Vec2::new(90.0, 60.0),
    );
    let cw_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::Cw,
    };
    let cw_resp = ui.allocate_rect(cw_rect, egui::Sense::click());
    if cw_resp.clicked() {
        state.selected = cw_sel;
    }

    let pccw_rect = Rect::from_min_size(
        rect.min + Vec2::new(14.0, rect.height() - 74.0),
        Vec2::new(90.0, 60.0),
    );
    let pccw_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::PressCcw,
    };
    let pccw_resp = ui.allocate_rect(pccw_rect, egui::Sense::click());
    if pccw_resp.clicked() {
        state.selected = pccw_sel;
    }

    let pcw_rect = Rect::from_min_size(
        rect.min + Vec2::new(rect.width() - 104.0, rect.height() - 74.0),
        Vec2::new(90.0, 60.0),
    );
    let pcw_sel = Selection::Knob {
        index: knob_idx,
        target: KnobTarget::PressCw,
    };
    let pcw_resp = ui.allocate_rect(pcw_rect, egui::Sense::click());
    if pcw_resp.clicked() {
        state.selected = pcw_sel;
    }

    // Now acquire painter and draw
    let painter = ui.painter();

    // Housing
    painter.rect(
        rect,
        Rounding::same(16.0),
        Color32::from_rgb(38, 38, 42),
        Stroke::new(2.0_f32, Color32::from_rgb(60, 60, 65)),
    );

    // Press center circle
    let press_color = if is_press_sel {
        Color32::from_rgb(0, 120, 215)
    } else if press_resp.hovered() {
        Color32::from_rgb(70, 70, 78)
    } else {
        Color32::from_rgb(50, 50, 56)
    };

    painter.circle(
        center,
        42.0,
        press_color,
        Stroke::new(2.0_f32, Color32::from_rgb(100, 100, 110)),
    );

    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        format!("↓\n{}", press_binding),
        egui::FontId::proportional(12.0),
        Color32::WHITE,
    );

    // Draw CCW
    draw_zone(
        painter,
        ccw_rect,
        state.selected == ccw_sel,
        ccw_resp.hovered(),
        "← CCW",
        &state.get_binding(ccw_sel).unwrap_or_else(|| "none".into()),
    );

    // Draw CW
    draw_zone(
        painter,
        cw_rect,
        state.selected == cw_sel,
        cw_resp.hovered(),
        "CW →",
        &state.get_binding(cw_sel).unwrap_or_else(|| "none".into()),
    );

    // Draw Press+CCW
    draw_zone(
        painter,
        pccw_rect,
        state.selected == pccw_sel,
        pccw_resp.hovered(),
        "⤿ P+CCW",
        &state.get_binding(pccw_sel).unwrap_or_else(|| "none".into()),
    );

    // Draw Press+CW
    draw_zone(
        painter,
        pcw_rect,
        state.selected == pcw_sel,
        pcw_resp.hovered(),
        "P+CW ⤾",
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
        Color32::from_rgb(0, 100, 180)
    } else if is_hovered {
        Color32::from_rgb(55, 55, 62)
    } else {
        Color32::from_rgb(44, 44, 50)
    };

    let stroke = if is_selected {
        Stroke::new(2.0_f32, Color32::from_rgb(0, 150, 255))
    } else {
        Stroke::new(1.0_f32, Color32::from_rgb(70, 70, 78))
    };

    painter.rect(rect, Rounding::same(8.0), bg_color, stroke);

    let text = format!("{}\n{}", title, binding);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );
}

fn render_button_unit(ui: &mut Ui, state: &mut GuiState, row: usize, col: usize) {
    let sel = Selection::Button { row, col };
    let is_selected = state.selected == sel;
    let binding = state.get_binding(sel).unwrap_or_else(|| "space".into());

    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(140.0, 260.0), egui::Sense::hover());
    let btn_rect = Rect::from_center_size(rect.center(), Vec2::new(110.0, 110.0));
    let btn_resp = ui.allocate_rect(btn_rect, egui::Sense::click());
    if btn_resp.clicked() {
        state.selected = sel;
    }

    let bg_color = if is_selected {
        Color32::from_rgb(0, 120, 215)
    } else if btn_resp.hovered() {
        Color32::from_rgb(65, 65, 72)
    } else {
        Color32::from_rgb(48, 48, 54)
    };

    let stroke = if is_selected {
        Stroke::new(2.0_f32, Color32::from_rgb(0, 180, 255))
    } else {
        Stroke::new(1.5_f32, Color32::from_rgb(80, 80, 90))
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        Rounding::same(16.0),
        Color32::from_rgb(38, 38, 42),
        Stroke::new(2.0_f32, Color32::from_rgb(60, 60, 65)),
    );

    painter.rect(btn_rect, Rounding::same(12.0), bg_color, stroke);

    let text = format!("Key 1\n[{}]", binding);
    painter.text(
        btn_rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );
}
