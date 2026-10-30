use std::collections::BTreeMap;

use bevy::{
    app::{App, FixedPostUpdate},
    platform::collections::HashMap,
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::system::{Res, ResMut};
use bevy_renet::renet::ClientId;
use shared::{
    replication::{
        BufferedComponentChanges, ComponentChange, NetworkEntityMap, ReplicationRegistry,
    },
    server_messages::ServerMessages,
    snapshots::{ComponentSnapshot, EntitySnapshot, NetworkEntityId},
};

use crate::networking::server::TickCounter;

use super::{
    client::ReplicationClients,
    messages::{MessageBuffer, MessageContext},
    server::TickSet,
};

type SnapshotBatch = BTreeMap<u8, Vec<EntitySnapshot>>;

pub(crate) fn configure(app: &mut App) {
    app.add_systems(FixedPostUpdate, queue_changes.in_set(TickSet::QueueChanges));
}

fn queue_changes(
    mut changes: ResMut<BufferedComponentChanges>,
    clients: Res<ReplicationClients>,
    registry: Res<ReplicationRegistry>,
    entities: Res<NetworkEntityMap>,
    tick: Res<TickCounter>,
    mut buffer: ResMut<MessageBuffer>,
) {
    if changes.changes.is_empty() {
        return;
    }

    let pending = BufferedComponentChanges {
        changes: std::mem::take(&mut changes.changes),
    };

    let batch = build_channels_snapshots(&pending, &registry, &entities);
    if batch.is_empty() {
        return;
    }

    let mut ctx = MessageContext {
        tick: tick.0,
        outgoing: &mut buffer.outgoing,
    };

    push_changes(&mut ctx, clients.ready_clients(), &batch);
}

fn build_channels_snapshots(
    changes: &BufferedComponentChanges,
    registry: &ReplicationRegistry,
    entities: &NetworkEntityMap,
) -> SnapshotBatch {
    let mut grouped: HashMap<u8, HashMap<NetworkEntityId, EntitySnapshot>> = HashMap::new();

    for (&(entity, component_id), change) in &changes.changes {
        let Some(network_id) = entities.get_network_id(entity) else {
            continue;
        };
        let Some(config) = registry.components.get(&component_id) else {
            continue;
        };

        let snapshot = grouped
            .entry(change.delivery_channel(config.delivery))
            .or_default()
            .entry(network_id)
            .or_insert_with(|| EntitySnapshot::new(network_id));

        match change {
            ComponentChange::Added(data) | ComponentChange::Upsert(data) => {
                snapshot.components.push(ComponentSnapshot {
                    component_id,
                    data: data.clone(),
                });
            }
            ComponentChange::Remove => {
                snapshot.removed_components.push(component_id);
            }
        }
    }

    grouped
        .into_iter()
        .map(|(channel, entities)| (channel, entities.into_values().collect()))
        .collect()
}

fn push_changes(
    ctx: &mut MessageContext,
    clients: impl IntoIterator<Item = ClientId>,
    batch: &SnapshotBatch,
) {
    for client_id in clients {
        for (&channel, snapshots) in batch {
            for snapshot in snapshots {
                ctx.push_outgoing(
                    client_id,
                    channel,
                    ServerMessages::EntityUpdate(snapshot.clone()),
                );
            }
        }
    }
}
