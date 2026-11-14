mod config;
mod game;
mod networking;

use bevy::{DefaultPlugins, prelude::*, window::WindowPlugin};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_renet::{RenetClientPlugin, netcode::NetcodeClientPlugin};
use shared::app_init;
use shared::role::NetworkRole;

use crate::{
    config::ClientConfig,
    game::{
        debug_zoom,
        menu::{MenuState, menu_ui, toggle_menu},
        spawn_camera,
    },
    networking::{
        client::GameClient, netgraph::network_diagnostics_ui, world_streaming::WorldStreamingPlugin,
    },
};

fn main() {
    #[cfg(target_os = "linux")]
    prefer_wslg_x11();

    let mut app = app_init(NetworkRole::Client);

    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "World Prototype".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }),
        EguiPlugin::default(),
        RenetClientPlugin,
        NetcodeClientPlugin,
        WorldStreamingPlugin,
    ));

    app.add_systems(Startup, spawn_camera);
    app.init_resource::<MenuState>();
    app.add_systems(Update, (toggle_menu, debug_zoom));
    app.add_systems(
        EguiPrimaryContextPass,
        (menu_ui, network_diagnostics_ui).chain(),
    );

    let config = ClientConfig::new();
    let mut client = GameClient::new(app, &config);

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
