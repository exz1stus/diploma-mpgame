mod client_events;
mod config;
mod entity;
mod handlers;
mod messages;
mod replication;
mod server;
mod simulation;
mod test_scenes;
mod world;
use std::time::Duration;

use bevy::{
    MinimalPlugins,
    app::{PluginGroup, ScheduleRunnerPlugin},
};
use bevy_renet::{RenetServerPlugin, netcode::NetcodeServerPlugin};
use shared::app_init;
use shared::role::NetworkRole;

use crate::{config::AppConfig, server::GameServer, test_scenes::TestScenesPlugin};

fn main() {
    let mut app = app_init(NetworkRole::Server);

    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
        RenetServerPlugin,
        NetcodeServerPlugin,
    ));
    app.add_plugins(TestScenesPlugin::from_env());

    let config = AppConfig::new();

    let mut server = GameServer::new(app, config);
    server.run();
}
