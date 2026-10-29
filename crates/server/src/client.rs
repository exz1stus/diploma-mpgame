use bevy::prelude::App;
use bevy_ecs::{
    observer::On,
    query::{With, Without},
    resource::{IsResource, Resource},
    system::{ParamSet, Query, Res, ResMut},
    world::EntityRef,
};
use bevy_renet::{
    RenetServer, RenetServerEvent,
    renet::{ClientId, DefaultChannel, DisconnectReason, ServerEvent},
};
use shared::{replication::ReplicationRegistry, snapshots::NetworkIdentity};
use shared::{
    server_messages::{ServerMessage, ServerMessages},
    snapshots::NetworkEntityId,
};

use crate::{
    config::AppConfig,
    messages::{BufferedClientMessage, TickCounter},
    world::build_world_snapshot,
};

pub(crate) fn configure(app: &mut App) {
    app.init_resource::<ReplicationClients>();
    app.add_observer(on_client_event);
}

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientSyncPhase {
    Connected,
    Ready,
}
//TODO: add spatial network culling
struct ClientReplicationState {
    phase: ClientSyncPhase,
    // visible_entities: HashSet<NetworkEntityId>,
}

#[derive(Resource, Default)]
pub(crate) struct ReplicationClients {
    clients: HashMap<ClientId, ClientReplicationState>,
}

impl ReplicationClients {
    pub(crate) fn ready_clients(&self) -> impl Iterator<Item = ClientId> + '_ {
        self.clients
            .iter()
            .filter(|(_, state)| state.phase == ClientSyncPhase::Ready)
            .map(|(&id, _)| id)
    }
}

fn on_client_event(
    event: On<RenetServerEvent>,
    tick_counter: Res<TickCounter>,
    config: Res<AppConfig>,
    mut params: ParamSet<(
        ResMut<RenetServer>,
        Query<EntityRef, (With<NetworkIdentity>, Without<IsResource>)>,
    )>,
    mut clients: ResMut<ReplicationClients>,
    registry: Res<ReplicationRegistry>,
) {
    match **event {
        ServerEvent::ClientConnected { client_id } => {
            let world = build_world_snapshot(config.tickrate, &params.p1(), &registry);
            on_client_connected(
                &mut params.p0(),
                &mut clients,
                client_id,
                tick_counter.0,
                world,
            )
        }
        ServerEvent::ClientDisconnected { client_id, reason } => {
            on_client_disconnected(&mut clients, client_id, reason);
        }
    }
}

fn on_client_connected(
    server: &mut RenetServer,
    clients: &mut ReplicationClients,
    client_id: ClientId,
    tick: u64,
    world: shared::snapshots::WorldSnapshot,
) {
    let initiation_message = ServerMessage {
        sequence: None,
        tick,
        payload: ServerMessages::InitiateClientState { client_id, world },
    };

    clients.clients.insert(
        client_id,
        ClientReplicationState {
            phase: ClientSyncPhase::Connected,
            // visible_entities: HashSet::new(),
        },
    );

    let bytes: Vec<u8> = postcard::to_allocvec(&initiation_message).unwrap();
    server.send_message(client_id, DefaultChannel::ReliableOrdered, bytes);

    println!("Client {client_id} connected");
}

fn on_client_disconnected(
    clients: &mut ReplicationClients,
    client_id: u64,
    reason: DisconnectReason,
) {
    clients.clients.remove(&client_id);

    println!("Client {client_id} disconnected: {reason}");
}

pub(crate) fn on_client_initiated(req: &BufferedClientMessage, clients: &mut ReplicationClients) {
    let client_id = req.client_id;

    let Some(client) = clients.clients.get_mut(&client_id) else {
        eprint!("unregistered client tried to intialize");
        return;
    };

    client.phase = ClientSyncPhase::Ready;

    println!("Client {client_id} succesfully intiated");
}
