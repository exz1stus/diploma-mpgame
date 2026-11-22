//! The menu panel: a shared frame, then one function per [`Screen`].

use std::net::SocketAddr;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use bevy_egui::{EguiContexts, egui};

use super::state::{MenuState, Screen};
use crate::net::session::SessionIntent;
use crate::ui::theme::{CREAM, ERROR, INK, PANEL, action, paint_backdrop};

const GAME_TITLE: &str = "WORLD PROTOTYPE";

type PrimaryWindows<'w, 's> = Query<'w, 's, &'static mut Window, With<PrimaryWindow>>;

pub fn menu_ui(
    mut contexts: EguiContexts,
    mut menu: ResMut<MenuState>,
    mut windows: PrimaryWindows,
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
        .frame(egui::Frame::new().fill(INK).inner_margin(24.0))
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
                        title(ui);
                        show_screen(ui, &mut menu, &mut windows);
                        show_error(ui, &menu);
                    });
            });
        });

    Ok(())
}

fn title(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(GAME_TITLE)
            .size(32.0)
            .strong()
            .color(CREAM),
    );
    ui.add_space(20.0);
}

fn show_screen(ui: &mut egui::Ui, menu: &mut MenuState, windows: &mut PrimaryWindows) {
    match menu.screen {
        Screen::Main => main_screen(ui, menu),
        Screen::Join => join_screen(ui, menu),
        Screen::Host => host_screen(ui, menu),
        Screen::Connecting => connecting_screen(ui, menu),
        Screen::Pause => pause_screen(ui, menu),
        Screen::Options => options_screen(ui, menu, windows),
        Screen::ConfirmLeave => confirm_leave_screen(ui, menu),
        Screen::ConfirmQuit => confirm_quit_screen(ui, menu),
        Screen::Error => error_screen(ui, menu),
        Screen::InGame => {}
    }
}

fn show_error(ui: &mut egui::Ui, menu: &MenuState) {
    if let Some(error) = &menu.error {
        ui.add_space(16.0);
        ui.colored_label(ERROR, error);
    }
}

fn main_screen(ui: &mut egui::Ui, menu: &mut MenuState) {
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

fn join_screen(ui: &mut egui::Ui, menu: &mut MenuState) {
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
            Ok(address) if address.port() != 0 && !address.ip().is_unspecified() => {
                menu.request(SessionIntent::Join(address));
            }
            _ => {
                menu.error = Some("Enter an IP address and a port from 1 to 65535.".into());
            }
        }
    }
    if action(ui, "BACK") {
        menu.back_to_main();
    }
}

fn host_screen(ui: &mut egui::Ui, menu: &mut MenuState) {
    ui.heading("Host a new world");
    ui.label("World seed");
    ui.text_edit_singleline(&mut menu.seed);
    ui.label("Server port");
    ui.text_edit_singleline(&mut menu.port);
    ui.add_space(12.0);
    ui.label("Starts a fresh session. World saving is not implemented yet.");
    ui.add_space(12.0);

    if action(ui, "CREATE WORLD") {
        let seed = menu.seed.trim().parse::<u64>();
        let port = menu.port.trim().parse::<u16>();
        match (seed, port) {
            (Ok(seed), Ok(port)) if port != 0 => {
                menu.request(SessionIntent::Host { seed, port });
            }
            _ => {
                menu.error = Some("Use a whole-number seed and a port from 1 to 65535.".into());
            }
        }
    }
    if action(ui, "BACK") {
        menu.back_to_main();
    }
}

fn connecting_screen(ui: &mut egui::Ui, menu: &mut MenuState) {
    ui.heading("Opening world");
    ui.add_space(12.0);
    ui.spinner();
    ui.label(&menu.status);
    ui.add_space(20.0);

    if action(ui, "CANCEL") {
        menu.request(SessionIntent::Leave);
    }
    ui.ctx().request_repaint();
}

fn pause_screen(ui: &mut egui::Ui, menu: &mut MenuState) {
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

fn options_screen(ui: &mut egui::Ui, menu: &mut MenuState, windows: &mut PrimaryWindows) {
    ui.heading("Options");

    if let Ok(mut window) = windows.single_mut() {
        fullscreen_checkbox(ui, &mut window);
    }
    ui.checkbox(&mut menu.show_diagnostics, "Network diagnostics");
    ui.add_space(16.0);
    ui.label("WASD: move · I: inventory · E: interact · Escape: menu");
    ui.add_space(16.0);

    if action(ui, "BACK") {
        menu.screen = menu.return_to;
    }
}

fn fullscreen_checkbox(ui: &mut egui::Ui, window: &mut Window) {
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

fn confirm_leave_screen(ui: &mut egui::Ui, menu: &mut MenuState) {
    ui.heading("Leave this world?");
    ui.label("If you are hosting, this closes the server for everyone.");
    ui.add_space(16.0);

    if action(ui, "LEAVE WORLD") {
        menu.request(SessionIntent::Leave);
    }
    if action(ui, "STAY") {
        menu.screen = Screen::Pause;
    }
}

fn confirm_quit_screen(ui: &mut egui::Ui, menu: &mut MenuState) {
    ui.heading("Quit the game?");
    ui.label("Any server hosted by this client will close.");
    ui.add_space(16.0);

    if action(ui, "QUIT") {
        menu.request(SessionIntent::Quit);
    }
    if action(ui, "CANCEL") {
        menu.screen = menu.return_to;
    }
}

fn error_screen(ui: &mut egui::Ui, menu: &mut MenuState) {
    ui.heading("Could not continue");
    ui.add_space(12.0);

    if action(ui, "BACK TO MENU") {
        menu.back_to_main();
    }
}
