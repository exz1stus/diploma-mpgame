use std::sync::Arc;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use super::first_failure;
use crate::features::feature::{FeatureGenerator, TerrainFeature};
use crate::{BiomeMapDefinition, FbmNoise, NoiseConfig, NoiseSource, TileDraft, TilePos};

/// Harmonics of a basin outline: the radius varies around the rim as a sum of sines with
/// 2 to 16 waves, each weaker in proportion to its count, so outlines are irregular lobed
/// blobs with rough, coastline-like edges.
const OUTLINE_HARMONICS: usize = 15;

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
    /// Index into the generator's variants: which biome map the territory uses.
    pub variant: usize,
    /// Outline irregularity: the radius varies by up to this fraction around the rim.
    pub shape_noise: f64,
    /// Phase of each outline harmonic.
    outline: [f64; OUTLINE_HARMONICS],
    /// Edge detail: multi-octave noise displacing the normalized distance (see
    /// `BasinSettings::edge_noise`), sampled at this basin's own offset.
    edge: EdgeNoise,
    edge_offset: (f64, f64),
    /// Horizontal scale of the edge noise (`TerrainFeature::scaled`).
    edge_scale: f64,
    coarse_elevation: Arc<dyn NoiseSource>,
}

/// The edge displacement noise shared by a generator's basins.
#[derive(Clone)]
struct EdgeNoise {
    amplitude: f64,
    ridged: bool,
    noise: Option<Arc<FbmNoise>>,
}

impl EdgeNoise {
    fn new(settings: &BasinSettings) -> Self {
        let noise = (settings.edge_noise > 0.0).then(|| {
            Arc::new(FbmNoise::new(
                0x4241_5349_4e45_4447,
                NoiseConfig {
                    frequency: 1.0 / settings.edge_noise_scale,
                    octaves: settings.edge_octaves,
                    lacunarity: 2.0,
                    persistence: 0.5,
                },
            ))
        });
        Self {
            amplitude: settings.edge_noise,
            ridged: settings.edge_ridged,
            noise,
        }
    }

    /// Displacement in [-amplitude, amplitude]. Ridged noise folds the fBm into sharp creases:
    /// thin spikes and notches instead of rounded wobbles.
    fn displacement(&self, x: f64, y: f64) -> f64 {
        let Some(noise) = &self.noise else {
            return 0.0;
        };
        let value = noise.sample(x, y).clamp(-1.0, 1.0);
        let shaped = if self.ridged {
            (1.0 - 4.0 * value.abs()).clamp(-1.0, 1.0)
        } else {
            value
        };
        self.amplitude * shaped
    }
}

impl Basin {
    /// Radius scale toward a direction: 1 for a circle, within 1 ± shape_noise otherwise.
    fn outline_scale(&self, dx: f64, dy: f64) -> f64 {
        if self.shape_noise == 0.0 {
            return 1.0;
        }
        let angle = dy.atan2(dx);
        let mut sum = 0.0;
        let mut norm = 0.0;
        for (index, phase) in self.outline.iter().enumerate() {
            let waves = (index + 2) as f64;
            sum += (waves * angle + phase).sin() / waves;
            norm += 1.0 / waves;
        }
        (1.0 + self.shape_noise * sum / norm).max(0.2)
    }
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
            edge_scale: self.edge_scale * scale,
            ..self
        }
    }

    fn center(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    fn influence_radius(&self) -> f64 {
        self.radius * (1.5 + self.edge.amplitude) * (1.0 + self.shape_noise)
    }

    fn variant(&self) -> usize {
        self.variant
    }

    fn apply(&self, pos: TilePos, tile: &mut TileDraft) -> Option<f64> {
        let x = pos.x as f64;
        let y = pos.y as f64;
        let height = tile.height;
        let (dx, dy) = (x - self.x, y - self.y);
        let (ox, oy) = self.edge_offset;
        let detail = self
            .edge
            .displacement(x / self.edge_scale + ox, y / self.edge_scale + oy);
        let u = (dx.hypot(dy) / (self.radius * self.outline_scale(dx, dy)) + detail).max(0.0);
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
    /// Must be empty when `variants` are listed.
    #[serde(default)]
    pub biome_map: BiomeMapDefinition,
    /// Basin types: each basin picks one by weight and uses its biome map. Empty: every
    /// basin uses `biome_map`.
    #[serde(default)]
    pub variants: Vec<BasinVariant>,
    /// Outline irregularity in [0, 0.6]: 0 keeps basins circular.
    #[serde(default)]
    pub shape_noise: f64,
    /// Edge detail in [0, 0.5]: multi-octave noise added to the normalized distance from the
    /// centre, so the territory edge and rim get detail at every scale. 0 disables it.
    #[serde(default)]
    pub edge_noise: f64,
    /// Size of the largest edge details, in tiles.
    #[serde(default = "default_edge_noise_scale")]
    pub edge_noise_scale: f64,
    /// Octaves of edge detail, each half the size of the previous one.
    #[serde(default = "default_edge_octaves")]
    pub edge_octaves: usize,
    /// Ridged edge noise: sharp spikes and notches instead of rounded wobbles.
    #[serde(default)]
    pub edge_ridged: bool,
    /// Objects spawned only inside this feature's territory, in any biome.
    #[serde(default)]
    pub objects: Vec<crate::ObjectSpawn>,
}

fn default_edge_noise_scale() -> f64 {
    200.0
}

fn default_edge_octaves() -> usize {
    5
}

/// One basin type of `BasinSettings::variants`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasinVariant {
    pub name: String,
    /// Relative chance of this type, positive.
    pub weight: f64,
    pub biome_map: BiomeMapDefinition,
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
    edge: EdgeNoise,
}

impl BasinGenerator {
    pub fn new(settings: BasinSettings, coarse_elevation: impl NoiseSource) -> Self {
        Self {
            edge: EdgeNoise::new(&settings),
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
            (
                self.shape_noise.is_finite() && (0.0..=0.6).contains(&self.shape_noise),
                "shape_noise must be in [0, 0.6]",
            ),
            (
                self.edge_noise.is_finite() && (0.0..=0.5).contains(&self.edge_noise),
                "edge_noise must be in [0, 0.5]",
            ),
            (
                self.edge_noise_scale.is_finite() && self.edge_noise_scale > 0.0,
                "edge_noise_scale must be positive",
            ),
            (
                (1..=12).contains(&self.edge_octaves),
                "edge_octaves must be in 1..=12",
            ),
            (
                self.variants
                    .iter()
                    .all(|variant| variant.weight.is_finite() && variant.weight > 0.0),
                "variant weights must be positive",
            ),
            (
                self.variants.is_empty() || self.biome_map.0.is_empty(),
                "biome_map must be empty when variants are listed",
            ),
        ];
        first_failure(&checks)
    }

    /// The biome map of each variant, in variant order (just `biome_map` without variants).
    pub fn biome_maps(&self) -> Vec<BiomeMapDefinition> {
        if self.variants.is_empty() {
            vec![self.biome_map.clone()]
        } else {
            self.variants
                .iter()
                .map(|variant| variant.biome_map.clone())
                .collect()
        }
    }
}

impl FeatureGenerator for BasinGenerator {
    type Feature = Basin;

    fn biome_map(&self) -> BiomeMapDefinition {
        self.settings.biome_map.clone()
    }

    fn biome_maps(&self) -> Vec<BiomeMapDefinition> {
        self.settings.biome_maps()
    }

    fn cell_size(&self) -> f64 {
        self.settings.cell_size as f64
    }

    fn max_influence_radius(&self) -> f64 {
        // Outline lobes and edge detail reach beyond the circular radius.
        self.settings.max_radius
            * (1.5 + self.settings.edge_noise)
            * (1.0 + self.settings.shape_noise)
    }

    fn generate_cell(&self, cx: i64, cy: i64, rng: &mut impl Rng) -> Vec<Basin> {
        let s = &self.settings;
        if s.probability == 0.0 || rng.random::<f64>() >= s.probability {
            return Vec::new();
        }
        let x = (cx as f64 + rng.random::<f64>()) * s.cell_size as f64;
        let y = (cy as f64 + rng.random::<f64>()) * s.cell_size as f64;
        let radius = rng.random_range(s.min_radius..=s.max_radius);
        let depth = rng.random_range(s.min_depth..=s.max_depth);
        let rim_height = rng.random_range(s.min_rim_height..=s.max_rim_height);
        // Drawn after the original fields, so settings without variants or outline noise
        // keep generating the same basins.
        let outline = std::array::from_fn(|_| rng.random_range(0.0..std::f64::consts::TAU));
        let variant = pick_variant(&s.variants, rng);
        let edge_offset = (
            rng.random_range(-1.0e5..1.0e5),
            rng.random_range(-1.0e5..1.0e5),
        );
        vec![Basin {
            x,
            y,
            radius,
            depth,
            rim_height,
            smoothing: s.smoothing,
            temperature_offset: s.temperature_offset,
            variant,
            shape_noise: s.shape_noise,
            outline,
            edge: self.edge.clone(),
            edge_offset,
            edge_scale: 1.0,
            coarse_elevation: Arc::clone(&self.coarse_elevation),
        }]
    }
}

/// A variant index chosen by weight; 0 without variants.
fn pick_variant(variants: &[BasinVariant], rng: &mut impl Rng) -> usize {
    let total: f64 = variants.iter().map(|variant| variant.weight).sum();
    if variants.len() < 2 || total <= 0.0 {
        return 0;
    }
    let mut roll = rng.random::<f64>() * total;
    for (index, variant) in variants.iter().enumerate() {
        if roll < variant.weight {
            return index;
        }
        roll -= variant.weight;
    }
    variants.len() - 1
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    struct Flat;
    impl NoiseSource for Flat {
        fn sample(&self, _x: f64, _y: f64) -> f64 {
            0.0
        }
    }

    fn generator(shape_noise: f64, weights: &[f64]) -> BasinGenerator {
        let settings = BasinSettings {
            probability: 1.0,
            shape_noise,
            variants: weights
                .iter()
                .enumerate()
                .map(|(index, &weight)| BasinVariant {
                    name: format!("v{index}"),
                    weight,
                    biome_map: BiomeMapDefinition::default(),
                })
                .collect(),
            biome_map: BiomeMapDefinition::default(),
            ..BasinSettings::default()
        };
        BasinGenerator::new(settings, Flat)
    }

    fn basins(generator: &BasinGenerator, count: i64) -> Vec<Basin> {
        (0..count)
            .flat_map(|cell| {
                let mut rng = ChaCha8Rng::seed_from_u64(cell as u64);
                generator.generate_cell(cell, 0, &mut rng)
            })
            .collect()
    }

    #[test]
    fn basins_pick_variants_by_weight() {
        let picked = basins(&generator(0.0, &[3.0, 1.0]), 400);
        let second = picked.iter().filter(|basin| basin.variant == 1).count();
        assert!((60..140).contains(&second), "about a quarter: {second}");
        assert!(
            basins(&generator(0.0, &[]), 20)
                .iter()
                .all(|basin| basin.variant == 0)
        );
    }

    #[test]
    fn outline_noise_makes_basins_irregular_and_zero_keeps_circles() {
        let circle = basins(&generator(0.0, &[]), 1).remove(0);
        let blob = basins(&generator(0.4, &[]), 1).remove(0);
        let scales: Vec<f64> = (0..64)
            .map(|step| {
                let angle = step as f64 / 64.0 * std::f64::consts::TAU;
                blob.outline_scale(angle.cos(), angle.sin())
            })
            .collect();
        let (low, high) = scales
            .iter()
            .fold((f64::MAX, f64::MIN), |(low, high), &scale| {
                (low.min(scale), high.max(scale))
            });
        assert!(
            high - low > 0.2,
            "radius varies around the rim: {low}..{high}"
        );
        assert!(low >= 0.6 && high <= 1.4, "within 1 ± shape_noise");
        assert_eq!(circle.outline_scale(1.0, 0.0), 1.0);
        assert_eq!(circle.outline_scale(-0.3, 0.8), 1.0);
        // Edge detail varies the distance from tile to tile along the rim.
        let mut detailed = generator(0.0, &[]);
        detailed.settings.edge_noise = 0.15;
        detailed.edge = EdgeNoise::new(&detailed.settings);
        let rough = basins(&detailed, 1).remove(0);
        let mut tile = TileDraft::default();
        let samples: Vec<f64> = (0..200)
            .map(|step| {
                let angle = step as f64 / 200.0 * std::f64::consts::TAU;
                let pos = TilePos {
                    x: (rough.x + angle.cos() * rough.radius * 1.2) as i64,
                    y: (rough.y + angle.sin() * rough.radius * 1.2) as i64,
                };
                rough.apply(pos, &mut tile).unwrap()
            })
            .collect();
        let jumps = samples
            .windows(2)
            .filter(|pair| (pair[1] - pair[0]).abs() > 0.01)
            .count();
        assert!(jumps > 40, "detail along the rim: {jumps} changes");
        assert!(
            samples
                .iter()
                .all(|u| (1.2 - 0.15 - 0.02..1.2 + 0.15 + 0.02).contains(u))
        );
        // The same position and size as without outline noise: only the shape changes.
        assert_eq!(
            (circle.x, circle.y, circle.radius),
            (blob.x, blob.y, blob.radius)
        );
    }
}
