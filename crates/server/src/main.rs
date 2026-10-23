mod client_events;
mod config;
mod messages;
mod server;
use std::time::Duration;

use bevy::{
    MinimalPlugins,
    app::{PluginGroup, ScheduleRunnerPlugin},
};
use bevy_renet::{RenetServerPlugin, netcode::NetcodeServerPlugin};
use shared::app_init;

use crate::{config::AppConfig, server::GameServer};

fn main() {
    let mut app = app_init();

    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
        RenetServerPlugin,
        NetcodeServerPlugin,
    ));

    let config = AppConfig::new();

    let mut server = GameServer::new(app, &config);
    server.run();
}
