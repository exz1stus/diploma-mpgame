//! Which menu screen is open, the form fields, and what the menu asks the session to do.

use std::net::SocketAddr;

use bevy::prelude::*;

use crate::net::session::SessionIntent;

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

    pub(super) return_to: Screen,
    pub(super) address: String,
    pub(super) seed: String,
    pub(super) port: String,

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
    pub(super) fn open(&mut self, screen: Screen) {
        self.return_to = self.screen;
        self.screen = screen;
        self.error = None;
    }
    pub(super) fn back_to_main(&mut self) {
        self.error = None;
        self.screen = Screen::Main;
    }
    pub(super) fn request(&mut self, intent: SessionIntent) {
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
        Screen::Join | Screen::Host | Screen::Error => menu.back_to_main(),
        Screen::Connecting => menu.request(SessionIntent::Leave),

        Screen::Main => {}
    }
}
