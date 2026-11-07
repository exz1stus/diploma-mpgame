use std::sync::Arc;

use super::BiomeMap;
use crate::{GenerationStage, TileDraft, TilePos};

pub struct BiomeStage {
    default_map: Arc<BiomeMap>,
}

impl BiomeStage {
    pub fn new(default_map: Arc<BiomeMap>) -> Self {
        Self { default_map }
    }
}

impl GenerationStage for BiomeStage {
    fn apply(&self, _pos: TilePos, tile: &mut TileDraft) {
        tile.surface_biome = self
            .default_map
            .determine_biome(tile.temperature, tile.height);
        tile.biome = tile
            .biome_map
            .as_ref()
            .and_then(|map| map.determine_biome(tile.temperature, tile.height))
            .or(tile.surface_biome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BiomeRepository;

    #[test]
    fn feature_maps_choose_by_final_height_and_temperature_and_gaps_use_default() {
        let repository = BiomeRepository::parse();
        let feature_map = Arc::new(BiomeMap::from_ron(r#"[
            (biome: "frozen_volcano", height: (min: 0.0, max: 1.0), temperature: (min: -20.0, max: -5.0)),
            (biome: "volcano", height: (min: 0.0, max: 0.7), temperature: (min: -5.0, max: 20.0)),
            (biome: "crater", height: (min: 0.7, max: 1.0), temperature: (min: -5.0, max: 20.0)),
        ]"#, &repository).unwrap());
        let default_map = Arc::new(BiomeMap::parse(&repository));
        let stage = BiomeStage::new(Arc::clone(&default_map));
        for (height, temperature, expected) in [
            (0.5, -10.0, repository.id("frozen_volcano")),
            (0.5, 0.0, repository.id("volcano")),
            (0.8, 0.0, repository.id("crater")),
            (1.0, 0.0, repository.id("crater")),
            // No feature rule covers either of these temperature ranges.
            (0.5, -30.0, default_map.determine_biome(-30.0, 0.5)),
            (0.8, 25.0, default_map.determine_biome(25.0, 0.8)),
            (0.5, 100.0, None),
        ] {
            let mut tile = TileDraft {
                height,
                temperature,
                biome_map: Some(Arc::clone(&feature_map)),
                ..Default::default()
            };
            stage.apply(TilePos { x: 0, y: 0 }, &mut tile);
            assert_eq!(
                tile.biome, expected,
                "height={height}, temperature={temperature}"
            );
        }
    }
}
