//! Load and validate editable RON catalogs, rules, layouts, and inventory profiles.

use super::{
    Access, Inventory, InventorySlot,
    definitions::{
        self, InventoryAccess, InventoryFile, InventoryProfile, ItemDefinition, ItemFile,
        RulesFile, UiFile,
    },
};
use bevy::prelude::Resource;
use std::{
    collections::BTreeMap,
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Resource)]
pub struct InventoryConfig {
    pub items: BTreeMap<String, ItemDefinition>,
    pub profiles: BTreeMap<String, InventoryProfile>,
    pub rules: RulesFile,
    pub ui: UiFile,
}

impl InventoryConfig {
    pub fn directory() -> PathBuf {
        std::env::var_os("INVENTORY_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/week7"))
    }

    pub fn load() -> Result<Self, Box<dyn Error>> {
        Self::load_directory(&Self::directory())
    }

    pub fn load_directory(directory: &Path) -> Result<Self, Box<dyn Error>> {
        fn read<T: serde::de::DeserializeOwned>(
            directory: &Path,
            name: &str,
        ) -> Result<T, Box<dyn Error>> {
            let text = std::fs::read_to_string(directory.join(name))?;
            Ok(ron::from_str(&text)?)
        }

        let items: ItemFile = read(directory, "items.ron")?;
        let inventories: InventoryFile = read(directory, "inventories.ron")?;
        let rules: RulesFile = read(directory, "rules.ron")?;
        let ui: UiFile = read(directory, "ui.ron")?;

        if [
            items.schema_version,
            inventories.schema_version,
            rules.schema_version,
            ui.schema_version,
        ]
        .into_iter()
        .any(|version| version != 1)
        {
            return Err("unsupported inventory schema version".into());
        }

        let mut item_map = BTreeMap::new();

        for item in items.items {
            if item.id.trim().is_empty() || item.max_stack == 0 {
                return Err("invalid item definition".into());
            }

            if let Some(definitions::ConsumeEffect::Heal { amount }) = item.consume.as_ref()
                && *amount == 0
            {
                return Err("healing amount must be positive".into());
            }

            if item_map.insert(item.id.clone(), item).is_some() {
                return Err("duplicate item ID".into());
            }
        }

        let mut profiles = BTreeMap::new();

        for profile in inventories.profiles {
            if profile.id.trim().is_empty() || profile.slots == 0 {
                return Err("invalid inventory profile".into());
            }

            if profiles.insert(profile.id.clone(), profile).is_some() {
                return Err("duplicate inventory profile ID".into());
            }
        }

        for required in ["player", "enemy", "corpse", "wooden_chest"] {
            if !profiles.contains_key(required) {
                return Err(format!("missing profile: {required}").into());
            }
        }

        for distance in [
            rules.pickup_range,
            rules.container_range,
            rules.give_range,
            rules.drop_distance,
        ] {
            if !distance.is_finite() || distance <= 0.0 {
                return Err("inventory distances must be finite and positive".into());
            }
        }

        if rules.max_health == 0
            || rules.max_quantity_per_command == 0
            || rules.commands_per_second == 0
            || rules.command_burst == 0
            || rules.cached_results_per_session == 0
            || rules.client_pending_timeout_ms == 0
        {
            return Err("inventory limits must be positive".into());
        }

        if !ui.slot_size_px.is_finite()
            || ui.slot_size_px <= 0.0
            || !ui.slot_gap_px.is_finite()
            || ui.slot_gap_px < 0.0
        {
            return Err("invalid inventory UI sizes".into());
        }

        let mut layouts = std::collections::BTreeSet::new();

        for layout in &ui.layouts {
            if layout.columns == 0
                || !profiles.contains_key(&layout.profile)
                || !layouts.insert(layout.profile.clone())
            {
                return Err("invalid or duplicate inventory layout".into());
            }
        }

        if layouts.len() != profiles.len() {
            return Err("every inventory profile needs a UI layout".into());
        }

        Ok(Self {
            items: item_map,
            profiles,
            rules,
            ui,
        })
    }

    pub fn inventory(&self, profile: &str) -> Inventory {
        let definition = self
            .profiles
            .get(profile)
            .expect("inventory profile must be validated at startup");

        let access = match &definition.access {
            InventoryAccess::OwnerOnly => Access::Owner,
            InventoryAccess::NearbyLoot => Access::Public,
            InventoryAccess::ServerOnly => Access::ServerOnly,
        };

        Inventory {
            profile: profile.to_owned(),
            revision: 0,
            access,
            accepts_gifts: true,
            slots: vec![InventorySlot::default(); usize::from(definition.slots)],
        }
    }
}
