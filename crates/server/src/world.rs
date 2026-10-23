use bevy_ecs::{query::With, system::Query, world::EntityRef};
use shared::{
    protocol::{ComponentSnapshot, EntitySnapshot, NetworkIdentity, WorldSnapshot},
    replication::ReplicationRegistry,
};

pub fn build_world_snapshot(
    entities: &Query<EntityRef, With<NetworkIdentity>>,
    registry: &ReplicationRegistry,
) -> WorldSnapshot {
    let mut snapshots = Vec::new();

    for entity in entities.iter() {
        let id = entity.get::<NetworkIdentity>().unwrap().id;
        let components: Vec<_> = registry
            .components
            .iter()
            .filter_map(|(&component_id, config)| {
                (config.serialize)(entity).map(|data| ComponentSnapshot { component_id, data })
            })
            .collect();

        snapshots.push(EntitySnapshot {
            id,
            components,
            removed_components: Vec::new(),
        });
    }

    WorldSnapshot {
        entities: snapshots,
    }
}
