use bevy_egui::egui;
use shared::inventory::definitions::Rgba;

pub(super) const INK: egui::Color32 = egui::Color32::from_rgb(15, 23, 26);
pub(super) const PANEL: egui::Color32 = egui::Color32::from_rgb(23, 34, 37);
pub(super) const BORDER: egui::Color32 = egui::Color32::from_rgb(62, 79, 77);
pub(super) const CREAM: egui::Color32 = egui::Color32::from_rgb(235, 226, 202);
pub(super) const MUTED: egui::Color32 = egui::Color32::from_rgb(157, 170, 158);
pub(super) const ACCENT: egui::Color32 = egui::Color32::from_rgb(222, 169, 91);
pub(super) const GREEN: egui::Color32 = egui::Color32::from_rgb(111, 185, 139);

pub(super) fn color((r, g, b, a): Rgba) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub(super) fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(10)
        .inner_margin(16.0)
}

pub(super) fn style(ui: &mut egui::Ui) {
    let style = ui.style_mut();
    style.spacing.item_spacing = egui::vec2(8.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.visuals.override_text_color = Some(CREAM);
    style.visuals.extreme_bg_color = INK;
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    style.visuals.widgets.inactive.weak_bg_fill = color((35, 49, 51, 255));
    style.visuals.widgets.hovered.weak_bg_fill = color((51, 68, 67, 255));
    style.visuals.widgets.active.weak_bg_fill = color((68, 83, 75, 255));
}

pub(super) fn item_glyph(
    painter: &egui::Painter,
    rect: egui::Rect,
    item: &str,
    tint: egui::Color32,
) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.34;
    let stroke = egui::Stroke::new((r * 0.14).max(1.0), tint);
    match item {
        "wood" => {
            for offset in [-0.4, 0.4] {
                let center = c + egui::vec2(offset * r, offset * r * 0.55);
                painter.line_segment(
                    [
                        center + egui::vec2(-r * 0.55, r * 0.6),
                        center + egui::vec2(r * 0.55, -r * 0.6),
                    ],
                    egui::Stroke::new(r * 0.65, tint),
                );
                painter.circle_stroke(
                    center + egui::vec2(-r * 0.55, r * 0.6),
                    r * 0.23,
                    egui::Stroke::new(1.0, INK),
                );
            }
        }
        "healing_herb" => {
            painter.line_segment(
                [c + egui::vec2(-r * 0.4, r), c + egui::vec2(r * 0.4, -r)],
                stroke,
            );
            for (x, y) in [(-0.45, -0.4), (0.5, 0.0), (-0.55, 0.4)] {
                let leaf = c + egui::vec2(x * r, y * r);
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        leaf + egui::vec2(-r * 0.45, -r * 0.35),
                        leaf + egui::vec2(r * 0.35, -r * 0.15),
                        leaf + egui::vec2(r * 0.45, r * 0.35),
                        leaf + egui::vec2(-r * 0.35, r * 0.15),
                    ],
                    tint,
                    egui::Stroke::NONE,
                ));
            }
        }
        "stone" | "iron_ore" => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-r, r * 0.3),
                    c + egui::vec2(-r * 0.65, -r * 0.65),
                    c + egui::vec2(r * 0.25, -r),
                    c + egui::vec2(r, -r * 0.1),
                    c + egui::vec2(r * 0.7, r * 0.8),
                    c + egui::vec2(-r * 0.3, r),
                ],
                tint.gamma_multiply(0.75),
                stroke,
            ));
            painter.line_segment([c + egui::vec2(-r * 0.65, -r * 0.65), c], stroke);
            if item == "iron_ore" {
                painter.circle_filled(c + egui::vec2(r * 0.35, r * 0.15), r * 0.24, ACCENT);
            }
        }
        _ => {
            painter.circle_stroke(c, r, stroke);
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                item.chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string(),
                egui::FontId::proportional(r),
                tint,
            );
        }
    }
}
