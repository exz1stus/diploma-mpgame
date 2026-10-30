mod config;
mod game;
mod networking;
use std::time::Duration;

use avian2d::prelude::PhysicsPlugins;

use bevy::{
    MinimalPlugins,
    app::{PluginGroup, ScheduleRunnerPlugin},
};
use bevy_renet::{RenetServerPlugin, netcode::NetcodeServerPlugin};
use shared::app_init;
use shared::role::NetworkRole;

use crate::{config::AppConfig, game::GamePlugin, networking::ServerPlugin};

fn main() {
    let mut app = app_init(NetworkRole::Server);

    let config = AppConfig::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
        PhysicsPlugins::default(),
        RenetServerPlugin,
        NetcodeServerPlugin,
        ServerPlugin::new(config),
        GamePlugin,
    ));

    app.run();
}
