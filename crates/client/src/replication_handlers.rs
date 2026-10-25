use crate::{
    replication::{apply_components, spawn_remote_entity},
    state::{ClientState, Owned, RemoteWorld},
};
use bevy::ecs::{system::Commands, world::World};
use bevy_renet::renet::ClientId;
use shared::{
    components::{Ownership, Position},
    replication::{ReplicatedComponent, ReplicationRegistry},
    snapshots::{EntitySnapshot, NetworkComponentId, NetworkEntityId, WorldSnapshot},
};

pub fn handle_initial_state(
    client_id: ClientId,
    world: WorldSnapshot,
    tick: u64,
    commands: &mut Commands,
    client_state: &mut ClientState,
    remote_world: &mut RemoteWorld,
    registry: &ReplicationRegistry,
    now: f64,
) -> bool {
    if world.tickrate == 0 {
        return false;
    }
    for (_, replica) in remote_world.entities.drain() {
        commands.entity(replica.entity).despawn();
    }
    remote_world.positions.clear();
    remote_world.despawned.clear();
    remote_world
        .pending_updates
        .retain(|(update_tick, _)| *update_tick > tick);

    client_state.id = client_id;
    client_state.sequence = 0;
    client_state.last_tick = tick;
    client_state.clock.reset(tick, world.tickrate, now);

    for snapshot in world.entities {
        spawn_remote_entity(
            snapshot,
            tick,
            commands,
            client_state,
            remote_world,
            registry,
        );
    }
    client_state.initiated = true;
    true
}
pub fn handle_entity_spawn(
    snapshot: EntitySnapshot,
    tick: u64,
    commands: &mut Commands,
    client_state: &mut ClientState,
    remote_world: &mut RemoteWorld,
    registry: &ReplicationRegistry,
) {
    if !client_state.initiated {
        return;
    }

    spawn_remote_entity(
        snapshot,
        tick,
        commands,
        client_state,
        remote_world,
        registry,
    );
}
pub fn handle_entity_update(
    snapshot: EntitySnapshot,
    tick: u64,
    commands: &mut Commands,
    client_state: &mut ClientState,
    remote_world: &mut RemoteWorld,
    registry: &ReplicationRegistry,
) {
    if remote_world.entities.contains_key(&snapshot.id) {
        apply_components(
            snapshot,
            tick,
            commands,
            client_state,
            remote_world,
            registry,
        );
        return;
    }

    if remote_world.despawned.contains(&snapshot.id) {
        return;
    }

    // Bound memory if an update's reliable spawn never arrives.
    if remote_world.pending_updates.len() == 256 {
        remote_world.pending_updates.pop_front();
    }

    remote_world.pending_updates.push_back((tick, snapshot));
}
pub fn handle_entity_despawn(
    id: NetworkEntityId,
    commands: &mut Commands,
    remote_world: &mut RemoteWorld,
) {
    if let Some(replica) = remote_world.entities.remove(&id) {
        commands.entity(replica.entity).despawn();
    }

    remote_world.positions.remove(&id);
    remote_world
        .pending_updates
        .retain(|(_, snapshot)| snapshot.id != id);
    remote_world.despawned.insert(id);
}
pub fn handle_component_remove(
    entity_id: NetworkEntityId,
    component_id: NetworkComponentId,
    tick: u64,
    commands: &mut Commands,
    remote_world: &mut RemoteWorld,
    registry: &ReplicationRegistry,
) {
    let Some(replica) = remote_world.entities.get_mut(&entity_id) else {
        return;
    };

    let Some(registered) = registry.components.get(&component_id) else {
        return;
    };

    if replica
        .last_ticks
        .get(&component_id)
        .is_some_and(|last| tick < *last)
    {
        return;
    }

    replica.last_ticks.insert(component_id, tick);

    if component_id == Position::NETWORK_ID {
        remote_world.positions.remove(&entity_id);
    }

    if component_id == Ownership::NETWORK_ID {
        replica.owned = false;
        commands.entity(replica.entity).remove::<Owned>();
    }

    let entity = replica.entity;
    let remove = registered.remove;

    commands.queue(move |world: &mut World| remove(world, entity));
}
