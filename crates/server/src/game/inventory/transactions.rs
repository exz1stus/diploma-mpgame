//! Stage slot changes on clones; bump revisions and commit only after every check passes.

use super::{Id, Result, access::lookup, lifecycle::spawn_ground, locks::LockKey};
use bevy::prelude::*;
use shared::{
    components::Health,
    inventory::{
        Destination, GroundItem, Inventory, InventoryConfig, InventoryError as Error,
        InventoryOutcome as Outcome, ItemStack, SlotRef,
    },
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Plan {
    inventories: BTreeMap<Id, (Entity, Inventory)>,
    pub(super) keys: BTreeSet<LockKey>,
    pub(super) ground: Option<GroundChange>,
    pub(super) spawn: Vec<(Id, Vec2, GroundItem)>,
    pub(super) health: Option<(Entity, u16)>,
    pub(super) outcome: Outcome,
}

pub(super) enum GroundChange {
    Replace(Entity, GroundItem),
    Remove(Entity),
}

impl Plan {
    fn inventory<'a>(&'a mut self, world: &World, id: Id) -> Result<&'a mut Inventory> {
        if let std::collections::btree_map::Entry::Vacant(entry) = self.inventories.entry(id) {
            let entity = lookup(world, id)?;
            let inventory = world
                .get::<Inventory>(entity)
                .cloned()
                .ok_or(Error::NotFound)?;

            entry.insert((entity, inventory));
        }

        Ok(&mut self.inventories.get_mut(&id).unwrap().1)
    }

    pub(super) fn take(
        &mut self,
        world: &World,
        source: SlotRef,
        quantity: u32,
    ) -> Result<ItemStack> {
        let inventory = self.inventory(world, source.inventory)?;
        let slot = inventory
            .slots
            .get_mut(usize::from(source.slot))
            .ok_or(Error::NotFound)?;

        if slot.revision != source.expected_revision {
            return Err(Error::StaleSlot);
        }

        let stack = slot.stack.as_mut().ok_or(Error::NotFound)?;

        if quantity == 0 || quantity > stack.quantity {
            return Err(Error::InvalidQuantity);
        }

        let moved = ItemStack {
            item: stack.item.clone(),
            quantity,
        };

        stack.quantity -= quantity;

        if stack.quantity == 0 {
            slot.stack = None;
        }

        self.keys
            .insert(LockKey::Slot(source.inventory, source.slot));
        Ok(moved)
    }

    pub(super) fn put(
        &mut self,
        world: &World,
        config: &InventoryConfig,
        destination: Destination,
        stack: ItemStack,
    ) -> Result<()> {
        let maximum = config
            .items
            .get(&stack.item)
            .ok_or(Error::InvalidItem)?
            .max_stack;

        let id = destination.inventory();
        let inventory = self.inventory(world, id)?;

        let candidates: Vec<usize> = match destination {
            Destination::Slot(target) => {
                let slot = inventory
                    .slots
                    .get(usize::from(target.slot))
                    .ok_or(Error::NotFound)?;

                if slot.revision != target.expected_revision {
                    return Err(Error::StaleSlot);
                }

                if slot
                    .stack
                    .as_ref()
                    .is_some_and(|existing| existing.item != stack.item)
                {
                    return Err(Error::IncompatibleStack);
                }

                vec![usize::from(target.slot)]
            }

            Destination::Auto(_) => {
                // Fill compatible stacks before using empty slots.
                let partial = inventory
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(index, slot)| {
                        slot.stack.as_ref().and_then(|existing| {
                            (existing.item == stack.item && existing.quantity < maximum)
                                .then_some(index)
                        })
                    });

                let empty = inventory
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(index, slot)| slot.stack.is_none().then_some(index));

                partial.chain(empty).collect()
            }
        };

        let mut remaining = stack.quantity;
        let mut touched = Vec::new();

        for index in candidates {
            if remaining == 0 {
                break;
            }

            let slot = &mut inventory.slots[index];
            let existing = slot.stack.as_ref().map_or(0, |item| item.quantity);
            let free = maximum.checked_sub(existing).ok_or(Error::InvalidItem)?;
            let amount = remaining.min(free);

            if amount == 0 {
                continue;
            }

            match slot.stack.as_mut() {
                Some(item) => {
                    item.quantity = item.quantity.checked_add(amount).ok_or(Error::Overflow)?;
                }

                None => {
                    slot.stack = Some(ItemStack {
                        item: stack.item.clone(),
                        quantity: amount,
                    });
                }
            }

            remaining -= amount;
            touched.push(index as u16);
        }

        if remaining != 0 {
            // Only staged clones changed; the source remains untouched in World.
            return Err(Error::InventoryFull);
        }

        for slot in touched {
            self.keys.insert(LockKey::Slot(id, slot));
        }

        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        for key in &self.keys {
            if let LockKey::Slot(id, index) = key {
                let slot = &mut self
                    .inventories
                    .get_mut(id)
                    .expect("staged inventory")
                    .1
                    .slots[usize::from(*index)];

                slot.revision = slot.revision.checked_add(1).ok_or(Error::Overflow)?;
            }
        }

        for (_, inventory) in self.inventories.values_mut() {
            inventory.revision = inventory.revision.checked_add(1).ok_or(Error::Overflow)?;
        }

        Ok(())
    }

    pub(super) fn commit(self, world: &mut World) {
        // Entity existence cannot change during this exclusive operation.
        for (_, (entity, inventory)) in self.inventories {
            world.entity_mut(entity).insert(inventory);
        }

        if let Some((entity, health)) = self.health {
            world.entity_mut(entity).insert(Health(health));
        }

        match self.ground {
            Some(GroundChange::Replace(entity, ground)) => {
                world.entity_mut(entity).insert(ground);
            }

            Some(GroundChange::Remove(entity)) => {
                // Immediate removal prevents another pickup in this tick.
                world.despawn(entity);
            }

            None => {}
        }

        for (id, position, item) in self.spawn {
            spawn_ground(world, id, position, item);
        }
    }
}
