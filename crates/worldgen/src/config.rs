use crate::{GenerationError, TilePos, TileRect};

#[derive(Clone, Copy, Debug)]
pub struct WorldGeneratorConfig {
    pub generator_version: u32,
    pub seed: u64,
    pub world_size_chunks: u32,
    pub chunk_size: u32,
}

impl Default for WorldGeneratorConfig {
    fn default() -> Self {
        Self {
            generator_version: 1,
            seed: 0,
            world_size_chunks: 500,
            chunk_size: 32,
        }
    }
}
impl WorldGeneratorConfig {
    pub fn tile_bounds(self) -> Result<TileRect, GenerationError> {
        let side = self
            .world_size_chunks
            .checked_mul(self.chunk_size)
            .filter(|side| *side > 0)
            .ok_or(GenerationError::InvalidRegion)?;
        Ok(TileRect {
            min: TilePos { x: 0, y: 0 },
            max: TilePos {
                x: i64::from(side),
                y: i64::from(side),
            },
        })
    }
}
