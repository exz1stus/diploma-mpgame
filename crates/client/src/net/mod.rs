//! Netcode: the session (connect, host, leave), packets, replication, prediction and
//! reconciliation, and the world stream. It produces state only; nothing here draws.

pub(crate) mod diagnostics;
pub(crate) mod interpolation;
pub(crate) mod network;
pub(crate) mod physics;
pub(crate) mod prediction;
pub(crate) mod prediction_history;
pub(crate) mod reconciliation;
pub(crate) mod replication;
pub(crate) mod replication_handlers;
pub(crate) mod session;
pub(crate) mod simulation_clock;
pub(crate) mod state;
pub(crate) mod time_sync;
pub(crate) mod world_streaming;

use bevy::prelude::*;
use bevy_renet::{
    RenetClient, RenetClientPlugin, RenetSend, client_connected, netcode::NetcodeClientPlugin,
};
use shared::world::connection_config;

use crate::{config::ClientConfig, schedule::ClientSet};
use diagnostics::{NetworkDiagnostics, sample_network};
use network::{
    NetworkSimulation, collect_incoming, release_incoming, release_outgoing,
    reset_disconnected_client, send_messages,
};
use physics::{initialize_physics_body, log_prediction_status, run_prediction};
use prediction::{PredictionSettings, initialize_prediction, smooth_prediction_visual};
use reconciliation::{
    ReconciliationStatus, apply_reconciliation, compare_authoritative_snapshots,
    log_reconciliation_status,
};
use session::{drive_session, install_session};
use simulation_clock::ClientSimulationClock;
use state::{ClientState, RemoteWorld};
use time_sync::TimeSyncEstimates;
use world_streaming::WorldStreamingPlugin;

pub struct NetPlugin {
    pub config: ClientConfig,
}

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((RenetClientPlugin, NetcodeClientPlugin, WorldStreamingPlugin));
        app.insert_resource(disconnected_client());
        install_session(app, &self.config);
        init_resources(app);
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        physics::configure(app);
        add_systems(app);
    }
}

/// The client starts idle; the session connects it when the menu asks to.
fn disconnected_client() -> RenetClient {
    let mut client = RenetClient::new(connection_config());
    client.disconnect();
    client
}

fn init_resources(app: &mut App) {
    app.init_resource::<NetworkDiagnostics>()
        .init_resource::<ClientState>()
        .init_resource::<NetworkSimulation>()
        .init_resource::<PredictionSettings>()
        .init_resource::<RemoteWorld>()
        .init_resource::<ClientSimulationClock>()
        .init_resource::<TimeSyncEstimates>()
        .init_resource::<ReconciliationStatus>();
}

fn add_systems(app: &mut App) {
    app.add_systems(
        Update,
        (drive_session, reset_disconnected_client)
            .chain()
            .in_set(ClientSet::Session),
    );
    app.add_systems(
        Update,
        (
            (collect_incoming, release_incoming)
                .chain()
                .in_set(ClientSet::Receive),
            (
                initialize_prediction,
                initialize_physics_body,
                compare_authoritative_snapshots,
                apply_reconciliation,
                log_reconciliation_status,
                run_prediction,
                smooth_prediction_visual,
                log_prediction_status,
            )
                .chain()
                .in_set(ClientSet::Simulate),
            send_messages.in_set(ClientSet::Send),
            release_outgoing.in_set(ClientSet::Flush),
        )
            .run_if(client_connected),
    );
    app.add_systems(PostUpdate, sample_network.after(RenetSend));
}
