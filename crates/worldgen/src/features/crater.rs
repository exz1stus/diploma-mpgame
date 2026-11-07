use rand::{Rng, RngExt};
use serde::Deserialize;

use crate::features::feature::{FeatureGenerator, TerrainFeature};
use crate::{BiomeMapDefinition, TileDraft, TilePos};

#[derive(Clone)]
pub struct Crater {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub depth: f64,
    pub rim_height: f64,
    pub temperature_offset: f64,
}

impl TerrainFeature for Crater {
    fn center(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    fn influence_radius(&self) -> f64 {
        self.radius * 1.5
    }

    fn apply(&self, pos: TilePos, tile: &mut TileDraft) -> bool {
        let x = pos.x as f64;
        let y = pos.y as f64;
        let dx = x - self.x;
        let dy = y - self.y;

        let r = (dx * dx + dy * dy).sqrt() / self.radius;

        if r >= 1.5 {
            return false;
        }

        let bowl = -self.depth * (1.0 - r * r).max(0.0).powi(2);

        let rim = self.rim_height * (-((r - 1.0) / 0.12).powi(2)).exp();

        tile.height = tile.height + bowl + rim;
        tile.temperature_offset += self.temperature_offset * (1.0 - r / 1.5).powi(2);
        true
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraterGenerator {
    pub cell_size: i64,
    pub min_radius: f64,
    pub max_radius: f64,
    /// Expected craters per cell, in [0, 4]. Zero disables crater generation.
    pub density: f64,
    pub temperature_offset: f64,
    pub biome_map: BiomeMapDefinition,
}

impl Default for CraterGenerator {
    fn default() -> Self {
        Self::from_ron(include_str!("crater.ron")).expect("bundled crater definition must be valid")
    }
}

impl CraterGenerator {
    pub fn from_ron(source: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(source)
    }
}

impl FeatureGenerator for CraterGenerator {
    type Feature = Crater;

    fn biome_map(&self) -> BiomeMapDefinition {
        self.biome_map.clone()
    }

    fn cell_size(&self) -> i64 {
        self.cell_size
    }

    fn max_influence_radius(&self) -> f64 {
        self.max_radius * 1.5
    }

    fn generate_cell(&self, cx: i64, cy: i64, rng: &mut impl Rng) -> Vec<Crater> {
        let count = (0..4)
            .filter(|_| rng.random::<f64>() < self.density / 4.0)
            .count();
        let mut craters = Vec::with_capacity(count);

        let origin_x = cx as f64 * self.cell_size as f64;
        let origin_y = cy as f64 * self.cell_size as f64;

        for _ in 0..count {
            // Small impacts are common; large bowls remain occasional landmarks.
            let radius =
                self.min_radius + (self.max_radius - self.min_radius) * rng.random::<f64>().powi(2);

            craters.push(Crater {
                x: origin_x + rng.random::<f64>() * self.cell_size as f64,
                y: origin_y + rng.random::<f64>() * self.cell_size as f64,
                radius,
                // Keep large impacts from flattening the floor at the world
                // height clamp or producing disproportionately tall rims.
                depth: (radius * 0.001).min(0.22),
                rim_height: (radius * 0.0002).min(0.06),
                temperature_offset: self.temperature_offset,
            });
        }

        craters
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn impacts_favor_small_craters_and_bound_large_vertical_changes() {
        let generator = CraterGenerator {
            density: 4.0,
            ..Default::default()
        };
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let midpoint = (generator.min_radius + generator.max_radius) * 0.5;
        let mut small = 0;
        let mut total = 0;
        for x in 0..128 {
            for crater in generator.generate_cell(x, 0, &mut rng) {
                assert!((generator.min_radius..=generator.max_radius).contains(&crater.radius));
                assert!(crater.depth <= 0.22);
                assert!(crater.rim_height <= 0.06);
                small += usize::from(crater.radius < midpoint);
                total += 1;
            }
        }
        assert!(
            small > total * 3 / 5,
            "small impacts should be more common than large impacts"
        );
    }
}
