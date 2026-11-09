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
}
