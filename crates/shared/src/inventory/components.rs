//! Reusable inventory, slot, stack, chest, and ground-item ECS data.

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
