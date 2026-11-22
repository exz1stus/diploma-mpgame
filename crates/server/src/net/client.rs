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

use crate::{config::AppConfig, net::server::TickCounter};

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

impl ClientState {
    #[cfg(test)]
    pub(crate) fn ready() -> Self {
        Self {
            owned_entities: HashSet::new(),
            phase: ClientSyncPhase::Ready,
        }
    }

    pub fn has_intiated(&self) -> bool {
        self.phase == ClientSyncPhase::Ready
    }
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

pub(crate) fn plugin(app: &mut App) {
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
    client_state: &mut ClientState,
) {
    let client_id = req.client_id;

    client_state.phase = ClientSyncPhase::Ready;

    commands.trigger(ClientIntiated { client_id });

    println!("Client {client_id} succesfully intiated");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        game::inventory::{InventoryService, retire_entity},
        net::{entity, messages::MessageBuffer},
    };
    use shared::components::{DisconnectPolicy, Ownership, Player};
    use shared::{
        inventory::{GroundItem, InventoryConfig, ItemStack},
        replication::NetworkEntityMap,
        role::NetworkRole,
    };

    #[test]
    fn disconnect_despawns_owned_player() {
        let mut app = shared::app_init(NetworkRole::Server);
        app.init_resource::<ReplicationClients>();
        app.init_resource::<NetworkEntityMap>();
        app.init_resource::<MessageBuffer>();
        app.insert_resource(TickCounter(0));
        entity::plugin(&mut app);
        super::super::ownership::plugin(&mut app);

        let config = InventoryConfig::load_directory(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/week7"),
        )
        .unwrap();
        let mut inventory = config.inventory("player");
        inventory.slots[0].stack = Some(ItemStack {
            charge: 0,
            item: "wood".into(),
            quantity: 5,
        });
        app.insert_resource(InventoryService::new(config));
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
        let player_id = entity::reserve_network_ids(app.world_mut(), 1).unwrap()[0];
        let player = entity::network_spawn_reserved(
            app.world_mut(),
            player_id,
            (
                Player,
                inventory,
                avian2d::physics_transform::Position(bevy::math::Vec2::ZERO),
                Ownership {
                    client_id: 7,
                    disconnect_policy: DisconnectPolicy::Despawn,
                },
            ),
        );
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
        // A second queued lifecycle request must not spill the same items again.
        retire_entity(app.world_mut(), player);
        let mut ground = app.world_mut().query::<&GroundItem>();
        let drops: Vec<_> = ground.iter(app.world()).collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].stack.item, "wood");
        assert_eq!(drops[0].stack.quantity, 5);
    }
}
