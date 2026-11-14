use bevy::math::Vec2;
use worldgen::{ChunkPos, TilePos, WorldGeneratorConfig};

pub(crate) use shared::world::TILE_SIZE;

pub(crate) fn chunk_at_world(position: Vec2, chunk_size: u32) -> ChunkPos {
    let span = TILE_SIZE * chunk_size as f32;
    ChunkPos {
        x: (position.x / span).floor() as i64,
        y: (position.y / span).floor() as i64,
    }
}

pub(crate) fn tile_to_world(tile: TilePos) -> Vec2 {
    Vec2::new(tile.x as f32, tile.y as f32) * TILE_SIZE
}

/// Maps between game space and generator space. The generator's world spans chunks
/// `0..world_size_chunks`; the game world is centered, so its middle is chunk (0, 0)
/// and world position (0, 0). Only chunk generation sees generator coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorldFrame {
    half_chunks: i64,
    chunk_size: i64,
}

impl WorldFrame {
    pub fn new(config: WorldGeneratorConfig) -> Self {
        Self {
            half_chunks: i64::from(config.world_size_chunks / 2),
            chunk_size: i64::from(config.chunk_size),
        }
    }

    pub fn to_generator(self, pos: ChunkPos) -> ChunkPos {
        ChunkPos {
            x: pos.x + self.half_chunks,
            y: pos.y + self.half_chunks,
        }
    }

    pub fn chunk_from_generator(self, pos: ChunkPos) -> ChunkPos {
        ChunkPos {
            x: pos.x - self.half_chunks,
            y: pos.y - self.half_chunks,
        }
    }

    pub fn tile_from_generator(self, pos: TilePos) -> TilePos {
        let offset = self.half_chunks * self.chunk_size;
        TilePos {
            x: pos.x - offset,
            y: pos.y - offset,
        }
    }
}

/// Square ("Minecraft") distance between chunks.
pub(crate) fn chebyshev(a: ChunkPos, b: ChunkPos) -> u32 {
    (a.x - b.x).unsigned_abs().max((a.y - b.y).unsigned_abs()) as u32
}

pub(crate) fn square(center: ChunkPos, radius: u32) -> impl Iterator<Item = ChunkPos> {
    let r = i64::from(radius);
    (center.y - r..=center.y + r)
        .flat_map(move |y| (center.x - r..=center.x + r).map(move |x| ChunkPos { x, y }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_positions_floor_into_negative_chunks() {
        let span = TILE_SIZE * 4.0;
        assert_eq!(
            chunk_at_world(Vec2::new(-0.1, 0.0), 4),
            ChunkPos { x: -1, y: 0 }
        );
        assert_eq!(
            chunk_at_world(Vec2::new(span, span - 0.1), 4),
            ChunkPos { x: 1, y: 0 }
        );
    }

    #[test]
    fn world_frame_centers_the_generator_world() {
        let frame = WorldFrame::new(WorldGeneratorConfig {
            chunk_size: 4,
            world_size_chunks: 16,
            ..Default::default()
        });
        let middle = ChunkPos { x: 0, y: 0 };
        assert_eq!(frame.to_generator(middle), ChunkPos { x: 8, y: 8 });
        assert_eq!(
            frame.chunk_from_generator(ChunkPos { x: 0, y: 15 }),
            ChunkPos { x: -8, y: 7 }
        );
        assert_eq!(
            frame.tile_from_generator(TilePos { x: 32, y: 33 }),
            TilePos { x: 0, y: 1 }
        );
    }

    #[test]
    fn square_covers_chebyshev_radius() {
        let center = ChunkPos { x: 3, y: -2 };
        let cells: Vec<_> = square(center, 2).collect();
        assert_eq!(cells.len(), 25);
        assert!(cells.iter().all(|&c| chebyshev(c, center) <= 2));
    }
}
