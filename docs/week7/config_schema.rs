//! Reference data schema for the accompanying RON files.
//! This file is not part of a game crate. See README.md for integration.
//! Gameplay types, commands, transactions, and replication are separate work.

use serde::Deserialize;

pub type Rgba = (u8, u8, u8, u8);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemFile {
    pub schema_version: u32,
    pub items: Vec<ItemDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDefinition {
    pub id: String,
    pub display_name: String,
    pub max_stack: u32,
    pub icon: Option<String>,
    pub fallback_rgba: Rgba,
    pub consume: Option<ConsumeEffect>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ConsumeEffect {
    Heal { amount: u16 },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryFile {
    pub schema_version: u32,
    pub profiles: Vec<InventoryProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryProfile {
    pub id: String,
    pub display_name: String,
    pub slots: u16,
    pub access: InventoryAccess,
}

#[derive(Debug, Deserialize)]
pub enum InventoryAccess {
    OwnerOnly,
    ServerOnly,
    NearbyLoot,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesFile {
    pub schema_version: u32,
    pub pickup_range: f32,
    pub container_range: f32,
    pub give_range: f32,
    pub drop_distance: f32,
    pub max_health: u16,
    pub max_quantity_per_command: u32,
    pub commands_per_second: u32,
    pub command_burst: u32,
    pub cached_results_per_session: usize,
    pub client_pending_timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiFile {
    pub schema_version: u32,
    pub slot_size_px: f32,
    pub slot_gap_px: f32,
    pub layouts: Vec<InventoryLayout>,
    pub bindings: Bindings,
    pub empty_rgba: Rgba,
    pub selected_rgba: Rgba,
    pub pending_rgba: Rgba,
    pub busy_rgba: Rgba,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryLayout {
    pub profile: String,
    pub columns: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bindings {
    pub toggle_inventory: String,
    pub interact_or_collect: String,
    pub drop_selected: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemoSceneFile {
    pub schema_version: u32,
    pub chests: Vec<ChestFixture>,
    pub ground_items: Vec<GroundItemFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChestFixture {
    pub id: String,
    pub profile: String,
    pub position: (f32, f32),
    pub contents: Vec<SlotFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotFixture {
    pub slot: u16,
    pub item: String,
    pub quantity: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundItemFixture {
    pub id: String,
    pub position: (f32, f32),
    pub item: String,
    pub quantity: u32,
}
