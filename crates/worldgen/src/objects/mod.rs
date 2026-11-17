//! Deterministic placement on completed terrain, independent of appearance.

mod definition;
mod placer;
mod rule;
mod spawns;

pub use definition::{
    Footprint, ObjectDefinition, ObjectId, ObjectInstanceId, ObjectRepository, ObjectsError,
    PlacedObject,
};
pub use placer::ObjectPlacer;
pub use rule::ObjectRule;
pub use spawns::{FeatureSpawns, feature_rules, rule_id, spawn_rules};

#[cfg(test)]
mod tests;
