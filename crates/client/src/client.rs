//! Client startup and system execution order. Follow the modules below for details.

pub use crate::network::NetworkSimulation;
pub(crate) use crate::state::Owned;

use crate::network::{
    collect_incoming, release_incoming, release_outgoing, reset_disconnected_client, send_messages,
};
use crate::rendering::{add_circle_visuals, render_remote_circles, setup_circle_assets};
use crate::state::{ClientState, RemoteWorld};
use crate::{
    config::ClientConfig,
    diagnostics::{NetworkDiagnostics, sample_network},
};
use bevy::prelude::*;
use bevy_renet::{
    RenetClient, RenetSend, client_connected,
    netcode::{ClientAuthentication, NetcodeClientTransport},
    renet::ConnectionConfig,
};
use std::{
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct GameClient {
    app: App,
}

impl GameClient {
    pub fn new(mut app: App, config: &ClientConfig) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();

        let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        let authentication = ClientAuthentication::Unsecure {
            server_addr: config.server_addr,
            protocol_id: config.protocol_id,
            client_id: current_time.as_micros() as u64,
            user_data: None,
        };

        let transport = NetcodeClientTransport::new(current_time, authentication, socket).unwrap();

        let client = RenetClient::new(ConnectionConfig::default());

        app.insert_resource(client);
        app.insert_resource(transport);

        app.init_resource::<NetworkDiagnostics>();
        app.init_resource::<ClientState>();
        app.init_resource::<NetworkSimulation>();
        app.init_resource::<RemoteWorld>();

        app.add_systems(Startup, setup_circle_assets);

        app.add_systems(
            Update,
            (
                collect_incoming,
                release_incoming,
                add_circle_visuals,
                render_remote_circles,
                send_messages,
                release_outgoing,
            )
                .chain()
                .run_if(client_connected),
        );

        app.add_systems(Update, reset_disconnected_client);

        app.add_systems(PostUpdate, sample_network.after(RenetSend));

        Self { app }
    }

    pub fn run(&mut self) {
        self.app.run();
    }
}
