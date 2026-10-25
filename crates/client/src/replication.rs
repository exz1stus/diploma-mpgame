use super::{
    interpolation::PositionSample,
    state::{ClientState, Owned, RemoteWorld, ReplicatedEntity},
};
use bevy::{platform::collections::HashMap, prelude::*};
use shared::{
    components::{Ownership, Position},
    protocol::{EntitySnapshot, NetworkIdentity},
    replication::{ReplicatedComponent, ReplicationRegistry},
    server_messages::{ServerMessage, ServerMessages},
};

pub fn process_server_message(
    message: ServerMessage,
    commands: &mut Commands,
    client_state: &mut ClientState,
    remote_world: &mut RemoteWorld,
    registry: &ReplicationRegistry,
    now: f64,
) {
    let tick = message.tick;
    client_state.last_tick = client_state.last_tick.max(tick);

    match message.payload {
        ServerMessages::InitiateClientState { client_id, world } => {
            if world.tickrate == 0 {
                return;
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
            client_state.initiated = true;
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
        }
        ServerMessages::EntitySpawn(snapshot) => {
            if client_state.initiated {
                spawn_remote_entity(
                    snapshot,
                    tick,
                    commands,
                    client_state,
                    remote_world,
                    registry,
                );
            }
        }
        ServerMessages::EntityUpdate(snapshot) => {
            if remote_world.entities.contains_key(&snapshot.id) {
                apply_components(
                    snapshot,
                    tick,
                    commands,
                    client_state,
                    remote_world,
                    registry,
                );
            } else if !remote_world.despawned.contains(&snapshot.id) {
                // Bound memory if an update's reliable spawn never arrives.
                if remote_world.pending_updates.len() == 256 {
                    remote_world.pending_updates.pop_front();
                }
                remote_world.pending_updates.push_back((tick, snapshot));
            }
        }
        ServerMessages::EntityDespawn(id) => {
            if let Some(replica) = remote_world.entities.remove(&id) {
                commands.entity(replica.entity).despawn();
            }
            remote_world.positions.remove(&id);
            remote_world
                .pending_updates
                .retain(|(_, snapshot)| snapshot.id != id);
            remote_world.despawned.insert(id);
        }
        ServerMessages::ComponentRemove {
            entity_id,
            component_id,
        } => {
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
        ServerMessages::Pong(_) => {}
    }
}

fn spawn_remote_entity(
    snapshot: EntitySnapshot,
    tick: u64,
    commands: &mut Commands,
    client_state: &mut ClientState,
    remote_world: &mut RemoteWorld,
    registry: &ReplicationRegistry,
) {
    let id = snapshot.id;

    if remote_world.entities.contains_key(&id) {
        return;
    };
    let entity = commands
        .spawn((
            NetworkIdentity { id },
            Transform::default(),
            GlobalTransform::default(),
        ))
        .id();
    remote_world.despawned.remove(&id);
    remote_world.entities.insert(
        id,
        ReplicatedEntity {
            entity,
            owned: false,
            last_ticks: HashMap::default(),
        },
    );
    apply_components(
        snapshot,
        tick,
        commands,
        client_state,
        remote_world,
        registry,
    );

    let pending = std::mem::take(&mut remote_world.pending_updates);
    for (pending_tick, snapshot) in pending {
        if snapshot.id == id {
            if pending_tick > tick {
                apply_components(
                    snapshot,
                    pending_tick,
                    commands,
                    client_state,
                    remote_world,
                    registry,
                );
            }
        } else {
            remote_world
                .pending_updates
                .push_back((pending_tick, snapshot));
        }
    }
}

fn apply_components(
    snapshot: EntitySnapshot,
    tick: u64,
    commands: &mut Commands,
    client_state: &mut ClientState,
    remote_world: &mut RemoteWorld,
    registry: &ReplicationRegistry,
) {
    client_state.last_tick = client_state.last_tick.max(tick);
    let Some(replica) = remote_world.entities.get_mut(&snapshot.id) else {
        return;
    };
    let entity = replica.entity;
    if let Some(component) = snapshot
        .components
        .iter()
        .find(|component| component.component_id == Ownership::NETWORK_ID)
    {
        if replica
            .last_ticks
            .get(&Ownership::NETWORK_ID)
            .is_none_or(|last| tick >= *last)
        {
            if let Ok(owner) = postcard::from_bytes::<Ownership>(&component.data) {
                let owned = owner.client_id == Some(client_state.id);
                if owned != replica.owned {
                    remote_world.positions.remove(&snapshot.id);
                }
                replica.owned = owned;
                if owned {
                    commands.entity(entity).insert(Owned);
                } else {
                    commands.entity(entity).remove::<Owned>();
                }
            }
        }
    }

    for component in snapshot.components {
        let Some(registered) = registry.components.get(&component.component_id) else {
            continue;
        };

        if replica
            .last_ticks
            .get(&component.component_id)
            .is_some_and(|last| {
                tick < *last || (tick == *last && component.component_id != Ownership::NETWORK_ID)
            })
        {
            continue;
        }
        if component.component_id == Position::NETWORK_ID {
            let Ok(position) = postcard::from_bytes::<Position>(&component.data) else {
                continue;
            };
            if registered.interpolated && !replica.owned {
                let samples = remote_world.positions.entry(snapshot.id).or_default();
                samples.push_back(PositionSample {
                    tick,
                    x: position.x,
                    y: position.y,
                });
                if samples.len() > 32 {
                    samples.pop_front();
                }
                client_state.clock.observe(tick);
            } else {
                remote_world.positions.remove(&snapshot.id);
            }
        }
        replica.last_ticks.insert(component.component_id, tick);
        let apply = registered.apply;
        commands.queue(move |world: &mut World| apply(world, entity, &component.data));
    }
}
