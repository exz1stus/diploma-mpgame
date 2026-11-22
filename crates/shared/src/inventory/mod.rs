//! Shared inventory API. Data, protocol, and editable configuration live in separate modules.

mod components;
mod config;
pub mod definitions;
mod fixtures;
mod protocol;

pub use components::{Access, Chest, GroundItem, Inventory, InventorySlot, ItemStack};
pub use config::InventoryConfig;
pub use protocol::{
    Destination, InventoryAction, InventoryCommand, InventoryError, InventoryOutcome,
    InventoryPacket, InventoryReceipt, InventoryState, InventoryView, SlotRef,
};

pub type ItemId = String;
pub type InventoryId = crate::snapshots::NetworkEntityId;
