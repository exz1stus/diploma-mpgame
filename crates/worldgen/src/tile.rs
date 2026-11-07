use crate::BiomeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TilePos {
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i64,
    pub y: i64,
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

#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
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
    pub max_temperature: f64,
    pub biome_override: Option<BiomeId>,
    pub biome: Option<BiomeId>,
}

impl TileDraft {
    /// Finish a tile after all stages. A feature biome takes precedence.
    /// Panics if neither a feature nor the biome map assigned a biome.
    pub fn finish(&self) -> Tile {
        Tile {
            height: self.height,
            temperature: self.temperature,
            biome: self
                .biome_override
                .or(self.biome)
                .expect("no biome covers the generated tile"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_prefers_feature_biome() {
        let classified = BiomeId(0);
        let mut draft = TileDraft {
            biome: Some(classified),
            ..Default::default()
        };
        assert_eq!(draft.finish().biome, classified);
        let feature = BiomeId(1);
        draft.biome_override = Some(feature);
        assert_eq!(draft.finish().biome, feature);
    }

    #[test]
    #[should_panic(expected = "no biome covers the generated tile")]
    fn final_tiles_require_a_biome() {
        TileDraft::default().finish();
    }
}
