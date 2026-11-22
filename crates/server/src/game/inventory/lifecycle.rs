//! Ground entity spawning and one-time inventory spilling when an owner is removed.

use super::{
    Id, InventoryService, Result,
    access::position,
    locks::{LockKey, Reservations},
};
use crate::networking::entity::{network_spawn_reserved, reserve_network_ids};
use avian2d::physics_transform::Position;
use bevy::prelude::*;
use shared::{
    components::NetworkPhysicsState,
    inventory::{GroundItem, Inventory, InventoryError as Error},
};
use std::collections::BTreeSet;

impl InventoryService {
    fn retire(&mut self, world: &mut World, entity: Entity) -> Result<()> {
        if world.get_entity(entity).is_err() {
            return Ok(());
        }

        let Some(inventory) = world.get::<Inventory>(entity).cloned() else {
            world.despawn(entity);
            return Ok(());
        };

        let id = world
            .get::<shared::snapshots::NetworkIdentity>(entity)
            .ok_or(Error::NotFound)?
            .id;

        let stacks: Vec<_> = inventory
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.stack.clone().map(|stack| (index as u16, stack)))
            .collect();

        let origin = if stacks.is_empty() {
            Vec2::ZERO
        } else {
            position(world, entity)?
        };

        let keys: BTreeSet<_> = stacks
            .iter()
            .map(|(slot, _)| LockKey::Slot(id, *slot))
            .collect();

        let _reservation = Reservations::acquire(&mut self.locks, &keys)?;
        let ids = reserve_network_ids(world, stacks.len())?;

        // All validation/allocation completed before live mutation.
        world.entity_mut(entity).remove::<Inventory>();

        for (ground_id, (_, stack)) in ids.into_iter().zip(stacks) {
            spawn_ground(world, ground_id, origin, GroundItem { revision: 0, stack });
        }

        world.despawn(entity);
        Ok(())
    }
}

pub(super) fn spawn_ground(world: &mut World, id: Id, position: Vec2, item: GroundItem) {
    network_spawn_reserved(
        world,
        id,
        (
            item,
            Position(position),
            NetworkPhysicsState {
                position,
                ..default()
            },
            Transform::from_xyz(position.x, position.y, 0.0),
        ),
    );
}

pub(crate) fn retire_entity(world: &mut World, entity: Entity) {
    world.resource_scope(|world, mut service: Mut<InventoryService>| {
        if let Err(error) = service.retire(world, entity) {
            // Keep the entity and its inventory if the spill cannot commit.
            warn!("Inventory retirement failed: {error:?}");
        }
    });
}
