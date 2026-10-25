use bevy_ecs::{
    query::{With, Without},
    resource::IsResource,
    system::Query,
    world::EntityRef,
};
use shared::{
    replication::ReplicationRegistry,
    snapshots::{EntitySnapshot, NetworkIdentity, WorldSnapshot},
};

pub(crate) fn build_world_snapshot(
    tickrate: u32,
    entities: &Query<EntityRef, (With<NetworkIdentity>, Without<IsResource>)>,
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
