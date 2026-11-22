//! Validate an inventory action and stage its complete transaction before committing.

use super::{
    InventoryService, Result,
    access::{actor, lookup, position, public_inventory, within},
    locks::{LockKey, Reservations},
    sessions::Session,
    transactions::{GroundChange, Plan},
};
use crate::networking::entity::reserve_network_ids;
use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use shared::{
    components::Health,
    inventory::{
        Destination, GroundItem, InventoryAction, InventoryError as Error,
        InventoryOutcome as Outcome, ItemStack, definitions::ConsumeEffect,
    },
};

impl InventoryService {
    pub(super) fn apply(
        &mut self,
        world: &mut World,
        client: ClientId,
        session: &mut Session,
        action: &InventoryAction,
    ) -> Result<Outcome> {
        match action {
            InventoryAction::Refresh => return Ok(Outcome::default()),

            InventoryAction::CloseContainer => {
                session.open = None;
                return Ok(Outcome::default());
            }

            InventoryAction::OpenContainer { container } => {
                let (_, _, origin) = actor(world, client, session)?;
                public_inventory(world, *container, origin, self.config.rules.container_range)?;

                session.open = Some(*container);
                return Ok(Outcome::default());
            }

            _ => {}
        }

        let (actor_id, actor_entity, origin) = actor(world, client, session)?;
        let mut plan = Plan::default();

        match action {
            InventoryAction::CollectItem {
                ground,
                expected_revision,
                quantity,
            } => {
                self.check_quantity(*quantity)?;

                let entity = lookup(world, *ground)?;
                within(
                    origin,
                    position(world, entity)?,
                    self.config.rules.pickup_range,
                )?;

                let mut item = world
                    .get::<GroundItem>(entity)
                    .cloned()
                    .ok_or(Error::NotFound)?;

                if item.revision != *expected_revision {
                    return Err(Error::StaleSlot);
                }

                if *quantity > item.stack.quantity {
                    return Err(Error::InvalidQuantity);
                }

                let moved = ItemStack {
                    item: item.stack.item.clone(),
                    quantity: *quantity,
                };

                plan.put(world, &self.config, Destination::Auto(actor_id), moved)?;

                item.stack.quantity -= *quantity;

                if item.stack.quantity == 0 {
                    plan.ground = Some(GroundChange::Remove(entity));
                } else {
                    item.revision = item.revision.checked_add(1).ok_or(Error::Overflow)?;

                    plan.ground = Some(GroundChange::Replace(entity, item));
                }

                plan.keys.insert(LockKey::Ground(*ground));
                plan.outcome.moved_quantity = *quantity;
            }

            InventoryAction::DropItem {
                source,
                quantity,
                direction,
            } => {
                self.check_quantity(*quantity)?;

                if source.inventory != actor_id {
                    return Err(Error::AccessDenied);
                }

                if !direction.is_finite() {
                    return Err(Error::InvalidDirection);
                }

                let direction = direction.try_normalize().ok_or(Error::InvalidDirection)?;

                let stack = plan.take(world, *source, *quantity)?;
                let location = origin + direction * self.config.rules.drop_distance;

                if !location.is_finite() {
                    return Err(Error::InvalidDirection);
                }

                // Reserve the ID before changing live inventory.
                let id = reserve_network_ids(world, 1)?[0];

                plan.spawn
                    .push((id, location, GroundItem { revision: 0, stack }));

                plan.outcome.moved_quantity = *quantity;
            }

            InventoryAction::TransferContainerItem {
                source,
                destination,
                quantity,
            } => {
                self.check_quantity(*quantity)?;
                self.check_source(world, session, actor_id, origin, *source)?;
                self.check_destination(world, session, actor_id, origin, *source, *destination)?;

                if let Destination::Slot(target) = destination {
                    if source.inventory == target.inventory && source.slot == target.slot {
                        return Err(Error::AccessDenied);
                    }
                } else if destination.inventory() == source.inventory {
                    return Err(Error::AccessDenied);
                }

                let stack = plan.take(world, *source, *quantity)?;
                plan.put(world, &self.config, *destination, stack)?;
                plan.outcome.moved_quantity = *quantity;
            }

            InventoryAction::ConsumeItem { source } => {
                if source.inventory != actor_id {
                    return Err(Error::AccessDenied);
                }

                let stack = plan.take(world, *source, 1)?;

                let definition = self
                    .config
                    .items
                    .get(&stack.item)
                    .ok_or(Error::InvalidItem)?;

                let health = world
                    .get::<Health>(actor_entity)
                    .ok_or(Error::CannotConsume)?
                    .0;

                let new_health = match definition.consume.as_ref() {
                    Some(ConsumeEffect::Heal { amount }) => {
                        if health >= self.config.rules.max_health {
                            return Err(Error::CannotConsume);
                        }

                        u32::from(health)
                            .checked_add(u32::from(*amount))
                            .ok_or(Error::Overflow)?
                            .min(u32::from(self.config.rules.max_health))
                            as u16
                    }

                    None => return Err(Error::CannotConsume),
                };

                plan.health = Some((actor_entity, new_health));
                plan.keys.insert(LockKey::Health(actor_id));
                plan.outcome.healed = new_health - health;
                plan.outcome.moved_quantity = 1;
            }

            _ => unreachable!("nonmutating actions returned above"),
        }

        plan.finish()?;

        // This entire method runs in an exclusive World system.
        // No other inventory transaction can interleave between plan and commit.
        let _reservation = Reservations::acquire(&mut self.locks, &plan.keys)?;
        let outcome = plan.outcome;
        plan.commit(world);

        Ok(outcome)
    }
}
