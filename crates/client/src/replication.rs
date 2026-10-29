use crate::replication_handlers::{
    handle_component_remove, handle_entity_despawn, handle_entity_spawn, handle_entity_update,
    handle_initial_state,
};

use super::{
    interpolation::PositionSample,
    state::{ClientState, Owned, RemoteWorld, ReplicatedEntity},
};
use bevy::{platform::collections::HashMap, prelude::*};
use shared::{
    components::{Ownership, Position},
    replication::{ReplicatedComponent, ReplicationRegistry},
    server_messages::{ServerMessage, ServerMessages},
    snapshots::{EntitySnapshot, NetworkIdentity},
};

pub fn process_server_message(
    message: ServerMessage,
    commands: &mut Commands,
    client_state: &mut ClientState,
    remote_world: &mut RemoteWorld,
    registry: &ReplicationRegistry,
    now: f64,
) -> bool {
    let tick = message.tick;
    client_state.last_tick = client_state.last_tick.max(tick);

    match message.payload {
        ServerMessages::InitiateClientState { client_id, world } => {
            return handle_initial_state(
                client_id,
                world,
                tick,
                commands,
                client_state,
                remote_world,
                registry,
                now,
            );
        }
        ServerMessages::EntitySpawn(snapshot) => {
            handle_entity_spawn(
                snapshot,
                tick,
                commands,
                client_state,
                remote_world,
                registry,
            );
        }
        ServerMessages::EntityUpdate(snapshot) => {
            handle_entity_update(
                snapshot,
                tick,
                commands,
                client_state,
                remote_world,
                registry,
            );
        }
        ServerMessages::EntityDespawn(id) => {
            handle_entity_despawn(id, commands, remote_world);
        }
        ServerMessages::ComponentRemove {
            entity_id,
            component_id,
        } => {
            handle_component_remove(
                entity_id,
                component_id,
                tick,
                commands,
                remote_world,
                registry,
            );
        }
        ServerMessages::MovementCorrection(_) => {
            todo!();
        }
        ServerMessages::Pong(_) => {}
    }
    false
}

pub fn spawn_remote_entity(
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

pub fn apply_components(
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
                let owned = owner.client_id == client_state.id;
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
