use bevy_ecs::{component::Component, entity::Entity};
use worldgen::{ChunkPos, PlacedObject, Tile};

/// Keeps chunks loaded (and simulated) around this entity's physics position.
/// Distance is in chunks, measured as a square around the observer's chunk.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct WorldObserver {
    pub simulation_distance: u32,
}

impl Default for WorldObserver {
    fn default() -> Self {
        Self {
            simulation_distance: 4,
        }
    }
}

/// A loaded chunk. Its world objects are related through [`InChunk`] / [`ChunkContents`]
/// and are despawned together with it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Chunk {
    pub pos: ChunkPos,
}

/// Ground tiles, indexed `y * size + x` in chunk-local coordinates.
#[derive(Component, Clone, Debug)]
pub(crate) struct ChunkTiles {
    pub size: u32,
    pub tiles: Vec<Tile>,
}

/// A generated object living as an ECS entity (tree, rock, ...). Game logic attaches more
/// components to it (growth, health, ...); the chunk saves it back when unloading.
#[derive(Component, Clone, Debug)]
pub(crate) struct WorldObject {
    pub placed: PlacedObject,
}

#[derive(Component, Debug)]
#[relationship(relationship_target = ChunkContents)]
pub(crate) struct InChunk(#[entities] pub Entity);

#[derive(Component, Default, Debug)]
#[relationship_target(relationship = InChunk, linked_spawn)]
pub(crate) struct ChunkContents(Vec<Entity>);
