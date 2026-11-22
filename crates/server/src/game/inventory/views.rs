use super::{
    InventoryService,
    access::{lookup, position, public_inventory},
    sessions::Session,
};
use crate::net::{
    messages::{BufferedServerMessage, MessageBuffer, MessageTarget},
    server::TickCounter,
};
use bevy::prelude::*;
use bevy_renet::renet::{ClientId, DefaultChannel};
use shared::{
    components::{Health, Ownership},
    inventory::{Inventory, InventoryPacket, InventoryReceipt, InventoryState, InventoryView},
    server_messages::{ServerMessage, ServerMessages},
};

impl InventoryService {
    pub(super) fn publish(
        &self,
        world: &mut World,
        client: ClientId,
        session: &mut Session,
        receipt: Option<InventoryReceipt>,
    ) {
        let state = self.visible_state(world, client, session);

        if receipt.is_none() && session.last_state.as_ref() == Some(&state) {
            return;
        }

        session.serial = session
            .serial
            .checked_add(1)
            .expect("inventory frame counter exhausted");

        session.last_state = Some(state.clone());

        let packet = InventoryPacket {
            epoch: session.epoch,
            serial: session.serial,
            state,
            receipt,
        };

        let tick = world.resource::<TickCounter>().0;

        world
            .resource_mut::<MessageBuffer>()
            .outgoing
            .push_back(BufferedServerMessage {
                target: MessageTarget::Client(client),
                channel: DefaultChannel::ReliableOrdered.into(),
                message: ServerMessage {
                    sequence: None,
                    tick,
                    payload: ServerMessages::Inventory(packet),
                },
            });
    }

    fn visible_state(
        &self,
        world: &World,
        client: ClientId,
        session: &mut Session,
    ) -> InventoryState {
        let Some(player_id) = session.player else {
            session.open = None;
            return InventoryState::default();
        };

        let Ok(player) = lookup(world, player_id) else {
            session.open = None;
            return InventoryState::default();
        };

        if world
            .get::<Ownership>(player)
            .is_none_or(|owner| owner.client_id != client)
        {
            session.open = None;
            return InventoryState::default();
        }

        let Some(inventory) = world.get::<Inventory>(player) else {
            session.open = None;
            return InventoryState::default();
        };

        let mut views = vec![InventoryView {
            id: player_id,
            inventory: inventory.clone(),
        }];

        if let Some(container) = session.open {
            let allowed = position(world, player).and_then(|origin| {
                public_inventory(world, container, origin, self.config.rules.container_range)
            });

            if let Ok(entity) = allowed {
                views.push(InventoryView {
                    id: container,
                    inventory: world
                        .get::<Inventory>(entity)
                        .expect("checked above")
                        .clone(),
                });
            } else {
                session.open = None;
            }
        }

        InventoryState {
            player: Some(player_id),
            open_container: session.open,
            expedition: world
                .get_resource::<super::expedition::Expedition>()
                .and_then(|mission| {
                    Some(Box::new(mission.view(
                        &self.config.expedition,
                        world.get::<shared::inventory::survival::LifeSupport>(player)?,
                    )))
                }),
            health: world.get::<Health>(player).map(|health| health.0),
            views,
        }
    }
}
