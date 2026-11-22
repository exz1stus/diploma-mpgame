//! Submit one inventory operation at a time and retry with the same identity after a timeout.

use super::state::{InventoryClient, Pending};
use crate::networking::{
    network::{NetworkSimulation, queue_outgoing},
    state::ClientState,
};
use bevy::prelude::*;
use shared::{
    client_messages::ClientMessages,
    commands::CommandEnvelope,
    inventory::{InventoryCommand, InventoryConfig},
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        send_inventory_commands
            .after(crate::networking::network::release_incoming)
            .before(crate::networking::network::release_outgoing),
    );
}

fn send_inventory_commands(world: &mut World) {
    if !world.resource::<ClientState>().initiated {
        return;
    }

    let now = world.resource::<Time<Real>>().elapsed_secs_f64();

    let timeout = world
        .resource::<InventoryConfig>()
        .rules
        .client_pending_timeout_ms as f64
        / 1000.0;

    let command = {
        let mut inventory = world.resource_mut::<InventoryClient>();

        if inventory.epoch == 0 {
            return;
        }

        if let Some(pending) = inventory.pending.as_mut() {
            if now - pending.last_sent < timeout {
                return;
            }

            // Retry the exact operation identity and payload.
            pending.last_sent = now;
            pending.command.clone()
        } else {
            let Some(action) = inventory.actions.pop_front() else {
                return;
            };

            let Some(counter) = inventory.counter.checked_add(1) else {
                inventory.error = Some("Inventory counter exhausted".into());
                return;
            };

            inventory.counter = counter;

            let command = InventoryCommand {
                epoch: inventory.epoch,
                counter,
                action,
            };

            inventory.pending = Some(Pending {
                command: command.clone(),
                last_sent: now,
            });

            command
        }
    };

    let envelope = match CommandEnvelope::encode(&command) {
        Ok(envelope) => envelope,
        Err(error) => {
            let mut inventory = world.resource_mut::<InventoryClient>();
            inventory.pending = None;
            inventory.error = Some(format!("Encoding failed: {error:?}"));
            return;
        }
    };

    world.resource_scope(|world, mut simulation: Mut<NetworkSimulation>| {
        let mut state = world.resource_mut::<ClientState>();

        queue_outgoing(
            &mut simulation,
            &mut state,
            ClientMessages::NetworkCommand(envelope),
            now,
        );
    });
}
