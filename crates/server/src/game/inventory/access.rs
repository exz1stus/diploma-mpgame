use super::{Id, InventoryService, Result, sessions::Session};
use crate::networking::client::ReplicationClients;
use avian2d::physics_transform::Position;
use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use shared::{
    components::{Health, Ownership, Player},
    inventory::{Access, Destination, Inventory, InventoryError as Error, SlotRef},
    replication::NetworkEntityMap,
};

impl InventoryService {
    pub(super) fn check_quantity(&self, quantity: u32) -> Result<()> {
        if quantity == 0 || quantity > self.config.rules.max_quantity_per_command {
            return Err(Error::InvalidQuantity);
        }

        Ok(())
    }

    pub(super) fn check_source(
        &self,
        world: &World,
        session: &Session,
        actor: Id,
        origin: Vec2,
        source: SlotRef,
    ) -> Result<()> {
        if source.inventory == actor {
            return Ok(());
        }

        if session.open != Some(source.inventory) {
            return Err(Error::AccessDenied);
        }

        public_inventory(
            world,
            source.inventory,
            origin,
            self.config.rules.container_range,
        )?;

        Ok(())
    }

    pub(super) fn check_destination(
        &self,
        world: &World,
        session: &Session,
        actor: Id,
        origin: Vec2,
        source: SlotRef,
        destination: Destination,
    ) -> Result<()> {
        let id = destination.inventory();

        if id == actor {
            return Ok(());
        }

        let entity = lookup(world, id)?;
        let inventory = world.get::<Inventory>(entity).ok_or(Error::NotFound)?;

        match inventory.access {
            Access::Public => {
                if session.open != Some(id) {
                    return Err(Error::AccessDenied);
                }

                public_inventory(world, id, origin, self.config.rules.container_range)?;
            }

            Access::Owner => {
                // Giving only: own source -> automatic recipient insertion.
                if source.inventory != actor
                    || !matches!(destination, Destination::Auto(_))
                    || !inventory.accepts_gifts
                    || world.get::<Player>(entity).is_none()
                    || world.get::<Ownership>(entity).is_none()
                    || world.get::<Health>(entity).is_none_or(|h| h.0 == 0)
                {
                    return Err(Error::AccessDenied);
                }

                within(
                    origin,
                    position(world, entity)?,
                    self.config.rules.give_range,
                )?;
            }

            Access::ServerOnly => return Err(Error::AccessDenied),
        }

        Ok(())
    }
}

pub(super) fn lookup(world: &World, id: Id) -> Result<Entity> {
    world
        .resource::<NetworkEntityMap>()
        .get_local_entity(id)
        .filter(|entity| world.get_entity(*entity).is_ok())
        .ok_or(Error::NotFound)
}

pub(super) fn position(world: &World, entity: Entity) -> Result<Vec2> {
    let value = world
        .get::<Position>(entity)
        .map(|position| position.0)
        .ok_or(Error::NotFound)?;

    if !value.is_finite() {
        return Err(Error::OutOfRange);
    }

    Ok(value)
}

pub(super) fn within(a: Vec2, b: Vec2, maximum: f32) -> Result<()> {
    let distance = a.distance_squared(b);

    if !distance.is_finite() || distance > maximum * maximum {
        return Err(Error::OutOfRange);
    }

    Ok(())
}

pub(super) fn actor(
    world: &World,
    client: ClientId,
    session: &Session,
) -> Result<(Id, Entity, Vec2)> {
    let id = session.player.ok_or(Error::AccessDenied)?;
    let entity = lookup(world, id)?;

    if world.get::<Player>(entity).is_none()
        || world
            .get::<Health>(entity)
            .is_none_or(|health| health.0 == 0)
        || world
            .get::<Ownership>(entity)
            .is_none_or(|owner| owner.client_id != client)
    {
        return Err(Error::AccessDenied);
    }

    Ok((id, entity, position(world, entity)?))
}

pub(super) fn public_inventory(
    world: &World,
    id: Id,
    origin: Vec2,
    maximum: f32,
) -> Result<Entity> {
    let entity = lookup(world, id)?;
    let inventory = world.get::<Inventory>(entity).ok_or(Error::NotFound)?;

    if inventory.access != Access::Public {
        return Err(Error::AccessDenied);
    }

    within(origin, position(world, entity)?, maximum)?;
    Ok(entity)
}

pub(super) fn owned_player(world: &World, client: ClientId) -> Option<Id> {
    let clients = world.resource::<ReplicationClients>();
    let state = clients.clients.get(&client)?;

    state.owned_entities.iter().find_map(|entity| {
        let owner = world.get::<Ownership>(*entity)?;

        if owner.client_id != client
            || world.get::<Player>(*entity).is_none()
            || world.get::<Inventory>(*entity).is_none()
        {
            return None;
        }

        world
            .get::<shared::snapshots::NetworkIdentity>(*entity)
            .map(|identity| identity.id)
    })
}
