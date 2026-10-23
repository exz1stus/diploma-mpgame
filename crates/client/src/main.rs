mod client;
mod config;
mod diagnostics;
mod netgraph;

use bevy::{
    DefaultPlugins,
    asset::Assets,
    color::Color,
    ecs::system::ResMut,
    math::primitives::Circle,
    mesh::{Mesh, Mesh2d},
    prelude::{Camera2d, Commands, Name, Startup},
    sprite_render::{ColorMaterial, MeshMaterial2d},
    transform::components::Transform,
};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_renet::{RenetClientPlugin, netcode::NetcodeClientPlugin};
use client::GameClient;
use shared::{
    app_init,
    components::{Health, Position},
    protocol::NetworkIdentity,
};

use crate::{config::ClientConfig, netgraph::network_diagnostics_ui};

fn spawn(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Name::new("Test circle"),
        NetworkIdentity { id: 1 },
        Position { x: 0.0, y: 0.0 },
        Health(100),
        // Circle
        Mesh2d(meshes.add(Circle::new(20.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.2, 0.8, 0.4))),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}
fn main() {
    let mut app = app_init();

    app.add_plugins((
        DefaultPlugins,
        EguiPlugin::default(),
        RenetClientPlugin,
        NetcodeClientPlugin,
    ));

    app.add_systems(Startup, (spawn, spawn_camera));
    app.add_systems(EguiPrimaryContextPass, network_diagnostics_ui);

    let config = ClientConfig::new();
    let mut client = GameClient::new(app, &config);

    #[cfg(target_os = "linux")]
    prefer_wslg_x11();

    client.run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
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
