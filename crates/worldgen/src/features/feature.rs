use crate::features::hash::hash_cell;

pub trait TerrainFeature: Send + Sync {
    /// Center in world coordinates.
    fn center(&self) -> (f64, f64);

    /// Maximum distance at which this feature can
    /// influence terrain.
    fn influence_radius(&self) -> f64;

    /// Modify terrain at a world-space position.
    fn apply_height(&self, x: f64, y: f64, height: f64) -> f64;
}

pub trait FeatureGenerator: Send + Sync {
    type Feature: TerrainFeature;

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

pub struct FeatureLayer<G: FeatureGenerator> {
    pub seed: u64,
    pub generator: G,
}

impl<G: FeatureGenerator> FeatureLayer<G> {
    pub fn features_in(&self, bounds: WorldBounds) -> Vec<G::Feature> {
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let size = self.generator.cell_size();
        let padding = self.generator.max_influence_radius();

        let min_x = ((bounds.min_x - padding) / size as f64).floor() as i64;
        let min_y = ((bounds.min_y - padding) / size as f64).floor() as i64;

        let max_x = ((bounds.max_x + padding) / size as f64).floor() as i64;
        let max_y = ((bounds.max_y + padding) / size as f64).floor() as i64;

        let mut result = Vec::new();

        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                let cell_seed = hash_cell(self.seed, cx, cy);

                let mut rng = ChaCha8Rng::seed_from_u64(cell_seed);

                let features = self.generator.generate_cell(cx, cy, &mut rng);

                for feature in features {
                    let (x, y) = feature.center();

                    if bounds.intersects_circle(x, y, feature.influence_radius()) {
                        result.push(feature);
                    }
                }
            }
        }

        result
    }
}
