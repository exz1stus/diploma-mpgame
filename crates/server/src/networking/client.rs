use bevy::app::App;
use bevy_ecs::{
    entity::Entity,
    event::Event,
    observer::On,
    query::{With, Without},
    resource::{IsResource, Resource},
    system::{Commands, ParamSet, Query, Res, ResMut},
    world::EntityRef,
};
use bevy_renet::{
    RenetServer, RenetServerEvent,
    renet::{
        ClientId, DefaultChannel, DisconnectReason,
        ServerEvent::{self},
    },
};
use shared::server_messages::{ServerMessage, ServerMessages};
use shared::{replication::ReplicationRegistry, snapshots::NetworkIdentity};

use crate::{config::AppConfig, networking::server::TickCounter};

use super::{messages::BufferedClientMessage, world::build_world_snapshot};

use std::collections::{HashMap, HashSet};

#[derive(Event)]
pub(crate) struct ClientIntiated {
    pub client_id: ClientId,
}

#[derive(Event)]
pub(crate) struct ClientLeft {
    pub client_id: ClientId,
    pub state: ClientState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientSyncPhase {
    Connected,
    Ready,
}

//TODO: add spatial network culling
pub(crate) struct ClientState {
    pub(crate) owned_entities: HashSet<Entity>,
    phase: ClientSyncPhase,
    // visible_entities: HashSet<NetworkEntityId>,
}

#[derive(Resource, Default)]
pub(crate) struct ReplicationClients {
    pub clients: HashMap<ClientId, ClientState>,
}

impl ReplicationClients {
    pub(crate) fn ready_clients(&self) -> impl Iterator<Item = ClientId> + '_ {
        self.clients
            .iter()
            .filter(|(_, state)| state.phase == ClientSyncPhase::Ready)
            .map(|(&id, _)| id)
    }
}

pub(crate) fn configure(app: &mut App) {
    app.init_resource::<ReplicationClients>();
    app.add_observer(on_client_event);
}

fn on_client_event(
    event: On<RenetServerEvent>,
    tick_counter: Res<TickCounter>,
    config: Res<AppConfig>,
    mut commands: Commands,
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
            on_client_disconnected(&mut commands, &mut clients, client_id, reason);
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
        ClientState {
            phase: ClientSyncPhase::Connected,
            owned_entities: HashSet::new(),
            // visible_entities: HashSet::new(),
        },
    );

    let bytes: Vec<u8> = postcard::to_allocvec(&initiation_message).unwrap();
    server.send_message(client_id, DefaultChannel::ReliableOrdered, bytes);

    println!("Client {client_id} connected");
}

fn on_client_disconnected(
    commands: &mut Commands,
    clients: &mut ReplicationClients,
    client_id: u64,
    reason: DisconnectReason,
) {
    let Some(client_state) = clients.clients.remove(&client_id) else {
        return;
    };

    commands.trigger(ClientLeft {
        client_id,
        state: client_state,
    });

    println!("Client {client_id} disconnected: {reason}");
}

pub(crate) fn on_client_initiated(
    commands: &mut Commands,
    req: &BufferedClientMessage,
    clients: &mut ReplicationClients,
) {
    let client_id = req.client_id;

    let Some(client) = clients.clients.get_mut(&client_id) else {
        eprint!("unregistered client tried to intialize");
        return;
    };

    client.phase = ClientSyncPhase::Ready;

    commands.trigger(ClientIntiated { client_id });

    println!("Client {client_id} succesfully intiated");
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::components::{DisconnectPolicy, Ownership, Player};

    #[test]
    fn disconnect_despawns_owned_player() {
        let mut app = App::new();
        app.init_resource::<ReplicationClients>();
        super::super::ownership::configure(&mut app);
        app.world_mut()
            .resource_mut::<ReplicationClients>()
            .clients
            .insert(
                7,
                ClientState {
                    owned_entities: HashSet::new(),
                    phase: ClientSyncPhase::Ready,
                },
            );
        let player = app
            .world_mut()
            .spawn((
                Player,
                Ownership {
                    client_id: 7,
                    disconnect_policy: DisconnectPolicy::Despawn,
                },
            ))
            .id();
        assert!(
            app.world().resource::<ReplicationClients>().clients[&7]
                .owned_entities
                .contains(&player)
        );

        app.add_systems(
            bevy::app::Update,
            |mut commands: Commands, mut clients: ResMut<ReplicationClients>| {
                let state = clients.clients.remove(&7).unwrap();
                commands.trigger(ClientLeft {
                    client_id: 7,
                    state,
                });
            },
        );
        app.update();

        assert!(app.world().get_entity(player).is_err());
    }
}
