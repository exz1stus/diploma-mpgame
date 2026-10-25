use std::{
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::prelude::App;
use bevy_renet::{
    RenetServer,
    netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig},
    renet::ConnectionConfig,
};
use shared::replication::{BufferedComponentChanges, NetworkEntityMap};

use crate::{client_events, config::AppConfig, entity, messages, replication};

pub struct GameServer {
    app: App,
}

impl GameServer {
    pub fn new(mut app: App, config: AppConfig) -> Self {
        let socket = UdpSocket::bind(config.server_addr).unwrap();
        let server_config = ServerConfig {
            protocol_id: config.protocol_id,
            public_addresses: vec![config.server_addr],
            max_clients: 32,
            current_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
            authentication: ServerAuthentication::Unsecure,
        };
        let transport = NetcodeServerTransport::new(server_config, socket).unwrap();
        let server = RenetServer::new(ConnectionConfig::default());

        let tickrate = config.tickrate;

        app.insert_resource(server);
        app.insert_resource(transport);
        app.insert_resource(config);

        app.init_resource::<BufferedComponentChanges>();
        app.init_resource::<NetworkEntityMap>();

        messages::configure(&mut app, tickrate);
        client_events::configure(&mut app);
        entity::configure(&mut app);
        replication::configure(&mut app);

        Self { app }
    }

    pub fn run(&mut self) {
        self.app.run();
    }
}
