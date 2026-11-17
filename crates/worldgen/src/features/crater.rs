use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use super::first_failure;
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

    fn apply(&self, pos: TilePos, tile: &mut TileDraft) -> Option<f64> {
        let x = pos.x as f64;
        let y = pos.y as f64;
        let dx = x - self.x;
        let dy = y - self.y;

        let r = (dx * dx + dy * dy).sqrt() / self.radius;

        if r >= 1.5 {
            return None;
        }

        let bowl = -self.depth * (1.0 - r * r).max(0.0).powi(2);

        let rim = self.rim_height * (-((r - 1.0) / 0.12).powi(2)).exp();

        tile.height = tile.height + bowl + rim;
        tile.temperature_offset += self.temperature_offset * (1.0 - r / 1.5).powi(2);
        Some(r)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraterGenerator {
    pub cell_size: i64,
    pub min_radius: f64,
    pub max_radius: f64,
    /// Expected craters per cell, in [0, 4]. Zero disables crater generation.
    pub density: f64,
    pub temperature_offset: f64,
    /// Biome rules for the territory, evaluated on final height and temperature.
    /// Empty or omitted: the feature only shapes terrain and keeps the terrain biome.
    #[serde(default)]
    pub biome_map: BiomeMapDefinition,
    /// Objects spawned only inside this feature's territory, in any biome.
    #[serde(default)]
    pub objects: Vec<crate::ObjectSpawn>,
}

impl Default for CraterGenerator {
    fn default() -> Self {
        crate::config::parse_ron(crate::ConfigFile::Craters.bundled())
            .expect("bundled crater settings must be valid")
    }
}

impl CraterGenerator {
    pub(crate) fn validate(&self) -> Result<(), String> {
        first_failure(&[
            (self.cell_size > 0, "cell_size must be positive"),
            (
                self.min_radius.is_finite()
                    && self.min_radius > 0.0
                    && self.max_radius.is_finite()
                    && self.max_radius >= self.min_radius,
                "radii must satisfy 0 < min_radius <= max_radius",
            ),
            (
                self.density.is_finite() && (0.0..=4.0).contains(&self.density),
                "density must be in [0, 4]",
            ),
            (
                self.temperature_offset.is_finite(),
                "temperature_offset must be finite",
            ),
        ])
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
