//! Deterministic placement on completed terrain, independent of tile appearance.

mod definition;
mod placer;
mod rule;

pub use definition::{
    Footprint, ObjectDefinition, ObjectId, ObjectInstanceId, ObjectRepository, ObjectsError,
    PlacedObject,
};
pub use placer::ObjectPlacer;
pub use rule::ObjectRule;

#[cfg(test)]
mod tests;
