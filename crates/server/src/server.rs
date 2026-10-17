use std::{
    collections::VecDeque,
    net::UdpSocket,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use bevy_renet::{
    RenetServer, RenetServerEvent, RenetServerPlugin,
    netcode::{NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerConfig},
    renet::{ConnectionConfig, DefaultChannel, ServerEvent},
};
use shared::messages::{
    BufferedClientMessege, ClientInitialization, ClientMessage, ServerMessage,
    ServerMessages::{self, InitiateClientState},
};

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

        app.add_observer(on_client_event);
        app.add_systems(Update, (receive_messages, send_messages).chain());

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
        payload: InitiateClientState(ClientInitialization { client_id, tick }),
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
    mut server: ResMut<RenetServer>,
) {
    timer.0.tick(time.delta());

    if timer.0.just_finished() {
        tick_counter.0 += 1;
        let tick_number = tick_counter.0;
        while let Some(buffered) = buffer.pending.pop_front() {
            let msg = ServerMessage {
                sequence: buffered.message.sequence,
                tick: tick_number,
                payload: ServerMessages::Pong(String::from("pong")),
            };
            let bytes: Vec<u8> = postcard::to_allocvec(&msg).unwrap();
            server.send_message(buffered.client_id, DefaultChannel::ReliableOrdered, bytes);
        }
    }
}
