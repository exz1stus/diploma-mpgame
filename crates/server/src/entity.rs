use bevy::app::App;
use bevy_ecs::{
    bundle::Bundle,
    lifecycle::{Add, Remove},
    observer::On,
    query::Without,
    resource::{IsResource, Resource},
    system::{Commands, EntityCommands, ParamSet, Query, Res, ResMut},
    world::{EntityRef, EntityWorldMut},
};
use bevy_renet::renet::DefaultChannel::ReliableOrdered;
use shared::{
    replication::{NetworkEntityMap, ReplicationRegistry},
    server_messages::{ServerMessage, ServerMessages},
    snapshots::{EntitySnapshot, NetworkEntityId, NetworkIdentity},
};

use crate::messages::{BufferedServerMessage, MessageBuffer, MessageTarget, TickCounter};

#[derive(Resource, Debug, Default)]
struct NetworkIdAllocator {
    next_id: NetworkEntityId,
}

pub(crate) fn configure(app: &mut App) {
    app.init_resource::<NetworkIdAllocator>();
    app.add_observer(on_network_identity_inserted);
    app.add_observer(on_network_identity_removed);
}
pub(crate) trait NetworkCommandsExt {
    fn network_spawn<T: Bundle>(&mut self, bundle: T) -> EntityCommands<'_>;
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
}

fn on_network_identity_inserted(
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
            sequence: 0,
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
            sequence: 0,
            tick: tick_counter.0,
            payload: ServerMessages::EntityDespawn(network_id),
        },
    });
}
