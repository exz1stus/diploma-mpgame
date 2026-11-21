//! Colors and shared widgets of the menus.

use bevy_egui::egui;

pub const INK: egui::Color32 = egui::Color32::from_rgb(15, 23, 26);
pub const PANEL: egui::Color32 = egui::Color32::from_rgb(23, 34, 37);
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(78, 93, 87);
pub const CREAM: egui::Color32 = egui::Color32::from_rgb(235, 226, 202);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(157, 170, 158);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(222, 169, 91);
pub const ERROR: egui::Color32 = egui::Color32::from_rgb(240, 145, 130);

/// A full-width menu button, spaced from the widget above it.
pub fn action(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add_space(8.0);
    menu_button(ui, label, true, false)
}

fn menu_button(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> bool {
    let fill = if primary {
        ACCENT
    } else {
        egui::Color32::from_rgb(35, 49, 51)
    };
    let text = if primary { INK } else { CREAM };
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(format!("  {label}  ›"))
                .size(17.0)
                .strong()
                .color(text),
        )
        .fill(fill)
        .stroke(egui::Stroke::new(
            1.0,
            if primary { ACCENT } else { BORDER },
        ))
        .min_size(egui::vec2(ui.available_width(), 50.0)),
    )
    .clicked()
}

pub fn paint_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    let w = rect.width();
    let h = rect.height();
    let x = rect.left();
    let y = rect.top();

    // Flat shapes make a readable placeholder scene until the final background texture arrives.
    painter.circle_filled(
        egui::pos2(x + w * 0.76, y + h * 0.34),
        h.min(w) * 0.18,
        egui::Color32::from_rgb(92, 80, 59),
    );
    painter.circle_filled(
        egui::pos2(x + w * 0.76, y + h * 0.34),
        h.min(w) * 0.145,
        egui::Color32::from_rgb(179, 132, 75),
    );
    for (height, color) in [
        (0.65, egui::Color32::from_rgb(39, 55, 54)),
        (0.77, egui::Color32::from_rgb(29, 44, 44)),
        (0.88, egui::Color32::from_rgb(21, 34, 36)),
    ] {
        let base = y + h * (height + 0.10);
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x, base), rect.right_bottom()),
            0.0,
            color,
        );
        for (center, peak, half_width) in [(0.65, 0.14, 0.18), (0.84, 0.09, 0.15)] {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(x + w * (center - half_width), base),
                    egui::pos2(x + w * center, y + h * (height - peak)),
                    egui::pos2(x + w * (center + half_width), base),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
    }
    for i in 0..26 {
        let px = x + w * (0.39 + (i * 37 % 61) as f32 / 100.0);
        let py = y + h * (0.10 + (i * 23 % 49) as f32 / 100.0);
        painter.circle_filled(egui::pos2(px, py), 1.0, MUTED);
    }
}
