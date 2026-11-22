use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::hash::hash_cell;
use crate::{BiomeMapDefinition, TileDraft, TilePos, WorldBounds};

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub trait TerrainFeature: Send + Sync {
    /// Center in world coordinates.
    fn center(&self) -> (f64, f64);

    /// Maximum distance at which this feature can
    /// influence terrain.
    fn influence_radius(&self) -> f64;

    /// Which of the generator's biome maps (`FeatureGenerator::biome_maps`) this feature's
    /// territory uses.
    fn variant(&self) -> usize {
        0
    }

    /// Modify terrain at a world-space position. Chunk queries may select this
    /// feature for tiles outside its influence; leave those tiles unchanged.
    /// Inside the territory, even for weak or zero changes, return the distance
    /// from the centre divided by the feature radius; outside, return `None`.
    /// Biomes and appearance are decided by later stages.
    fn apply(&self, pos: TilePos, tile: &mut TileDraft) -> Option<f64>;

    /// The same feature with its horizontal geometry (centre, radius) `scale` times
    /// larger. Heights stay, so the shape is preserved.
    fn scaled(self, scale: f64) -> Self
    where
        Self: Sized;
}

pub trait FeatureGenerator: Send + Sync {
    type Feature: TerrainFeature + Clone;

    fn biome_map(&self) -> BiomeMapDefinition {
        BiomeMapDefinition::default()
    }

    /// One biome map per feature variant (see `TerrainFeature::variant`).
    fn biome_maps(&self) -> Vec<BiomeMapDefinition> {
        vec![self.biome_map()]
    }

    /// Size of the spatial generation cells, in tiles.
    fn cell_size(&self) -> f64;

    /// Maximum possible influence radius of a feature.
    fn max_influence_radius(&self) -> f64;

    /// Generate features owned by one spatial cell.
    fn generate_cell(
        &self,
        cell_x: i64,
        cell_y: i64,
        rng: &mut impl rand::Rng,
    ) -> Vec<Self::Feature>;
}

/// A generator whose features are `scale` times larger, at proportionally spaced cells.
/// The inner generator works in configured units, so everything it derives from sizes
/// (e.g. crater depth from radius) is the same at every scale.
pub struct ScaledFeatures<G> {
    inner: G,
    scale: f64,
}

impl<G: FeatureGenerator> ScaledFeatures<G> {
    pub fn new(inner: G, scale: f64) -> Self {
        Self { inner, scale }
    }
}

impl<G: FeatureGenerator> FeatureGenerator for ScaledFeatures<G> {
    type Feature = G::Feature;

    fn biome_map(&self) -> BiomeMapDefinition {
        self.inner.biome_map()
    }

    fn biome_maps(&self) -> Vec<BiomeMapDefinition> {
        self.inner.biome_maps()
    }

    fn cell_size(&self) -> f64 {
        self.inner.cell_size() * self.scale
    }

    fn max_influence_radius(&self) -> f64 {
        self.inner.max_influence_radius() * self.scale
    }

    fn generate_cell(
        &self,
        cell_x: i64,
        cell_y: i64,
        rng: &mut impl rand::Rng,
    ) -> Vec<Self::Feature> {
        self.inner
            .generate_cell(cell_x, cell_y, rng)
            .into_iter()
            .map(|feature| feature.scaled(self.scale))
            .collect()
    }
}

/// Generated features keyed by spatial cell coordinates.
type CellFeatureCache<F> = Mutex<HashMap<(i64, i64), Arc<[F]>>>;

pub struct FeatureLayer<G: FeatureGenerator> {
    pub seed: u64,
    pub generator: G,
    features_cache: CellFeatureCache<G::Feature>,
}

impl<G: FeatureGenerator> FeatureLayer<G> {
    pub fn new(seed: u64, generator: G) -> Self {
        Self {
            seed,
            generator,
            features_cache: Mutex::new(HashMap::new()),
        }
    }

    fn cell_features(&self, cx: i64, cy: i64) -> Arc<[G::Feature]> {
        let cache = self.features_cache.lock().expect("feature cache poisoned");
        if let Some(features) = cache.get(&(cx, cy)) {
            return Arc::clone(features);
        }
        drop(cache);
        let seed = hash_cell(self.seed, cx, cy);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let generated: Arc<[G::Feature]> = self.generator.generate_cell(cx, cy, &mut rng).into();
        let mut cache = self.features_cache.lock().expect("feature cache poisoned");
        Arc::clone(cache.entry((cx, cy)).or_insert(generated))
    }

    pub fn features_in(&self, bounds: WorldBounds) -> Vec<G::Feature> {
        let size = self.generator.cell_size();
        let padding = self.generator.max_influence_radius();

        let min_x = ((bounds.min_x - padding) / size).floor() as i64;
        let min_y = ((bounds.min_y - padding) / size).floor() as i64;

        let max_x = ((bounds.max_x + padding) / size).floor() as i64;
        let max_y = ((bounds.max_y + padding) / size).floor() as i64;

        let mut result: Vec<G::Feature> = Vec::new();

        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                let features = self.cell_features(cx, cy);

                for feature in features.iter() {
                    let (x, y) = feature.center();

                    if bounds.intersects_circle(x, y, feature.influence_radius()) {
                        result.push(feature.clone());
                    }
                }
            }
        }

        result
    }
}
