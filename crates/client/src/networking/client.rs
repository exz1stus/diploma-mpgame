pub use crate::networking::network::NetworkSimulation;
use crate::networking::physics::{
    self, initialize_physics_body, log_prediction_status, run_prediction, sync_remote_colliders,
};
use crate::networking::reconciliation::{
    ReconciliationStatus, apply_reconciliation, compare_authoritative_snapshots,
    log_reconciliation_status,
};
use crate::networking::simulation_clock::ClientSimulationClock;
pub(crate) use crate::networking::state::Owned;

use crate::game::rendering::{add_circle_visuals, render_remote_circles, setup_circle_assets};
use crate::networking::network::{
    collect_incoming, release_incoming, release_outgoing, reset_disconnected_client, send_messages,
};
use crate::networking::prediction::{PredictionSettings, initialize_prediction};
use crate::networking::state::{ClientState, RemoteWorld};
use crate::networking::time_sync::TimeSyncEstimates;
use crate::{
    config::ClientConfig,
    networking::diagnostics::{NetworkDiagnostics, sample_network},
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
        app.init_resource::<PredictionSettings>();
        app.init_resource::<RemoteWorld>();
        app.init_resource::<ClientSimulationClock>();
        app.init_resource::<TimeSyncEstimates>();
        app.init_resource::<ReconciliationStatus>();
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        physics::configure(&mut app);

        app.add_systems(Startup, setup_circle_assets);

        app.add_systems(
            Update,
            (
                collect_incoming,
                release_incoming,
                add_circle_visuals,
                initialize_prediction,
                initialize_physics_body,
                sync_remote_colliders,
                compare_authoritative_snapshots,
                apply_reconciliation,
                log_reconciliation_status,
                run_prediction,
                log_prediction_status,
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
