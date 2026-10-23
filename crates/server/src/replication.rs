use std::collections::BTreeMap;

use bevy::{
    app::FixedPostUpdate,
    platform::collections::HashMap,
    prelude::{App, IntoScheduleConfigs},
};
use bevy_ecs::{
    entity::Entity,
    lifecycle::RemovedComponents,
    query::Added,
    system::{Query, Res, ResMut},
};
use bevy_renet::{RenetServer, renet::ClientId};
use shared::{
    protocol::{ComponentSnapshot, EntitySnapshot, NetworkEntityId, NetworkIdentity},
    replication::{
        BufferedComponentChanges, ComponentChange, NetworkEntityMap, ReplicationRegistry,
        ReplicationSet,
    },
    server_messages::ServerMessages,
};

use crate::messages::{MessageBuffer, MessageContext, TickCounter};

type SnapshotBatch = BTreeMap<u8, Vec<EntitySnapshot>>;

pub fn configure(app: &mut App) {
    app.add_systems(
        FixedPostUpdate,
        (sync_entity_map, queue_changes)
            .chain()
            .after(ReplicationSet::Collect),
    );
}

fn sync_entity_map(
    added: Query<(Entity, &NetworkIdentity), Added<NetworkIdentity>>,
    mut removed: RemovedComponents<NetworkIdentity>,
    mut entities: ResMut<NetworkEntityMap>,
) {
    for (entity, identity) in &added {
        entities.insert(entity, identity.id);
    }

    for entity in removed.read() {
        entities.remove(entity);
    }
}

pub(crate) fn queue_changes(
    mut changes: ResMut<BufferedComponentChanges>,
    registry: Res<ReplicationRegistry>,
    entities: Res<NetworkEntityMap>,
    server: Res<RenetServer>,
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

    let clients = server.clients_id();
    let mut ctx = MessageContext {
        tick: tick.0,
        outgoing: &mut buffer.outgoing,
    };

    push_changes(&mut ctx, &clients, &batch);
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
            .entry(config.delivery)
            .or_default()
            .entry(network_id)
            .or_insert_with(|| EntitySnapshot {
                id: network_id,
                components: Vec::new(),
                removed_components: Vec::new(),
            });

        match change {
            ComponentChange::Upsert(data) => {
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

fn push_changes(ctx: &mut MessageContext, clients: &[ClientId], batch: &SnapshotBatch) {
    for &client_id in clients {
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
