use std::{
    net::UdpSocket,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use bevy_renet::{
    RenetClient, RenetClientPlugin, client_connected,
    netcode::{ClientAuthentication, NetcodeClientPlugin, NetcodeClientTransport},
    renet::{ConnectionConfig, DefaultChannel},
};

use crate::config::ClientConfig;

use shared::messages::*;

pub struct GameClient {
    app: App,
}
impl GameClient {
    pub fn new(config: &ClientConfig) -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
            RenetClientPlugin,
            NetcodeClientPlugin,
        ));

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

        app.add_systems(
            Update,
            (send_messages, receive_messages)
                .chain()
                .run_if(client_connected),
        );

        Self { app }
    }

    pub fn run(&mut self) {
        self.app.run();
    }
}

fn send_messages(mut client: ResMut<RenetClient>) {
    let message = ClientMessages::Ping(String::from("client ping"));
    let bytes: Vec<u8> = postcard::to_allocvec(&message).unwrap();
    client.send_message(DefaultChannel::ReliableOrdered, bytes);
}

fn receive_messages(mut client: ResMut<RenetClient>) {
    while let Some(message) = client.receive_message(DefaultChannel::ReliableOrdered) {
        let message: ServerMessages = postcard::from_bytes(&message).unwrap();
        match message {
            ServerMessages::Pong(text_message) => {
                println!("Client: {}", text_message.payload)
            } // _ => println!("recieved unsupported message type"),
        }
    }
}
