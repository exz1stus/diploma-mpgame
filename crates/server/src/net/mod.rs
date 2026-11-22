use std::{
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::app::{App, Plugin};
use bevy_renet::{
    RenetServer,
    netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig},
};

use crate::config::AppConfig;

pub(crate) mod client;
pub mod commands;
pub(crate) mod entity;
mod handlers;
pub(crate) mod input;
pub(crate) mod messages;
mod ownership;
mod physics;
mod replication;
pub(crate) mod server;
mod world;
pub(crate) mod world_streaming;

pub(crate) struct ServerPlugin {
    config: AppConfig,
}

impl ServerPlugin {
    pub(crate) fn new(config: AppConfig) -> Self {
        Self { config }
    }
}

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        let socket = UdpSocket::bind(self.config.server_addr).unwrap();
        let server_config = ServerConfig {
            protocol_id: self.config.protocol_id,
            public_addresses: vec![self.config.server_addr],
            max_clients: 32,
            current_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
            authentication: ServerAuthentication::Unsecure,
        };
        let transport = NetcodeServerTransport::new(server_config, socket).unwrap();
        let server = RenetServer::new(shared::world::connection_config());

        app.insert_resource(server);
        app.insert_resource(transport);
        app.insert_resource(self.config.clone());

        app.add_plugins((
            server::plugin,
            messages::plugin,
            input::plugin,
            client::plugin,
            entity::plugin,
            ownership::plugin,
            physics::plugin,
            replication::plugin,
            commands::plugin,
        ));
    }
}
