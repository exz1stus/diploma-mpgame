use std::{
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::app::{App, Plugin};
use bevy_renet::{
    RenetServer,
    netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig},
    renet::ConnectionConfig,
};
use shared::replication::{BufferedComponentChanges, NetworkEntityMap};

use crate::config::AppConfig;

pub(crate) mod client;
pub(crate) mod entity;
mod handlers;
mod input;
mod messages;
mod ownership;
mod physics;
mod replication;
pub(crate) mod server;
mod world;

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
        let server = RenetServer::new(ConnectionConfig::default());

        let tickrate = self.config.tickrate;

        app.insert_resource(server);
        app.insert_resource(transport);
        app.insert_resource(self.config.clone());

        app.init_resource::<BufferedComponentChanges>();
        app.init_resource::<NetworkEntityMap>();

        server::configure(app, tickrate as f64);
        messages::configure(app);
        input::configure(app);
        client::configure(app);
        entity::configure(app);
        ownership::configure(app);
        physics::configure(app);
        replication::configure(app);
    }
}
