//! Draw order (world-space Z) of everything the 2D camera renders. Higher is drawn on top.

/// Terrain chunks.
pub const GROUND: f32 = -10.0;
/// World objects: above the ground, below entities.
pub const OBJECTS: f32 = -9.0;
/// Networked entities (players, enemies, projectiles).
pub const ENTITIES: f32 = 0.1;
