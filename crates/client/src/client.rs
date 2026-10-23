use std::{
    collections::VecDeque,
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::prelude::*;
use bevy_renet::{
    RenetClient, RenetSend, client_connected,
    netcode::{ClientAuthentication, NetcodeClientTransport},
    renet::{ConnectionConfig, DefaultChannel},
};

use crate::{
    config::ClientConfig,
    diagnostics::{NetworkDiagnostics, sample_network},
};

use shared::server_messages::{ServerMessage, ServerMessages};
use shared::{
    client_messages::{ClientMessage, ClientMessages},
    components::Position,
    protocol::NetworkIdentity,
    replication::ReplicatedComponent,
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

#[derive(Resource, Default)]
pub struct NetworkSimulation {
    pub outgoing_delay_ms: u64,
    pub server_delay_ms: u64,
    outgoing: VecDeque<QueuedMessage>,
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
    // if !client_state.initiated {
    //     return;
    // }

    // client_state.sequence += 1;
    // let message = ClientMessage {
    //     sequence: client_state.sequence,
    //     payload: ClientMessages::Ping {
    //         text: String::from("client ping"),
    //         server_delay_ms: simulation.server_delay_ms,
    //     },
    // };

    // let release_at_s = time.elapsed_secs_f64() + simulation.outgoing_delay_ms as f64 / 1_000.0;
    // let insert_at = simulation
    //     .outgoing
    //     .iter()
    //     .position(|queued| queued.release_at_s > release_at_s)
    //     .unwrap_or(simulation.outgoing.len());
    // simulation.outgoing.insert(
    //     insert_at,
    //     QueuedMessage {
    //         release_at_s,
    //         message,
    //     },
    // );
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
    mut positions: Query<(&NetworkIdentity, &mut Position, &mut Transform)>,
    time: Res<Time<Real>>,
) {
    for channel in [
        u8::from(DefaultChannel::ReliableOrdered),
        u8::from(DefaultChannel::ReliableUnordered),
        u8::from(DefaultChannel::Unreliable),
    ] {
        while let Some(bytes) = client.receive_message(channel) {
            let message: ServerMessage = postcard::from_bytes(&bytes).unwrap();
            client_state.last_tick = message.tick;
            match &message.payload {
                ServerMessages::InitiateClientState {
                    client_id, world, ..
                } => {
                    *client_state = ClientState {
                        id: *client_id,
                        sequence: 0,
                        last_tick: message.tick,
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
                // Test entity: apply position updates for network entity 1.
                ServerMessages::EntityUpdate(snapshot) => {
                    if snapshot.id == 1
                        && let Some(component) = snapshot
                            .components
                            .iter()
                            .find(|component| component.component_id == Position::NETWORK_ID)
                    {
                        match postcard::from_bytes::<Position>(&component.data) {
                            Ok(position) => {
                                if let Some((_, mut current_position, mut transform)) = positions
                                    .iter_mut()
                                    .find(|(identity, _, _)| identity.id == snapshot.id)
                                {
                                    println!("{:#?}", position);
                                    transform.translation.x = position.x;
                                    transform.translation.y = position.y;
                                    *current_position = position;
                                }
                            }
                            Err(_) => eprintln!("Failed to deserialize Position"),
                        }
                    }
                }
                ServerMessages::EntitySpawn(_) | ServerMessages::EntityDespawn(_) => {}
            }
            stats.add_received(message, time.elapsed_secs_f64());
        }
    }
}
