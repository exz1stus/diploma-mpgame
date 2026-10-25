use bevy_ecs::{query::With, system::Query, world::EntityRef};
use shared::{
    replication::ReplicationRegistry,
    snapshots::{EntitySnapshot, NetworkIdentity, WorldSnapshot},
};

pub fn build_world_snapshot(
    tickrate: u32,
    entities: &Query<EntityRef, With<NetworkIdentity>>,
    registry: &ReplicationRegistry,
) -> WorldSnapshot {
    let snapshots = entities
        .iter()
        .filter_map(|entity| EntitySnapshot::from_entity(entity, registry))
        .collect();

    WorldSnapshot {
        tickrate,
        entities: snapshots,
    }
}
