//! Client-side world entities, spawned from the server's world stream.
//! They carry data and world-space transforms only; rendering is separate.

use bevy::prelude::*;
use shared::world::{ChunkPos, Footprint, ObjectId, Tile};

/// RGBA color per tile id, as sent by the server for its world preset.
#[derive(Resource, Clone, Debug)]
pub struct WorldPalette {
    pub tile_colors: Vec<[u8; 4]>,
}

/// A streamed chunk. Its transform is the chunk's minimum corner in world space;
/// it is spawned visible so rendering can attach visual children.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldChunk {
    pub pos: ChunkPos,
}

/// Ground tiles, indexed `y * size + x` in chunk-local coordinates, +Y up.
#[derive(Component, Clone, Debug)]
pub struct ChunkTiles {
    pub size: u32,
    pub tiles: Vec<Tile>,
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
