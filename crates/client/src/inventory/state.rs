//! Confirmed client views, pending command identity, selection, and drag references.

use bevy::prelude::Resource;
use shared::inventory::{
    InventoryAction, InventoryCommand, InventoryError, InventoryPacket, InventoryState, ItemStack,
    SlotRef,
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
    pub(super) mission_tab: bool,
    pub(super) position: bevy::math::Vec2,
    pub(super) waypoint: Option<u16>,
    pub(super) beacon_signal: Option<bevy::math::Vec2>,
    pub escape_consumed: bool,

    pub(super) state: InventoryState,
    pub(super) pending: Option<Pending>,
    pub(super) actions: VecDeque<InventoryAction>,
    pub(super) selected: Option<SlotRef>,
    pub(super) quantity: u32,
    pub(super) recipient: Option<u64>,
    pub(super) error: Option<String>,
    pub(super) notice: Option<String>,
    pub(super) search: String,
    pub(super) filter: ItemFilter,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ItemFilter {
    #[default]
    All,
    Materials,
    Consumables,
    Equipment,
}

impl InventoryClient {
    pub(crate) fn receive_catalog(&mut self, mut packet: InventoryPacket, signature: u64) {
        let mismatched = packet
            .state
            .expedition
            .as_ref()
            .is_some_and(|v| v.catalog != signature);
        if mismatched {
            packet.state.expedition = None;
        }
        self.receive(packet);
        if mismatched {
            self.error=Some("Your expedition catalog differs from the server. Use the same expedition.ron and reconnect.".into());
        }
    }

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
            if self.selected_stack().is_none() {
                self.selected = None;
            }
        }

        if let Some(receipt) = packet.receipt {
            let matches_pending = self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.command.counter == receipt.counter);

            if matches_pending {
                self.pending = None;
                self.selected = None;

                match receipt.result {
                    Ok(outcome) => {
                        self.error = None;
                        self.notice = if outcome.healed > 0 {
                            Some(format!("Restored {} health.", outcome.healed))
                        } else if outcome.moved_quantity > 0 {
                            Some(format!("Moved {} item(s).", outcome.moved_quantity))
                        } else {
                            None
                        };
                    }
                    Err(error) => {
                        self.error = Some(error_message(error).into());
                        self.notice = None;
                    }
                }
            }
        }
    }

    pub(super) fn enqueue(&mut self, action: InventoryAction) {
        if self.epoch != 0 && self.pending.is_none() && self.actions.is_empty() {
            self.actions.push_back(action);
            self.error = None;
            self.notice = None;
        }
    }

    pub(super) fn selected_stack(&self) -> Option<(SlotRef, &ItemStack)> {
        let selected = self.selected?;

        let view = self
            .state
            .views
            .iter()
            .find(|view| view.id == selected.inventory)?;

        let slot = view.inventory.slots.get(usize::from(selected.slot))?;

        if slot.revision != selected.expected_revision {
            return None;
        }

        Some((selected, slot.stack.as_ref()?))
    }

    pub(super) fn selected_quantity(&self) -> Option<(SlotRef, u32)> {
        let (selected, stack) = self.selected_stack()?;

        Some((selected, self.quantity.max(1).min(stack.quantity)))
    }
}

fn error_message(error: InventoryError) -> &'static str {
    match error {
        InventoryError::NoPower => "The ship needs power. Check the generator and battery.",
        InventoryError::MissingMaterials => "Your backpack lacks the required materials.",
        InventoryError::RequiresDiscovery => "Complete the preceding expedition objective first.",
        InventoryError::Depleted => "This site has already been exhausted.",
        InventoryError::NotFound => "This item or container is no longer available.",
        InventoryError::AccessDenied => "You cannot move items to or from that inventory.",
        InventoryError::OutOfRange => "Move closer and try again.",
        InventoryError::InvalidQuantity => "Choose an amount within the stack and transfer limit.",
        InventoryError::InvalidDirection => "Unable to drop in that direction. Try again.",
        InventoryError::StaleSlot => "That stack changed. Select it again to continue.",
        InventoryError::InventoryFull => "There is not enough room for that amount.",
        InventoryError::IncompatibleStack => "Choose an empty slot or a stack of the same item.",
        InventoryError::SlotBusy => "That slot is busy. Try again shortly.",
        InventoryError::CannotConsume => {
            "This item cannot be used, or your health is already full."
        }
        InventoryError::WrongSession => "Your inventory session changed. Reconnect to continue.",
        InventoryError::InvalidCounter
        | InventoryError::OperationTooOld
        | InventoryError::OperationIdReused => {
            "Inventory request is out of sync. Reconnect to continue."
        }
        InventoryError::RateLimited => "Too many actions. Wait a moment and try again.",
        InventoryError::Overflow => "The inventory limit was reached.",
        InventoryError::InvalidItem => "This item definition is unavailable.",
    }
}

#[derive(Clone)]
pub(super) struct DragStack {
    pub(super) source: SlotRef,
    pub(super) quantity: u32,
}
