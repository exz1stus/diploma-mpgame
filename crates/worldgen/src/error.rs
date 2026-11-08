use std::{error::Error, fmt};

use crate::{ChunkPos, TilePos};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerationError {
    TileOutsideWorld(TilePos),
    ChunkOutsideWorld(ChunkPos),
    NoBiome(TilePos),
    NoTile(TilePos),
    InvalidRegion,
}

impl fmt::Display for GenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TileOutsideWorld(pos) => write!(f, "tile {pos:?} is outside the world"),
            Self::ChunkOutsideWorld(pos) => write!(f, "chunk {pos:?} is outside the world"),
            Self::NoBiome(pos) => write!(f, "no biome covers tile {pos:?}"),
            Self::NoTile(pos) => write!(f, "no tile mapping covers tile {pos:?}"),
            Self::InvalidRegion => write!(f, "invalid generation region or world dimensions"),
        }
    }
}

impl Error for GenerationError {}
