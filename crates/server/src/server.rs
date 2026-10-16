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
use shared::messages::{ClientMessages, RecievedClientMessage, ServerMessage, ServerMessages};

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
        app.add_systems(Update, (send_messages, receive_messages).chain());

        Self { app }
    }

    pub fn run(&mut self) {
        self.app.run();
    }
}

fn on_client_event(event: On<RenetServerEvent>) {
    match **event {
        ServerEvent::ClientConnected { client_id } => println!("Client {client_id} connected"),
        ServerEvent::ClientDisconnected { client_id, reason } => {
            println!("Client {client_id} disconnected: {reason}")
        }
    }
}

#[derive(Resource, Default)]
pub struct MessageBuffer {
    pub pending: VecDeque<(u64, RecievedClientMessage)>,
}

#[derive(Resource)]
struct TickCounter(u64);

#[derive(Resource)]
struct MessageTimer(Timer);

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
        while let Some((_received_tick, message)) = buffer.pending.pop_front() {
            let msg = ServerMessages::Pong(ServerMessage {
                sequence: tick_number,
                payload: String::from("pong"),
            });
            let bytes: Vec<u8> = postcard::to_allocvec(&msg).unwrap();
            server.send_message(message.client_id, DefaultChannel::ReliableOrdered, bytes);
        }
    }
}

fn receive_messages(
    tick_counter: Res<TickCounter>,
    mut server: ResMut<RenetServer>,
    mut buffer: ResMut<MessageBuffer>,
) {
    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, DefaultChannel::ReliableOrdered)
        {
            let message: ClientMessages = postcard::from_bytes(&message).unwrap();
            buffer.pending.push_back((
                tick_counter.0,
                RecievedClientMessage {
                    client_id,
                    payload: message,
                },
            ));
            println!("recieved message from client, pushed to buffer");
        }
    }
}
