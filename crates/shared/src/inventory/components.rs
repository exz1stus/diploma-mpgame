use super::ItemId;
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    Owner,
    Public,
    ServerOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub item: ItemId,
    pub quantity: u32,
    pub charge: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySlot {
    pub revision: u64,
    pub stack: Option<ItemStack>,
}

#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub profile: String,
    pub revision: u64,
    pub access: Access,
    pub accepts_gifts: bool,
    pub slots: Vec<InventorySlot>,
}

#[derive(Component, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Chest;

#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundItem {
    pub revision: u64,
    pub stack: ItemStack,
}

impl Inventory {
    /// One beacon owns a single stack and reserves the rest of its cargo footprint.
    pub fn reserved_slots(&self) -> usize {
        let extra = (2 * self.slots.len()).div_ceil(3).saturating_sub(1);
        self.slots
            .iter()
            .filter_map(|s| s.stack.as_ref())
            .filter(|s| s.item == super::survival::BEACON)
            .count()
            * extra
    }

    pub fn used_slots(&self) -> usize {
        self.slots.iter().filter(|s| s.stack.is_some()).count() + self.reserved_slots()
    }

    pub fn cargo_reserved(&self, index: usize) -> bool {
        self.slots[index].stack.is_none()
            && self.reserved_slots() > 0
            && self.slots[index..]
                .iter()
                .filter(|s| s.stack.is_none())
                .count()
                <= self.reserved_slots()
    }
}
