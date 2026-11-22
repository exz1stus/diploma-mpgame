//! Confirmed client views, pending command identity, selection, and drag references.

use bevy::prelude::Resource;
use shared::inventory::{
    InventoryAction, InventoryCommand, InventoryPacket, InventoryState, SlotRef,
};
use std::collections::VecDeque;

pub(super) struct Pending {
    pub(super) command: InventoryCommand,
    pub(super) last_sent: f64,
}

#[derive(Resource, Default)]
pub(crate) struct InventoryClient {
    pub(super) epoch: u64,
    pub(super) serial: u64,
    pub(super) counter: u64,

    pub visible: bool,
    pub escape_consumed: bool,

    pub(super) state: InventoryState,
    pub(super) pending: Option<Pending>,
    pub(super) actions: VecDeque<InventoryAction>,
    pub(super) selected: Option<SlotRef>,
    pub(super) quantity: u32,
    pub(super) recipient: Option<u64>,
    pub(super) error: Option<String>,
}

impl InventoryClient {
    pub fn receive(&mut self, packet: InventoryPacket) {
        if self.epoch == 0 {
            self.epoch = packet.epoch;
        }

        // Reset this resource on reconnect, rather than accepting a late epoch.
        if packet.epoch != self.epoch {
            return;
        }

        if packet.serial > self.serial {
            self.serial = packet.serial;
            self.state = packet.state;
        }

        if let Some(receipt) = packet.receipt {
            let matches_pending = self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.command.counter == receipt.counter);

            if matches_pending {
                self.pending = None;
                self.selected = None;

                self.error = receipt.result.err().map(|error| format!("{error:?}"));
            }
        }
    }

    pub(super) fn enqueue(&mut self, action: InventoryAction) {
        if self.epoch != 0 && self.pending.is_none() && self.actions.is_empty() {
            self.actions.push_back(action);
        }
    }

    pub(super) fn selected_quantity(&self) -> Option<(SlotRef, u32)> {
        let selected = self.selected?;

        let view = self
            .state
            .views
            .iter()
            .find(|view| view.id == selected.inventory)?;

        let stack = view
            .inventory
            .slots
            .get(usize::from(selected.slot))?
            .stack
            .as_ref()?;

        Some((selected, self.quantity.max(1).min(stack.quantity)))
    }
}

#[derive(Clone)]
pub(super) struct DragStack {
    pub(super) source: SlotRef,
    pub(super) quantity: u32,
}
