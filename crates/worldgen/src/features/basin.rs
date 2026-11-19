use std::sync::Arc;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use super::first_failure;
use crate::features::feature::{FeatureGenerator, TerrainFeature};
use crate::{BiomeMapDefinition, NoiseConfig, NoiseSource, TileDraft, TilePos};

/// A broad, shallow impact with a worn rim and reduced interior terrain detail.
/// Height, depth, and rim height use the normalized terrain-height scale.
#[derive(Clone)]
pub struct Basin {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub depth: f64,
    pub rim_height: f64,
    pub smoothing: f64,
    pub temperature_offset: f64,
    coarse_elevation: Arc<dyn NoiseSource>,
}

fn smoothstep(start: f64, end: f64, value: f64) -> f64 {
    let t = ((value - start) / (end - start)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl TerrainFeature for Basin {
    fn scaled(self, scale: f64) -> Self {
        Self {
            x: self.x * scale,
            y: self.y * scale,
            radius: self.radius * scale,
            ..self
        }
    }

    fn center(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    fn influence_radius(&self) -> f64 {
        self.radius * 1.5
    }

    fn apply(&self, pos: TilePos, tile: &mut TileDraft) -> Option<f64> {
        let x = pos.x as f64;
        let y = pos.y as f64;
        let height = tile.height;
        let u = (x - self.x).hypot(y - self.y) / self.radius;
        if u >= 1.5 {
            return None;
        }

        let bowl = (1.0 - u * u).max(0.0).powi(2);
        let mut smoothed = height;
        if bowl > 0.0 && self.smoothing > 0.0 {
            let coarse_height = (self.coarse_elevation.sample(x, y) + 1.0) * 0.5;
            smoothed += (coarse_height - height) * self.smoothing * bowl;
        }

        let edge = 1.0 - smoothstep(1.0, 1.5, u);
        let depression = -self.depth * bowl;
        let rim = self.rim_height * (-((u - 1.0) / 0.3).powi(2)).exp();

        // Fade the entire profile out so both height and slope join the terrain
        // continuously at the influence boundary. Clamp only after all layers.
        tile.height = smoothed + edge * (depression + rim);
        tile.temperature_offset += self.temperature_offset * bowl;
        Some(u)
    }
}

/// `features/basins.rs` settings. Probability 0 disables basins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasinSettings {
    pub cell_size: i64,
    /// Probability of one basin per generation cell, in [0, 1].
    pub probability: f64,
    pub min_radius: f64,
    pub max_radius: f64,
    pub min_depth: f64,
    pub max_depth: f64,
    pub min_rim_height: f64,
    pub max_rim_height: f64,
    /// Fraction of fine terrain detail removed at the center, in [0, 1].
    pub smoothing: f64,
    /// Octaves of the elevation noise that forms the smoothed basin floor.
    pub smoothing_octaves: usize,
    pub temperature_offset: f64,
    /// Biome rules for the territory, evaluated on final height and temperature.
    /// Empty or omitted: the feature only shapes terrain and keeps the terrain biome.
    #[serde(default)]
    pub biome_map: BiomeMapDefinition,
    /// Objects spawned only inside this feature's territory, in any biome.
    #[serde(default)]
    pub objects: Vec<crate::ObjectSpawn>,
}

impl Default for BasinSettings {
    fn default() -> Self {
        crate::config::parse_ron(crate::ConfigFile::Basins.bundled())
            .expect("bundled basin settings must be valid")
    }
}

/// Basin settings plus the coarse elevation the basin floors are smoothed towards.
#[derive(Clone)]
pub struct BasinGenerator {
    pub settings: BasinSettings,
    coarse_elevation: Arc<dyn NoiseSource>,
}

impl BasinGenerator {
    pub fn new(settings: BasinSettings, coarse_elevation: impl NoiseSource) -> Self {
        Self {
            settings,
            coarse_elevation: Arc::new(coarse_elevation),
        }
    }
}

impl BasinSettings {
    /// The coarse elevation noise basin floors are smoothed towards.
    pub fn floor_noise(&self, elevation: NoiseConfig) -> NoiseConfig {
        NoiseConfig {
            octaves: self.smoothing_octaves.min(elevation.octaves),
            ..elevation
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let checks = [
            (self.cell_size > 0, "cell_size must be positive"),
            (
                self.probability.is_finite() && (0.0..=1.0).contains(&self.probability),
                "probability must be in [0, 1]",
            ),
            (
                self.min_radius.is_finite()
                    && self.min_radius > 0.0
                    && self.max_radius.is_finite()
                    && self.max_radius >= self.min_radius,
                "radii must satisfy 0 < min_radius <= max_radius",
            ),
            (
                self.min_depth.is_finite()
                    && self.min_depth >= 0.0
                    && self.max_depth.is_finite()
                    && self.max_depth >= self.min_depth,
                "depths must satisfy 0 <= min_depth <= max_depth",
            ),
            (
                self.min_rim_height.is_finite()
                    && self.min_rim_height >= 0.0
                    && self.max_rim_height.is_finite()
                    && self.max_rim_height >= self.min_rim_height,
                "rim heights must satisfy 0 <= min_rim_height <= max_rim_height",
            ),
            (
                self.smoothing.is_finite() && (0.0..=1.0).contains(&self.smoothing),
                "smoothing must be in [0, 1]",
            ),
            (
                self.smoothing_octaves > 0,
                "smoothing_octaves must be positive",
            ),
            (
                self.temperature_offset.is_finite(),
                "temperature_offset must be finite",
            ),
        ];
        first_failure(&checks)
    }
}

impl FeatureGenerator for BasinGenerator {
    type Feature = Basin;

    fn biome_map(&self) -> BiomeMapDefinition {
        self.settings.biome_map.clone()
    }

    fn cell_size(&self) -> f64 {
        self.settings.cell_size as f64
    }

    fn max_influence_radius(&self) -> f64 {
        self.settings.max_radius * 1.5
    }

    fn generate_cell(&self, cx: i64, cy: i64, rng: &mut impl Rng) -> Vec<Basin> {
        let s = &self.settings;
        if s.probability == 0.0 || rng.random::<f64>() >= s.probability {
            return Vec::new();
        }
        vec![Basin {
            x: (cx as f64 + rng.random::<f64>()) * s.cell_size as f64,
            y: (cy as f64 + rng.random::<f64>()) * s.cell_size as f64,
            radius: rng.random_range(s.min_radius..=s.max_radius),
            depth: rng.random_range(s.min_depth..=s.max_depth),
            rim_height: rng.random_range(s.min_rim_height..=s.max_rim_height),
            smoothing: s.smoothing,
            temperature_offset: s.temperature_offset,
            coarse_elevation: Arc::clone(&self.coarse_elevation),
        }]
    }
}
