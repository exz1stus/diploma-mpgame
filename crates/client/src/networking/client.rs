use crate::game::camera::follow_player_camera;
use crate::game::commands::{
    PendingCommands, PredictedVisuals, ShootAimState, draw_shoot_aim, send_shoot_command,
    update_shoot_aim,
};
use crate::game::menu::toggle_menu;
pub use crate::networking::network::NetworkSimulation;
use crate::networking::physics::{
    self, initialize_physics_body, log_prediction_status, run_prediction,
};
use crate::networking::reconciliation::{
    ReconciliationStatus, apply_reconciliation, compare_authoritative_snapshots,
    log_reconciliation_status,
};
use crate::networking::simulation_clock::ClientSimulationClock;
pub(crate) use crate::networking::state::Owned;

use crate::game::rendering::{
    add_circle_visuals, render_player_visuals, render_world_chunks, render_world_objects,
    setup_circle_assets,
};
use crate::networking::network::{
    collect_incoming, release_incoming, release_outgoing, reset_disconnected_client, send_messages,
};
use crate::networking::prediction::{
    PredictionSettings, initialize_prediction, smooth_prediction_visual,
};
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

        let client = RenetClient::new(shared::world::connection_config());

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

        app.init_resource::<PendingCommands>();
        app.init_resource::<PredictedVisuals>();
        app.init_resource::<ShootAimState>();
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
                compare_authoritative_snapshots,
                apply_reconciliation,
                log_reconciliation_status,
                run_prediction,
                smooth_prediction_visual,
                render_player_visuals,
                update_shoot_aim,
                draw_shoot_aim,
                log_prediction_status,
                send_messages,
                send_shoot_command,
                release_outgoing,
            )
                .chain()
                .after(toggle_menu)
                .run_if(client_connected),
        );

        app.add_systems(Update, reset_disconnected_client);
        app.add_systems(Update, (render_world_chunks, render_world_objects));
        app.add_systems(Update, follow_player_camera.after(render_player_visuals));
        app.add_systems(PostUpdate, sample_network.after(RenetSend));

        Self { app }
    }

    pub fn run(&mut self) {
        self.app.run();
    }
}
