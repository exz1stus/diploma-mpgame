use std::net::SocketAddr;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use bevy_egui::{EguiContexts, egui};

use crate::networking::session::SessionIntent;

const GAME_TITLE: &str = "WORLD PROTOTYPE";
const INK: egui::Color32 = egui::Color32::from_rgb(15, 23, 26);
const PANEL: egui::Color32 = egui::Color32::from_rgb(23, 34, 37);
const BORDER: egui::Color32 = egui::Color32::from_rgb(78, 93, 87);
const CREAM: egui::Color32 = egui::Color32::from_rgb(235, 226, 202);
const MUTED: egui::Color32 = egui::Color32::from_rgb(157, 170, 158);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(222, 169, 91);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Main,
    Join,
    Host,
    Connecting,
    InGame,
    Pause,
    Options,
    Error,
    ConfirmLeave,
    ConfirmQuit,
}

#[derive(Resource)]
pub struct MenuState {
    pub screen: Screen,
    pub pending: Option<SessionIntent>,
    pub error: Option<String>,
    pub status: String,

    return_to: Screen,
    address: String,
    seed: String,
    port: String,

    pub show_diagnostics: bool,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            screen: Screen::Main,
            pending: None,
            error: None,
            status: String::new(),
            return_to: Screen::Main,
            address: "127.0.0.1:5000".into(),
            seed: "0".into(),
            port: "5000".into(),
            show_diagnostics: false,
        }
    }
}

impl MenuState {
    pub fn in_game(&self) -> bool {
        self.screen == Screen::InGame
    }
    pub fn set_default_address(&mut self, address: SocketAddr) {
        self.address = address.to_string();
    }
    fn open(&mut self, screen: Screen) {
        self.return_to = self.screen;
        self.screen = screen;
        self.error = None;
    }
    fn request(&mut self, intent: SessionIntent) {
        if self.pending.is_some() {
            return;
        }

        self.pending = Some(intent);
        self.error = None;
        self.status = "Please wait...".into();
        self.screen = Screen::Connecting;
    }
}

pub fn toggle_menu(keys: Res<ButtonInput<KeyCode>>, mut menu: ResMut<MenuState>) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match menu.screen {
        Screen::InGame => menu.screen = Screen::Pause,
        Screen::Pause => menu.screen = Screen::InGame,

        Screen::Options | Screen::ConfirmQuit => {
            menu.screen = menu.return_to;
        }

        Screen::ConfirmLeave => menu.screen = Screen::Pause,
        Screen::Join | Screen::Host | Screen::Error => {
            menu.error = None;
            menu.screen = Screen::Main;
        }
        Screen::Connecting => menu.request(SessionIntent::Leave),

        Screen::Main => {}
    }
}

pub fn menu_ui(
    mut contexts: EguiContexts,
    mut menu: ResMut<MenuState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) -> Result {
    if menu.in_game() {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;
    let mut viewport_ui = egui::Ui::new(
    ctx.clone(),
    "menu_viewport".into(),
    egui::UiBuilder::new()
        .layer_id(egui::LayerId::background())
        .max_rect(ctx.viewport_rect()),
);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(INK)
                .inner_margin(24.0),
        )
        .show(&mut viewport_ui, |ui| {
            paint_backdrop(ui.painter(), ui.max_rect());
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_max_width(440.0);
                ui.add_space(24.0);

                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(24.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        ui.label(
                            egui::RichText::new(GAME_TITLE)
                                .size(32.0)
                                .strong()
                                .color(CREAM),
                        );

                        ui.add_space(20.0);

                        match menu.screen {
                            Screen::Main => {
                                if action(ui, "JOIN GAME") {
                                    menu.open(Screen::Join);
                                }

                                if action(ui, "HOST WORLD") {
                                    menu.open(Screen::Host);
                                }

                                if action(ui, "OPTIONS") {
                                    menu.open(Screen::Options);
                                }

                                if action(ui, "QUIT") {
                                    menu.open(Screen::ConfirmQuit);
                                }
                            }

                            Screen::Join => {
                                ui.heading("Join game");
                                ui.label("Server address");
                                ui.add(
                                    egui::TextEdit::singleline(&mut menu.address)
                                        .hint_text("127.0.0.1:5000")
                                        .desired_width(f32::INFINITY),
                                );

                                ui.add_space(12.0);

                                if action(ui, "CONNECT") {
                                    match menu.address.trim().parse::<SocketAddr>() {
                                        Ok(address) if address.port() != 0
                                            && !address.ip().is_unspecified() =>
                                        {
                                            menu.request(SessionIntent::Join(address));
                                        }

                                        _ => {
                                            menu.error = Some(
                                                "Enter an IP address and a port from 1 to 65535."
                                                    .into(),
                                            );
                                        }
                                    }
                                }

                                if action(ui, "BACK") {
                                    menu.error = None;
                                    menu.screen = Screen::Main;
                                }
                            }

                            Screen::Host => {
                                ui.heading("Host a new world");
                                ui.label("World seed");
                                ui.text_edit_singleline(&mut menu.seed);

                                ui.label("Server port");
                                ui.text_edit_singleline(&mut menu.port);

                                ui.add_space(12.0);
                                ui.label(
                                    "Starts a fresh session. World saving is not implemented yet.",
                                );
                                ui.add_space(12.0);

                                if action(ui, "CREATE WORLD") {
                                    let seed = menu.seed.trim().parse::<u64>();
                                    let port = menu.port.trim().parse::<u16>();

                                    match (seed, port) {
                                        (Ok(seed), Ok(port)) if port != 0 => {
                                            menu.request(SessionIntent::Host { seed, port });
                                        }

                                        _ => {
                                            menu.error = Some(
                                                "Use a whole-number seed and a port from 1 to 65535."
                                                    .into(),
                                            );
                                        }
                                    }
                                }

                                if action(ui, "BACK") {
                                    menu.error = None;
                                    menu.screen = Screen::Main;
                                }
                            }

                            Screen::Connecting => {
                                ui.heading("Opening world");
                                ui.add_space(12.0);
                                ui.spinner();
                                ui.label(&menu.status);
                                ui.add_space(20.0);

                                if action(ui, "CANCEL") {
                                    menu.request(SessionIntent::Leave);
                                }

                                ctx.request_repaint();
                            }

                            Screen::Pause => {
                                ui.heading("Game menu");
                                ui.label("The multiplayer world keeps running.");
                                ui.add_space(16.0);

                                if action(ui, "RESUME") {
                                    menu.screen = Screen::InGame;
                                }

                                if action(ui, "OPTIONS") {
                                    menu.open(Screen::Options);
                                }

                                if action(ui, "LEAVE WORLD") {
                                    menu.open(Screen::ConfirmLeave);
                                }

                                if action(ui, "QUIT") {
                                    menu.open(Screen::ConfirmQuit);
                                }
                            }

                            Screen::Options => {
                                ui.heading("Options");

                                if let Ok(mut window) = windows.single_mut() {
                                    let mut fullscreen =
                                        !matches!(window.mode, WindowMode::Windowed);

                                    if ui
                                        .checkbox(
                                            &mut fullscreen,
                                            "Borderless fullscreen",
                                        )
                                        .changed()
                                    {
                                        window.mode = if fullscreen {
                                            WindowMode::BorderlessFullscreen(
                                                MonitorSelection::Current,
                                            )
                                        } else {
                                            WindowMode::Windowed
                                        };
                                    }
                                }

                                ui.checkbox(
                                    &mut menu.show_diagnostics,
                                    "Network diagnostics",
                                );

                                ui.add_space(16.0);
                                ui.label("WASD: move · Mouse: aim and shoot · Escape: menu");
                                ui.add_space(16.0);

                                if action(ui, "BACK") {
                                    menu.screen = menu.return_to;
                                }
                            }

                            Screen::ConfirmLeave => {
                                ui.heading("Leave this world?");
                                ui.label(
                                    "If you are hosting, this closes the server for everyone.",
                                );
                                ui.add_space(16.0);

                                if action(ui, "LEAVE WORLD") {
                                    menu.request(SessionIntent::Leave);
                                }

                                if action(ui, "STAY") {
                                    menu.screen = Screen::Pause;
                                }
                            }

                            Screen::ConfirmQuit => {
                                ui.heading("Quit the game?");
                                ui.label(
                                    "Any server hosted by this client will close.",
                                );
                                ui.add_space(16.0);

                                if action(ui, "QUIT") {
                                    menu.request(SessionIntent::Quit);
                                }

                                if action(ui, "CANCEL") {
                                    menu.screen = menu.return_to;
                                }
                            }

                            Screen::Error => {
                                ui.heading("Could not continue");
                                ui.add_space(12.0);

                                if action(ui, "BACK TO MENU") {
                                    menu.error = None;
                                    menu.screen = Screen::Main;
                                }
                            }

                            Screen::InGame => {}
                        }

                        if let Some(error) = &menu.error {
                            ui.add_space(16.0);
                            ui.colored_label(
                                egui::Color32::from_rgb(240, 145, 130),
                                error,
                            );
                        }
                    });
            });
        });

    Ok(())
}
fn action(ui: &mut egui::Ui, label: &str) -> bool {
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
