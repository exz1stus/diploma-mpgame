use std::{
    collections::VecDeque,
    net::UdpSocket,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use bevy_renet::{
    RenetServer, RenetServerEvent, RenetServerPlugin,
    netcode::{NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerConfig},
    renet::{ConnectionConfig, DefaultChannel, ServerEvent},
};
use serde::{Deserialize, Serialize};
use shared::client_messages::{ClientMessage, ClientMessages};
use shared::server_messages::{ServerMessage, ServerMessages};

use crate::config::AppConfig;

pub struct GameServer {
    app: App,
}

impl GameServer {
    pub fn new(config: &AppConfig) -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
            RenetServerPlugin,
            NetcodeServerPlugin,
        ));

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

        app.insert_resource(server);
        app.insert_resource(transport);

        app.insert_resource(TickCounter(0));
        app.insert_resource(MessageTimer(Timer::from_seconds(
            1.0 / config.tickrate as f32,
            TimerMode::Repeating,
        )));

        app.init_resource::<MessageBuffer>();
        app.init_resource::<OutgoingDelay>();

        app.add_observer(on_client_event);
        app.add_systems(
            Update,
            (receive_messages, send_messages, send_delayed_replies).chain(),
        );

        Self { app }
    }

    pub fn run(&mut self) {
        self.app.run();
    }
}

fn on_client_event(
    event: On<RenetServerEvent>,
    tick_counter: Res<TickCounter>,
    server: ResMut<RenetServer>,
) {
    match **event {
        ServerEvent::ClientConnected { client_id } => {
            on_client_connected(server, client_id, tick_counter.0)
        }
        ServerEvent::ClientDisconnected { client_id, reason } => {
            println!("Client {client_id} disconnected: {reason}")
        }
    }
}

fn on_client_connected(mut server: ResMut<RenetServer>, client_id: u64, tick: u64) {
    println!("Client {client_id} connected");
    let initiation_message = ServerMessage {
        sequence: 0,
        tick,
        payload: ServerMessages::InitiateClientState { client_id, tick },
    };
    let bytes: Vec<u8> = postcard::to_allocvec(&initiation_message).unwrap();
    server.send_message(client_id, DefaultChannel::ReliableOrdered, bytes);
}

#[derive(Resource, Default)]
pub struct MessageBuffer {
    pub pending: VecDeque<BufferedClientMessege>,
}

#[derive(Resource)]
struct TickCounter(u64);

#[derive(Resource)]
struct MessageTimer(Timer);

#[derive(Resource, Default)]
struct OutgoingDelay {
    pending: VecDeque<DelayedReply>,
}
struct DelayedReply {
    send_at: Instant,
    client_id: u64,
    bytes: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BufferedClientMessege {
    pub client_id: u64,
    pub recieved_tick: u64,
    pub message: ClientMessage,
}

fn receive_messages(
    tick_counter: Res<TickCounter>,
    mut server: ResMut<RenetServer>,
    mut buffer: ResMut<MessageBuffer>,
) {
    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, DefaultChannel::ReliableOrdered)
        {
            let message: ClientMessage =
                postcard::from_bytes(&message).expect("failed to deserialize client message");
            buffer.pending.push_back(BufferedClientMessege {
                client_id,
                recieved_tick: tick_counter.0,
                message,
            });
            println!("recieved message from client, pushed to buffer");
        }
    }
}

fn send_messages(
    time: Res<Time>,
    mut buffer: ResMut<MessageBuffer>,
    mut tick_counter: ResMut<TickCounter>,
    mut timer: ResMut<MessageTimer>,
    mut delay: ResMut<OutgoingDelay>,
) {
    timer.0.tick(time.delta());

    if timer.0.just_finished() {
        tick_counter.0 += 1;
        let tick_number = tick_counter.0;
        while let Some(buffered) = buffer.pending.pop_front() {
            let ClientMessages::Ping {
                server_delay_ms, ..
            } = &buffered.message.payload;
            let msg = ServerMessage {
                sequence: buffered.message.sequence,
                tick: tick_number,
                payload: ServerMessages::Pong(String::from("pong")),
            };

            let send_at = Instant::now() + Duration::from_millis(*server_delay_ms);
            let insert_at = delay
                .pending
                .iter()
                .position(|reply| reply.send_at > send_at)
                .unwrap_or(delay.pending.len());
            delay.pending.insert(
                insert_at,
                DelayedReply {
                    send_at,
                    client_id: buffered.client_id,
                    bytes: postcard::to_allocvec(&msg).unwrap(),
                },
            );
        }
    }
}
fn send_delayed_replies(mut delay: ResMut<OutgoingDelay>, mut server: ResMut<RenetServer>) {
    let now = Instant::now();

    while delay
        .pending
        .front()
        .is_some_and(|reply| reply.send_at <= now)
    {
        let reply = delay.pending.pop_front().unwrap();
        server.send_message(
            reply.client_id,
            DefaultChannel::ReliableOrdered,
            reply.bytes,
        );
    }
}
