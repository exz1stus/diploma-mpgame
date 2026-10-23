use std::collections::VecDeque;

use bevy::platform::collections::HashMap;
use bevy_renet::renet::DefaultChannel;
use shared::{
    protocol::NetworkEntityId,
    replication::{BufferedComponentChanges, ComponentChange, ReplicationRegistry},
};

type EntityChanges = HashMap<NetworkEntityId, VecDeque<ComponentChange>>;

type ChannelChanges = HashMap<DefaultChannel, EntityChanges>;

fn build_client_changes(
    changes: &BufferedComponentChanges,
    registry: &ReplicationRegistry,
) -> ChannelChanges {
    let mut channels = ChannelChanges::new();

    for (&(entity_id, component_id), change) in &changes.changes {
        // if !client.known_entities.contains(&entity_id) {
        //     continue;
        // }
        // TODO: implement client network culling

        let network_id = 0; // TODO get_network_id(entity);

        let Some(config) = registry.components.get(&component_id) else {
            continue;
        };

        channels
            .entry(config.delivery)
            .or_default()
            .entry(network_id)
            .or_default()
            .push(change);
    }

    channels
}
