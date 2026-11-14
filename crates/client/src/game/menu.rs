use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use bevy_egui::{EguiContexts, egui};
use bevy_renet::RenetClient;

use crate::networking::state::ClientState;

const GAME_TITLE: &str = "WORLD PROTOTYPE";
const INK: egui::Color32 = egui::Color32::from_rgb(15, 23, 26);
const PANEL: egui::Color32 = egui::Color32::from_rgb(23, 34, 37);
const BORDER: egui::Color32 = egui::Color32::from_rgb(78, 93, 87);
const CREAM: egui::Color32 = egui::Color32::from_rgb(235, 226, 202);
const MUTED: egui::Color32 = egui::Color32::from_rgb(157, 170, 158);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(222, 169, 91);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Main,
    Options,
    InGame,
}

#[derive(Resource)]
pub struct MenuState {
    screen: Screen,
    has_entered_game: bool,
    pub show_diagnostics: bool,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            screen: Screen::Main,
            has_entered_game: false,
            show_diagnostics: false,
        }
    }
}

impl MenuState {
    pub fn in_game(&self) -> bool {
        self.screen == Screen::InGame
    }
}

pub fn toggle_menu(keys: Res<ButtonInput<KeyCode>>, mut menu: ResMut<MenuState>) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    menu.screen = match menu.screen {
        Screen::InGame | Screen::Options => Screen::Main,
        Screen::Main if menu.has_entered_game => Screen::InGame,
        Screen::Main => Screen::Main,
    };
}

pub fn menu_ui(
    mut contexts: EguiContexts,
    mut menu: ResMut<MenuState>,
    client: Res<RenetClient>,
    client_state: Res<ClientState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) -> Result {
    if menu.in_game() {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;
    let available = client.is_connected() && client_state.initiated;
    let status = if available {
        "SERVER READY"
    } else if client.is_disconnected() {
        "SERVER UNAVAILABLE"
    } else {
        "CONNECTING TO SERVER"
    };
    let status_color = if available { MUTED } else { ACCENT };

    egui::Area::new(egui::Id::new("main_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(ctx.content_rect().min)
        .movable(false)
        .show(ctx, |ui| {
            let rect = ctx.content_rect();
            ui.set_min_size(rect.size());
            ui.painter().rect_filled(rect, 0.0, INK);
            paint_backdrop(ui.painter(), rect);

            let compact = rect.width() < 700.0;
            let side = if compact { 24.0 } else { 64.0 };
            let width = (rect.width() - side * 2.0).clamp(250.0, 440.0);
            let top = if rect.height() < 600.0 { 35.0 } else { 75.0 };
            let panel_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + side, rect.top() + top),
                egui::vec2(width, (rect.height() - top - 30.0).max(360.0)),
            );
            ui.painter().rect_filled(panel_rect, 4.0, PANEL);
            ui.painter().rect_stroke(
                panel_rect,
                4.0,
                egui::Stroke::new(1.0, BORDER),
                egui::StrokeKind::Inside,
            );
            ui.painter().rect_filled(
                egui::Rect::from_min_size(panel_rect.min, egui::vec2(5.0, panel_rect.height())),
                0.0,
                ACCENT,
            );

            let content_rect = panel_rect.shrink2(egui::vec2(34.0, 34.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                ui.set_width(content_rect.width());
                ui.label(
                    egui::RichText::new("◆  EARLY DEVELOPMENT")
                        .size(12.0)
                        .strong()
                        .color(ACCENT),
                );
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new(GAME_TITLE)
                        .size(if compact { 32.0 } else { 39.0 })
                        .strong()
                        .color(CREAM),
                );
                ui.label(
                    egui::RichText::new("A multiplayer world in progress")
                        .size(14.0)
                        .color(MUTED),
                );
                ui.add_space(28.0);
                ui.separator();
                ui.add_space(25.0);

                match menu.screen {
                    Screen::Main => {
                        let label = if menu.has_entered_game {
                            "CONTINUE"
                        } else {
                            "PLAY"
                        };
                        if menu_button(ui, label, available, true) {
                            menu.has_entered_game = true;
                            menu.screen = Screen::InGame;
                        }
                        ui.add_space(10.0);
                        if menu_button(ui, "OPTIONS", true, false) {
                            menu.screen = Screen::Options;
                        }
                        ui.add_space(10.0);
                        if menu_button(ui, "QUIT", true, false) {
                            exit.write(AppExit::Success);
                        }
                    }
                    Screen::Options => {
                        ui.label(
                            egui::RichText::new("DISPLAY & TOOLS")
                                .color(ACCENT)
                                .strong(),
                        );
                        ui.add_space(14.0);
                        if let Ok(mut window) = windows.single_mut() {
                            let mut fullscreen = !matches!(window.mode, WindowMode::Windowed);
                            if ui
                                .checkbox(&mut fullscreen, "Borderless fullscreen")
                                .changed()
                            {
                                window.mode = if fullscreen {
                                    WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                                } else {
                                    WindowMode::Windowed
                                };
                            }
                        }
                        ui.checkbox(&mut menu.show_diagnostics, "Network diagnostics in game");
                        ui.add_space(18.0);
                        ui.label(
                            egui::RichText::new("WASD move  ·  Mouse aim and shoot  ·  Esc menu")
                                .size(13.0)
                                .color(MUTED),
                        );
                        ui.add_space(24.0);
                        if menu_button(ui, "BACK", true, false) {
                            menu.screen = Screen::Main;
                        }
                    }
                    Screen::InGame => unreachable!(),
                }

                ui.add_space(26.0);
                ui.separator();
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!("●  {status}"))
                        .size(12.0)
                        .strong()
                        .color(status_color),
                );
                if !available {
                    ui.label(
                        egui::RichText::new(if client.is_disconnected() {
                            "Start the server, then restart the client."
                        } else {
                            "Waiting for the server's world state."
                        })
                        .size(12.0)
                        .color(MUTED),
                    );
                }
            });

            ui.painter().text(
                egui::pos2(rect.right() - 28.0, rect.bottom() - 26.0),
                egui::Align2::RIGHT_BOTTOM,
                "PROTOTYPE  •  ART IN PROGRESS",
                egui::FontId::proportional(11.0),
                MUTED,
            );
        });
    Ok(())
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

fn paint_backdrop(painter: &egui::Painter, rect: egui::Rect) {
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
