//! Inventory wire commands, operation results, and revisioned private views.

use super::{Inventory, InventoryId};
use crate::{commands::NetworkCommand, snapshots::NetworkEntityId};
use bevy::prelude::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotRef {
    pub inventory: InventoryId,
    pub slot: u16,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Destination {
    Auto(InventoryId),
    Slot(SlotRef),
}

impl Destination {
    pub fn inventory(self) -> InventoryId {
        match self {
            Self::Auto(id) => id,
            Self::Slot(slot) => slot.inventory,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InventoryAction {
    CollectItem {
        ground: NetworkEntityId,
        expected_revision: u64,
        quantity: u32,
    },
    DropItem {
        source: SlotRef,
        quantity: u32,
        direction: Vec2,
    },
    TransferContainerItem {
        source: SlotRef,
        destination: Destination,
        quantity: u32,
    },
    ConsumeItem {
        source: SlotRef,
    },
    OpenContainer {
        container: InventoryId,
    },
    CloseContainer,
    Refresh,
    Expedition(super::survival::ExpeditionAction),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryCommand {
    pub epoch: u64,
    pub counter: u64,
    pub action: InventoryAction,
}

impl NetworkCommand for InventoryCommand {
    // ID 1 is retired. Keep the inventory ID stable so old requests cannot be reinterpreted.
    const NETWORK_ID: u16 = 2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InventoryError {
    NotFound,
    AccessDenied,
    OutOfRange,
    InvalidQuantity,
    InvalidDirection,
    StaleSlot,
    InventoryFull,
    IncompatibleStack,
    SlotBusy,
    CannotConsume,
    WrongSession,
    InvalidCounter,
    OperationTooOld,
    OperationIdReused,
    RateLimited,
    Overflow,
    InvalidItem,
    NoPower,
    MissingMaterials,
    RequiresDiscovery,
    Depleted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryOutcome {
    pub moved_quantity: u32,
    pub healed: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryView {
    pub id: InventoryId,
    pub inventory: Inventory,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InventoryState {
    pub player: Option<InventoryId>,
    pub open_container: Option<InventoryId>,
    pub health: Option<u16>,
    pub expedition: Option<Box<super::survival::ExpeditionView>>,
    pub views: Vec<InventoryView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryReceipt {
    pub counter: u64,
    pub result: Result<InventoryOutcome, InventoryError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryPacket {
    pub epoch: u64,
    // Independent of server tick. Several commits can happen in one tick.
    pub serial: u64,
    // Complete set of inventories this client may currently inspect.
    pub state: InventoryState,
    pub receipt: Option<InventoryReceipt>,
}
