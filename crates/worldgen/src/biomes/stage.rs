use std::sync::Arc;

use super::BiomeMap;
use crate::{GenerationStage, TileDraft, TilePos};

pub struct BiomeStage {
    biomes: Arc<BiomeMap>,
}

impl BiomeStage {
    pub fn new(biomes: Arc<BiomeMap>) -> Self {
        Self { biomes }
    }
}

impl GenerationStage for BiomeStage {
    fn apply(&self, _pos: TilePos, tile: &mut TileDraft) {
        tile.biome = tile
            .biome_override
            .or_else(|| self.biomes.determine_biome(tile.temperature, tile.height));
    }
}
