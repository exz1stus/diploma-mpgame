mod assets;
mod config;
mod entities;
mod inventory;
mod net;
mod player;
mod render_layers;
mod schedule;
mod ui;
mod world;

use bevy::{DefaultPlugins, prelude::*, window::WindowPlugin};
use bevy_egui::EguiPlugin;
use shared::app_init;
use shared::role::NetworkRole;

use crate::{
    config::ClientConfig,
    entities::EntityVisualsPlugin,
    inventory::InventoryPlugin,
    net::NetPlugin,
    player::PlayerPlugin,
    schedule::SchedulePlugin,
    ui::UiPlugin,
    world::{ClockPlugin, WorldPresentationPlugin},
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
        SchedulePlugin,
        NetPlugin {
            config: ClientConfig::new(),
        },
        ClockPlugin,
        WorldPresentationPlugin,
        EntityVisualsPlugin,
        PlayerPlugin,
        InventoryPlugin,
        UiPlugin,
    ));

    app.run();
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
