use std::{
    collections::VecDeque,
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_renet::{
    RenetClient, RenetClientPlugin, RenetSend, client_connected,
    netcode::{ClientAuthentication, NetcodeClientPlugin, NetcodeClientTransport},
    renet::{ConnectionConfig, DefaultChannel},
};

use crate::{
    config::ClientConfig,
    diagnostics::{NetworkDiagnostics, sample_network},
    netgraph::network_diagnostics_ui,
};

use shared::client_messages::{ClientMessage, ClientMessages};
use shared::server_messages::{ServerMessage, ServerMessages};

pub struct GameClient {
    app: App,
}

impl GameClient {
    pub fn new(config: &ClientConfig) -> Self {
        let mut app = App::new();
        app.add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            RenetClientPlugin,
            NetcodeClientPlugin,
        ));

        app.add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        });
        app.add_systems(EguiPrimaryContextPass, network_diagnostics_ui);

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

        app.add_systems(
            Update,
            (
                receive_messages,
                send_messages,
                release_outgoing,
                print_net_stats,
            )
                .chain()
                .run_if(client_connected),
        );

        app.add_systems(PostUpdate, (sample_network).after(RenetSend));

        Self { app }
    }

    pub fn run(&mut self) {
        self.app.run();
    }
}

fn print_net_stats(diagnostics: Res<NetworkDiagnostics>) {
    println!("Ping: {}", diagnostics.ping_ms)
}

#[derive(Resource, Default)]
struct ClientState {
    id: u64,
    sequence: u64,
    last_tick: u64,
    initiated: bool,
}

#[derive(Resource)]
pub struct NetworkSimulation {
    pub outgoing_delay_ms: u64,
    pub server_delay_ms: u64,
    outgoing: VecDeque<QueuedMessage>,
}

impl Default for NetworkSimulation {
    fn default() -> Self {
        Self {
            outgoing_delay_ms: 0,
            server_delay_ms: 0,
            outgoing: VecDeque::new(),
        }
    }
}

struct QueuedMessage {
    release_at_s: f64,
    message: ClientMessage,
}

fn send_messages(
    mut simulation: ResMut<NetworkSimulation>,
    mut client_state: ResMut<ClientState>,
    time: Res<Time<Real>>,
) {
    if !client_state.initiated {
        return;
    }

    client_state.sequence += 1;
    let message = ClientMessage {
        sequence: client_state.sequence,
        payload: ClientMessages::Ping {
            text: String::from("client ping"),
            server_delay_ms: simulation.server_delay_ms,
        },
    };

    let release_at_s = time.elapsed_secs_f64() + simulation.outgoing_delay_ms as f64 / 1_000.0;
    let insert_at = simulation
        .outgoing
        .iter()
        .position(|queued| queued.release_at_s > release_at_s)
        .unwrap_or(simulation.outgoing.len());
    simulation.outgoing.insert(
        insert_at,
        QueuedMessage {
            release_at_s,
            message,
        },
    );
}
fn release_outgoing(
    mut simulation: ResMut<NetworkSimulation>,
    mut client: ResMut<RenetClient>,
    mut stats: ResMut<NetworkDiagnostics>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs_f64();

    while simulation
        .outgoing
        .front()
        .is_some_and(|queued| queued.release_at_s <= now)
    {
        let queued = simulation.outgoing.pop_front().unwrap();
        let bytes = postcard::to_allocvec(&queued.message).unwrap();
        stats.add_sent(queued.message, now);
        client.send_message(DefaultChannel::ReliableOrdered, bytes);
    }
}

fn receive_messages(
    mut client: ResMut<RenetClient>,
    mut stats: ResMut<NetworkDiagnostics>,
    mut client_state: ResMut<ClientState>,
    time: Res<Time<Real>>,
) {
    while let Some(bytes) = client.receive_message(DefaultChannel::ReliableOrdered) {
        let message: ServerMessage = postcard::from_bytes(&bytes).unwrap();
        client_state.last_tick = message.tick;
        match &message.payload {
            ServerMessages::InitiateClientState { client_id, tick } => {
                *client_state = ClientState {
                    id: *client_id,
                    sequence: 0,
                    last_tick: *tick,
                    initiated: true,
                };
                println!(
                    "Client {} initialized at tick {}",
                    client_state.id, client_state.last_tick
                );
            }
            ServerMessages::Pong(text_message) => {
                println!("Client: {}", text_message);
            }
        }
        stats.add_received(message, time.elapsed_secs_f64());
    }
}
