use super::{InventoryService, access::owned_player, sessions::Session};
use crate::net::client::ReplicationClients;
use bevy::prelude::*;
use shared::{
    commands::CommandRequest,
    inventory::{InventoryCommand, InventoryReceipt},
};

pub(super) fn step(world: &mut World) {
    let requests: Vec<_> = world
        .resource_mut::<Messages<CommandRequest<InventoryCommand>>>()
        .drain()
        .collect();

    let now = world.resource::<Time<Real>>().elapsed_secs_f64();

    let ready: Vec<_> = world
        .resource::<ReplicationClients>()
        .ready_clients()
        .collect();

    world.resource_scope(|world, mut service: Mut<InventoryService>| {
        service.sessions.retain(|client, _| ready.contains(client));

        for client in ready {
            let player = owned_player(world, client);

            if let Some(session) = service.sessions.get_mut(&client) {
                session.player = player;
            } else if let Some(player) = player {
                let epoch = service.next_epoch;
                service.next_epoch = epoch.checked_add(1).expect("inventory epoch exhausted");

                let burst = service.config.rules.command_burst;

                service
                    .sessions
                    .insert(client, Session::new(epoch, player, now, burst));
            }
        }

        for request in requests {
            let client = request.context.client_id;

            let Some(mut session) = service.sessions.remove(&client) else {
                continue;
            };

            let result = service.execute(world, client, &mut session, &request.command, now);

            service.publish(
                world,
                client,
                &mut session,
                Some(InventoryReceipt {
                    counter: request.command.counter,
                    result,
                }),
            );

            service.sessions.insert(client, session);
        }

        // Propagates another player's chest changes to every viewer.
        let clients: Vec<_> = service.sessions.keys().copied().collect();

        for client in clients {
            let mut session = service.sessions.remove(&client).unwrap();
            service.publish(world, client, &mut session, None);
            service.sessions.insert(client, session);
        }
    });
}
