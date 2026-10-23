use bevy::prelude::App;
use bevy_ecs::{observer::On, system::Res, system::ResMut};
use bevy_renet::{
    RenetServer, RenetServerEvent,
    renet::{DefaultChannel, ServerEvent},
};
use shared::server_messages::{ServerMessage, ServerMessages};

use crate::messages::TickCounter;

pub(crate) fn configure(app: &mut App) {
    app.add_observer(on_client_event);
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
