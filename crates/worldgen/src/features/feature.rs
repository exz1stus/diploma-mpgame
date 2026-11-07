use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::features::hash::hash_cell;
use crate::{TileDraft, TilePos, WorldBounds};

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub trait TerrainFeature: Send + Sync {
    /// Center in world coordinates.
    fn center(&self) -> (f64, f64);

    /// Maximum distance at which this feature can
    /// influence terrain.
    fn influence_radius(&self) -> f64;

    /// Modify terrain at a world-space position. Chunk queries may select this
    /// feature for tiles outside its influence; leave those tiles unchanged.
    fn apply(&self, pos: TilePos, tile: &mut TileDraft);
}

pub trait FeatureGenerator: Send + Sync {
    type Feature: TerrainFeature + Clone;

    /// Size of the spatial generation cells.
    fn cell_size(&self) -> i64;

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

pub struct FeatureLayer<G: FeatureGenerator> {
    pub seed: u64,
    pub generator: G,
    features_cache: Mutex<HashMap<(i64, i64), Arc<[G::Feature]>>>,
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
        let mut cache = self.features_cache.lock().expect("feature cache poisoned");

        let features = cache.entry((cx, cy)).or_insert_with(|| {
            let seed = hash_cell(self.seed, cx, cy);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);

            let generated: Arc<[G::Feature]> =
                self.generator.generate_cell(cx, cy, &mut rng).into();

            generated
        });

        Arc::clone(features)
    }

    pub fn features_in(&self, bounds: WorldBounds) -> Vec<G::Feature> {
        let size = self.generator.cell_size();
        let padding = self.generator.max_influence_radius();

        let min_x = ((bounds.min_x - padding) / size as f64).floor() as i64;
        let min_y = ((bounds.min_y - padding) / size as f64).floor() as i64;

        let max_x = ((bounds.max_x + padding) / size as f64).floor() as i64;
        let max_y = ((bounds.max_y + padding) / size as f64).floor() as i64;

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
