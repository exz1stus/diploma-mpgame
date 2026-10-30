use bevy::app::App;
use bevy_ecs::{
    bundle::Bundle,
    component::Component,
    entity::Entity,
    lifecycle::{Add, Remove},
    observer::On,
    query::Without,
    resource::{IsResource, Resource},
    system::{Commands, EntityCommands, ParamSet, Query, Res, ResMut},
    world::{EntityRef, EntityWorldMut},
};
use bevy_renet::renet::{ClientId, DefaultChannel::ReliableOrdered};
use shared::{
    components::{DisconnectPolicy, Ownership},
    replication::{NetworkEntityMap, ReplicationRegistry},
    server_messages::{ServerMessage, ServerMessages},
    snapshots::{EntitySnapshot, NetworkEntityId, NetworkIdentity},
};

use crate::networking::{client::ReplicationClients, server::TickCounter};

use super::messages::{BufferedServerMessage, MessageBuffer, MessageTarget};

#[derive(Resource, Debug, Default)]
struct NetworkIdAllocator {
    next_id: NetworkEntityId,
}

pub(crate) fn configure(app: &mut App) {
    app.init_resource::<NetworkIdAllocator>();
    app.add_observer(on_network_identity_added);
    app.add_observer(on_network_identity_removed);
}

pub(crate) trait NetworkCommandsExt {
    fn network_spawn<T: Bundle>(&mut self, bundle: T) -> EntityCommands<'_>;
    fn network_spawn_with_ownership<T: Bundle>(
        &mut self,
        client_id: ClientId,
        bundle: T,
    ) -> EntityCommands<'_>;
    fn network_spawn_with_ownership_and_policy<T: Bundle>(
        &mut self,
        client_id: ClientId,
        policy: DisconnectPolicy,
        bundle: T,
    ) -> EntityCommands<'_>;
}

impl NetworkCommandsExt for Commands<'_, '_> {
    fn network_spawn<T: Bundle>(&mut self, bundle: T) -> EntityCommands<'_> {
        let mut entity = self.spawn(bundle);

        entity.queue(|mut entity: EntityWorldMut| {
            let id = {
                let mut allocator = entity.resource_mut::<NetworkIdAllocator>();

                allocator.next_id += 1;
                allocator.next_id
            };

            entity.insert(NetworkIdentity { id });
        });

        entity
    }

    fn network_spawn_with_ownership<T: Bundle>(
        &mut self,
        client_id: ClientId,
        bundle: T,
    ) -> EntityCommands<'_> {
        self.network_spawn_with_ownership_and_policy(client_id, DisconnectPolicy::default(), bundle)
    }

    fn network_spawn_with_ownership_and_policy<T: Bundle>(
        &mut self,
        client_id: ClientId,
        policy: DisconnectPolicy,
        bundle: T,
    ) -> EntityCommands<'_> {
        self.network_spawn((
            Ownership {
                client_id,
                disconnect_policy: policy,
            },
            bundle,
        ))
    }
}

fn on_network_identity_added(
    event: On<Add, NetworkIdentity>,
    mut params: ParamSet<(Query<EntityRef, Without<IsResource>>, ResMut<MessageBuffer>)>,
    registry: Res<ReplicationRegistry>,
    mut entities: ResMut<NetworkEntityMap>,
    tick_counter: Res<TickCounter>,
) {
    let query = params.p0();

    let Ok(entity) = query.get(event.entity) else {
        return;
    };

    let Some(id) = entity.get::<NetworkIdentity>() else {
        return;
    };

    entities.insert(event.entity, id.id);

    let snapshot = { EntitySnapshot::from_entity(entity, &registry) };

    let Some(snapshot) = snapshot else {
        return;
    };

    let spawn_message = BufferedServerMessage {
        target: MessageTarget::Broadcast,
        channel: ReliableOrdered.into(),
        message: ServerMessage {
            sequence: None,
            tick: tick_counter.0,
            payload: ServerMessages::EntitySpawn(snapshot),
        },
    };

    params.p1().outgoing.push_back(spawn_message);
}

fn on_network_identity_removed(
    event: On<Remove, NetworkIdentity>,
    query: Query<&NetworkIdentity>,
    mut entities: ResMut<NetworkEntityMap>,
    mut buffer: ResMut<MessageBuffer>,
    tick_counter: Res<TickCounter>,
) {
    let Ok(identity) = query.get(event.entity) else {
        return;
    };

    let network_id = identity.id;
    entities.remove(event.entity);

    buffer.outgoing.push_back(BufferedServerMessage {
        target: MessageTarget::Broadcast,
        channel: ReliableOrdered.into(),
        message: ServerMessage {
            sequence: None,
            tick: tick_counter.0,
            payload: ServerMessages::EntityDespawn(network_id),
        },
    });
}
