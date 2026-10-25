use bevy::prelude::App;
use bevy_ecs::{
    observer::On,
    query::With,
    system::{ParamSet, Query, Res, ResMut},
    world::EntityRef,
};
use bevy_renet::{
    RenetServer, RenetServerEvent,
    renet::{ClientId, DefaultChannel, ServerEvent},
};
use shared::server_messages::{ServerMessage, ServerMessages};
use shared::{replication::ReplicationRegistry, snapshots::NetworkIdentity};

use crate::{config::AppConfig, messages::TickCounter, world::build_world_snapshot};

pub fn configure(app: &mut App) {
    app.add_observer(on_client_event);
}

fn on_client_event(
    event: On<RenetServerEvent>,
    tick_counter: Res<TickCounter>,
    config: Res<AppConfig>,
    mut params: ParamSet<(ResMut<RenetServer>, Query<EntityRef, With<NetworkIdentity>>)>,
    registry: Res<ReplicationRegistry>,
) {
    match **event {
        ServerEvent::ClientConnected { client_id } => {
            let world = build_world_snapshot(config.tickrate, &params.p1(), &registry);
            on_client_connected(params.p0(), client_id, tick_counter.0, world)
        }
        ServerEvent::ClientDisconnected { client_id, reason } => {
            println!("Client {client_id} disconnected: {reason}")
        }
    }
}

fn on_client_connected(
    mut server: ResMut<RenetServer>,
    client_id: ClientId,
    tick: u64,
    world: shared::snapshots::WorldSnapshot,
) {
    println!("Client {client_id} connected");

    let initiation_message = ServerMessage {
        sequence: 0,
        tick,
        payload: ServerMessages::InitiateClientState { client_id, world },
    };

    let bytes: Vec<u8> = postcard::to_allocvec(&initiation_message).unwrap();
    server.send_message(client_id, DefaultChannel::ReliableOrdered, bytes);
}
