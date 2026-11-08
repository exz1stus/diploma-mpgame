use serde::{Deserialize, Serialize};

use crate::{PlacedObject, Tile, TilePos};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: i64,
    pub y: i64,
}

/// A complete chunk response. Tiles use index y * size + x in local coordinates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkData {
    pub position: ChunkPos,
    pub size: u32,
    pub tiles: Vec<Tile>,
    /// All objects intersecting this chunk, including anchors in adjacent chunks.
    /// Deduplicate by instance ID when loading multiple chunks.
    pub objects: Vec<PlacedObject>,
}

/// Integer tile bounds with an inclusive minimum and exclusive maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRect {
    pub min: TilePos,
    pub max: TilePos,
}

impl TileRect {
    pub fn is_empty(self) -> bool {
        self.min.x >= self.max.x || self.min.y >= self.max.y
    }

    pub fn contains(self, pos: TilePos) -> bool {
        pos.x >= self.min.x && pos.x < self.max.x && pos.y >= self.min.y && pos.y < self.max.y
    }

    pub fn contains_rect(self, other: Self) -> bool {
        !other.is_empty()
            && self.contains(other.min)
            && other.max.x <= self.max.x
            && other.max.y <= self.max.y
    }

    pub fn intersects(self, other: Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.min.x < other.max.x
            && other.min.x < self.max.x
            && self.min.y < other.max.y
            && other.min.y < self.max.y
    }

    pub fn positions(self) -> impl Iterator<Item = TilePos> {
        (self.min.y..self.max.y)
            .flat_map(move |y| (self.min.x..self.max.x).map(move |x| TilePos { x, y }))
    }
}

#[derive(Clone, Copy)]
pub struct WorldBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl WorldBounds {
    pub fn intersects_circle(&self, x: f64, y: f64, radius: f64) -> bool {
        let closest_x = x.clamp(self.min_x, self.max_x);
        let closest_y = y.clamp(self.min_y, self.max_y);

        let dx = x - closest_x;
        let dy = y - closest_y;

        dx * dx + dy * dy <= radius * radius
    }
}

impl ChunkPos {
    pub fn world_bounds(&self, chunk_size: u32) -> WorldBounds {
        let min_x = self.x * chunk_size as i64;
        let min_y = self.y * chunk_size as i64;

        WorldBounds {
            min_x: min_x as f64,
            min_y: min_y as f64,
            max_x: (min_x + chunk_size as i64) as f64,
            max_y: (min_y + chunk_size as i64) as f64,
        }
    }
}
