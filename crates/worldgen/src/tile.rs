use crate::{BiomeId, BiomeMap};
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
}

/// What a chunk stores per tile: the biome and the final height, quantized to u16.
/// Appearance (textures, tint, relief) is derived from this by the consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileSample {
    pub biome: BiomeId,
    /// Normalized height × `u16::MAX`; read it with [`TileSample::height`].
    pub height: u16,
}

impl TileSample {
    pub fn new(biome: BiomeId, height: f64) -> Self {
        Self {
            biome,
            height: (height.clamp(0.0, 1.0) * f64::from(u16::MAX)).round() as u16,
        }
    }

    pub fn from_data(data: &TileData) -> Self {
        Self::new(data.biome, data.height)
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
}

impl TileDraft {
    /// Finish a tile after all stages. Only biome stages assign the final biome.
    /// Panics if no biome stage assigned a biome.
    pub fn finish(&self) -> TileData {
        TileData {
            height: self.height,
            temperature: self.temperature,
            biome: self.biome.expect("no biome covers the generated tile"),
        }
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
