mod config;
mod game;
mod networking;

use bevy::{DefaultPlugins, prelude::Startup};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_renet::{RenetClientPlugin, netcode::NetcodeClientPlugin};
use shared::app_init;
use shared::role::NetworkRole;

use crate::{
    config::ClientConfig,
    game::spawn_camera,
    networking::{client::GameClient, netgraph::network_diagnostics_ui},
};

fn main() {
    let mut app = app_init(NetworkRole::Client);

    app.add_plugins((
        DefaultPlugins,
        EguiPlugin::default(),
        RenetClientPlugin,
        NetcodeClientPlugin,
    ));

    app.add_systems(Startup, spawn_camera);
    app.add_systems(EguiPrimaryContextPass, network_diagnostics_ui);

    let config = ClientConfig::new();
    let mut client = GameClient::new(app, &config);

    #[cfg(target_os = "linux")]
    prefer_wslg_x11();

    client.run();
}

#[cfg(target_os = "linux")]
fn prefer_wslg_x11() {
    if std::env::var_os("WSL_DISTRO_NAME").is_none() || std::env::var_os("DISPLAY").is_none() {
        return;
    }

    // Winit selects Wayland first, but WSLg's Wayland socket may be unavailable
    // while its X11 display still works. Change the environment before Bevy starts.
    unsafe {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
}
