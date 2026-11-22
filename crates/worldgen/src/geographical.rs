use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::{ChunkPos, TilePos};

/// A rectangular world with its geographic origin at the northwest boundary.
/// X increases eastward and Y increases southward.
pub struct WorldGeography {
    pub chunk_size: u32,
    pub world_height: u32, // north-south extent in tiles
    pub world_width: u32,  // west-east extent in tiles
}

/// Geographic angles in radians, ready for solar calculations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPosition {
    /// Positive northward, in [-PI/2, PI/2] for positions inside the world.
    pub latitude: f64,
    /// Positive eastward, in [-PI, PI] for positions inside the world.
    /// Both endpoints represent the same meridian.
    pub longitude: f64,
}

impl WorldGeography {
    pub fn new(chunk_size: u32, world_size: u32) -> Self {
        assert!(chunk_size > 0, "chunk_size must be positive");
        assert!(world_size > 0, "world_size must be positive");
        let world_width = world_size
            .checked_mul(chunk_size)
            .expect("world dimensions overflow u32");
        let world_height = world_width;
        Self {
            chunk_size,
            world_width,
            world_height,
        }
    }

    pub fn global_to_chunk(&self, tile_pos: TilePos) -> ChunkPos {
        let size = i64::from(self.chunk_size);
        ChunkPos {
            x: tile_pos.x.div_euclid(size),
            y: tile_pos.y.div_euclid(size),
        }
    }

    /// Global position of the chunk's first tile.
    pub fn chunk_to_global(&self, chunk_pos: ChunkPos) -> TilePos {
        let size = i64::from(self.chunk_size);
        TilePos {
            x: chunk_pos
                .x
                .checked_mul(size)
                .expect("chunk X exceeds tile coordinate range"),
            y: chunk_pos
                .y
                .checked_mul(size)
                .expect("chunk Y exceeds tile coordinate range"),
        }
    }

    pub fn geo(&self, tile_pos: TilePos) -> GeoPosition {
        GeoPosition {
            latitude: self.latitude(tile_pos.y),
            longitude: self.longitude(tile_pos.x),
        }
    }

    /// Latitude in radians: north boundary +PI/2, equator 0, south boundary -PI/2.
    pub fn latitude(&self, y: i64) -> f64 {
        FRAC_PI_2 - (y as f64 / f64::from(self.world_height)) * PI
    }

    /// Longitude in radians: west boundary -PI, prime meridian 0, east boundary +PI.
    pub fn longitude(&self, x: i64) -> f64 {
        (x as f64 / f64::from(self.world_width)) * TAU - PI
    }
}
