use crate::{BiomeId, BiomeMap, FeatureMark};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TilePos {
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TileData {
    /// Normalized elevation after terrain features, in [0, 1].
    pub height: f64,
    /// Daily average Celsius temperature for the configured season.
    pub temperature: f64,
    pub biome: BiomeId,
    /// The biome the terrain map gives the final height and temperature. Equals `biome`
    /// outside feature territories; inside them it names the surrounding ground.
    pub ground: BiomeId,
    /// Feature territory containing the tile, for placement rules. Not stored in chunks.
    pub feature: Option<FeatureMark>,
}

/// What a chunk stores per tile: the biomes and the final height, quantized to u16.
/// Appearance (textures, tint, relief) is derived from this by the consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileSample {
    /// The tile's biome; decides which objects spawn.
    pub biome: BiomeId,
    /// The terrain biome under a feature biome (see [`TileData::ground`]), for consumers
    /// that draw feature territories like the surrounding terrain.
    pub ground: BiomeId,
    /// Normalized height × `u16::MAX`; read it with [`TileSample::height`].
    pub height: u16,
}

impl TileSample {
    /// A tile outside feature territories: its ground is its biome.
    pub fn new(biome: BiomeId, height: f64) -> Self {
        Self::on_ground(biome, biome, height)
    }

    pub fn on_ground(biome: BiomeId, ground: BiomeId, height: f64) -> Self {
        Self {
            biome,
            ground,
            height: (height.clamp(0.0, 1.0) * f64::from(u16::MAX)).round() as u16,
        }
    }

    pub fn from_data(data: &TileData) -> Self {
        Self::on_ground(data.biome, data.ground, data.height)
    }

    /// Normalized height in [0, 1].
    pub fn height(self) -> f32 {
        f32::from(self.height) / f32::from(u16::MAX)
    }
}
#[derive(Default)]
pub struct TileDraft {
    pub height: f64,
    pub temperature: f64,
    /// Feature heating/cooling added to climate temperature after terrain changes.
    pub temperature_offset: f64,
    /// Selected feature map; only the biome stage evaluates its rules.
    pub biome_map: Option<Arc<BiomeMap>>,
    pub biome: Option<BiomeId>,
    /// Terrain-map biome; defaults to `biome` when no stage sets it.
    pub ground: Option<BiomeId>,
    /// Last feature whose territory contains the tile.
    pub feature: Option<FeatureMark>,
}

impl TileDraft {
    /// Finish a tile after all stages. Only biome stages assign the final biome.
    /// Panics if no biome stage assigned a biome.
    pub fn finish(&self) -> TileData {
        self.try_finish()
            .expect("no biome covers the generated tile")
    }

    /// The finished tile, or `None` if no biome stage assigned a biome.
    pub fn try_finish(&self) -> Option<TileData> {
        let biome = self.biome?;
        Some(TileData {
            height: self.height,
            temperature: self.temperature,
            biome,
            ground: self.ground.unwrap_or(biome),
            feature: self.feature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_finishes_with_the_classified_biome() {
        let classified = BiomeId(0);
        let draft = TileDraft {
            biome: Some(classified),
            ..Default::default()
        };
        assert_eq!(draft.finish().biome, classified);
    }

    #[test]
    #[should_panic(expected = "no biome covers the generated tile")]
    fn final_tiles_require_a_biome() {
        TileDraft::default().finish();
    }

    #[test]
    fn samples_quantize_height_over_the_unit_range() {
        let biome = BiomeId(3);
        assert_eq!(TileSample::new(biome, 0.0).height, 0);
        assert_eq!(TileSample::new(biome, 1.0).height, u16::MAX);
        assert_eq!(TileSample::new(biome, 1.5).height, u16::MAX);
        let sample = TileSample::new(biome, 0.3);
        assert!((sample.height() - 0.3).abs() < 1e-4);
        assert_eq!(sample.biome, biome);
    }
}
