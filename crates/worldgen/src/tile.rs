use crate::{BiomeId, BiomeMap};
use std::sync::Arc;

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
    /// Biome supplying the appearance, which can differ from feature territory.
    pub surface_biome: BiomeId,
    /// Index into `surface_biome`'s palette, shared by preview and game renderers.
    pub palette_index: u16,
}

#[derive(Default)]
pub struct TileDraft {
    pub height: f64,
    /// Terrain height immediately before the feature marking this territory.
    pub environment_height: f64,
    pub temperature: f64,
    /// Feature heating/cooling added to climate temperature after terrain changes.
    pub temperature_offset: f64,
    /// Selected feature map; only the biome stage evaluates its rules.
    pub biome_map: Option<Arc<BiomeMap>>,
    pub biome: Option<BiomeId>,
    pub surface_biome: Option<BiomeId>,
    pub palette_index: u16,
}

impl TileDraft {
    /// Finish a tile after all stages. Only biome stages assign the final biome.
    /// Panics if no biome stage assigned a biome.
    pub fn finish(&self) -> Tile {
        Tile {
            height: self.height,
            temperature: self.temperature,
            biome: self.biome.expect("no biome covers the generated tile"),
            surface_biome: self
                .surface_biome
                .expect("no appearance covers the generated tile"),
            palette_index: self.palette_index,
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
            surface_biome: Some(classified),
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
