//! Client-side world data, spawned from the server's world stream.
//! Entities carry data and world-space transforms only; presentation is a separate plugin
//! ([`crate::world::presentation`]).

use bevy::prelude::*;
use shared::world::{ChunkPos, Footprint, ObjectId, TileSample, WorldManifest};

/// The streamed world's manifest: chunk size and the names behind biome and object IDs.
/// Present while a world stream is active.
#[derive(Resource, Clone, Debug, Deref)]
pub struct WorldInfo(pub WorldManifest);

/// A streamed chunk. Its transform is the chunk's minimum corner in world space;
/// it is spawned visible so presentation can attach visual children.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldChunk {
    pub pos: ChunkPos,
}

/// Ground tiles (biome and height), indexed `y * size + x` in chunk-local coordinates, +Y up.
#[derive(Component, Clone, Debug)]
pub struct ChunkTiles {
    pub size: u32,
    pub tiles: Vec<TileSample>,
}

/// A world object (rock, tree, ...). Its transform is the footprint's center.
#[derive(Component, Clone, Debug)]
pub struct WorldObject {
    pub kind: ObjectId,
    pub footprint: Footprint,
}

#[derive(Component, Debug)]
#[relationship(relationship_target = ChunkContents)]
pub struct InChunk(#[entities] pub Entity);

/// Objects of a chunk; they despawn with it.
#[derive(Component, Default, Debug)]
#[relationship_target(relationship = InChunk, linked_spawn)]
pub struct ChunkContents(Vec<Entity>);
