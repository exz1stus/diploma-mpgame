mod client;
mod config;
mod diagnostics;
mod netgraph;
mod test_scenes;

use bevy::DefaultPlugins;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_renet::{RenetClientPlugin, netcode::NetcodeClientPlugin};
use client::GameClient;
use shared::app_init;

use crate::{
    config::ClientConfig, netgraph::network_diagnostics_ui, test_scenes::TestScenesPlugin,
};

fn main() {
    let mut app = app_init();

    app.add_plugins((
        DefaultPlugins,
        EguiPlugin::default(),
        RenetClientPlugin,
        NetcodeClientPlugin,
    ));

    app.add_plugins(TestScenesPlugin::from_env());
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
