//! Validate demo loot and positions before any test-world entities are spawned.

use super::{
    InventoryConfig,
    definitions::{DemoSceneFile, InventoryAccess},
};
use std::{collections::BTreeSet, error::Error};

impl InventoryConfig {
    /// Parse and validate the entire fixture before the server spawns anything.
    pub fn load_demo_scene(&self) -> Result<DemoSceneFile, Box<dyn Error>> {
        let text = std::fs::read_to_string(Self::directory().join("demo_scene.ron"))?;
        let scene: DemoSceneFile = ron::from_str(&text)?;
        if scene.schema_version != 1 {
            return Err("unsupported demo scene schema version".into());
        }

        let mut ids = BTreeSet::new();
        let valid_stack = |item: &str, quantity: u32| {
            self.items
                .get(item)
                .is_some_and(|definition| quantity > 0 && quantity <= definition.max_stack)
        };

        for chest in &scene.chests {
            if chest.id.trim().is_empty() || !ids.insert(chest.id.as_str()) {
                return Err("demo fixture IDs must be nonempty and unique".into());
            }
            if !chest.position.0.is_finite() || !chest.position.1.is_finite() {
                return Err(format!("invalid position for fixture {}", chest.id).into());
            }
            let profile = self
                .profiles
                .get(&chest.profile)
                .ok_or_else(|| format!("unknown demo profile {}", chest.profile))?;
            if !matches!(&profile.access, InventoryAccess::NearbyLoot) {
                return Err(format!("demo chest {} requires a lootable profile", chest.id).into());
            }
            let mut occupied = BTreeSet::new();
            for stack in &chest.contents {
                if stack.slot >= profile.slots
                    || !occupied.insert(stack.slot)
                    || !valid_stack(&stack.item, stack.quantity)
                {
                    return Err(format!("invalid contents for fixture {}", chest.id).into());
                }
            }
        }
        for ground in &scene.ground_items {
            if ground.id.trim().is_empty() || !ids.insert(ground.id.as_str()) {
                return Err("demo fixture IDs must be nonempty and unique".into());
            }
            if !ground.position.0.is_finite()
                || !ground.position.1.is_finite()
                || !valid_stack(&ground.item, ground.quantity)
            {
                return Err(format!("invalid ground fixture {}", ground.id).into());
            }
        }

        Ok(scene)
    }
}
