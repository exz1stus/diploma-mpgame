use rand::{Rng, RngExt};

use crate::features::feature::{FeatureGenerator, TerrainFeature};

/// A volcanic cone with a summit caldera. Vertical dimensions use normalized height.
pub struct Volcano {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub height: f64,
    pub caldera_ratio: f64,
    pub caldera_depth: f64,
}

impl TerrainFeature for Volcano {
    fn center(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    fn influence_radius(&self) -> f64 {
        self.radius
    }

    fn apply_height(&self, x: f64, y: f64, height: f64) -> f64 {
        let u = (x - self.x).hypot(y - self.y) / self.radius;
        if u >= 1.0 {
            return height;
        }
        // Smooth summit and foot avoid a central spike or a seam at the base.
        let cone = self.height * (1.0 - u).powi(2) * (1.0 + 2.0 * u);
        let caldera = if self.caldera_ratio > 0.0 && u < self.caldera_ratio {
            let r = u / self.caldera_ratio;
            self.caldera_depth * (1.0 - r * r).powi(2)
        } else {
            0.0
        };
        height + cone - caldera
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VolcanoGenerator {
    pub cell_size: i64,
    pub min_radius: f64,
    pub max_radius: f64,
    /// Probability of one volcano per generation cell, in [0, 1]. Zero disables it.
    pub probability: f64,
    pub min_height: f64,
    pub max_height: f64,
    /// Caldera radius as a fraction of the volcano radius, in [0, 1).
    pub caldera_ratio: f64,
    pub caldera_depth: f64,
}

impl Default for VolcanoGenerator {
    fn default() -> Self {
        Self {
            cell_size: 4096,
            min_radius: 128.0,
            max_radius: 1024.0,
            probability: 0.15,
            min_height: 0.15,
            max_height: 0.4,
            caldera_ratio: 0.2,
            caldera_depth: 0.15,
        }
    }
}

impl VolcanoGenerator {
    pub(crate) fn validate(&self) {
        assert!(self.cell_size > 0, "volcano cell size must be positive");
        assert!(self.min_radius.is_finite() && self.min_radius > 0.0);
        assert!(self.max_radius.is_finite() && self.max_radius >= self.min_radius);
        assert!(self.probability.is_finite() && (0.0..=1.0).contains(&self.probability));
        assert!(self.min_height.is_finite() && self.min_height >= 0.0);
        assert!(self.max_height.is_finite() && self.max_height >= self.min_height);
        assert!(self.caldera_ratio.is_finite() && (0.0..1.0).contains(&self.caldera_ratio));
        assert!(self.caldera_depth.is_finite() && self.caldera_depth >= 0.0);
    }
}

impl FeatureGenerator for VolcanoGenerator {
    type Feature = Volcano;

    fn cell_size(&self) -> i64 {
        self.cell_size
    }

    fn max_influence_radius(&self) -> f64 {
        self.max_radius
    }

    fn generate_cell(&self, cx: i64, cy: i64, rng: &mut impl Rng) -> Vec<Volcano> {
        self.validate();
        if self.probability == 0.0 || rng.random::<f64>() >= self.probability {
            return Vec::new();
        }
        vec![Volcano {
            x: (cx as f64 + rng.random::<f64>()) * self.cell_size as f64,
            y: (cy as f64 + rng.random::<f64>()) * self.cell_size as f64,
            radius: rng.random_range(self.min_radius..=self.max_radius),
            height: rng.random_range(self.min_height..=self.max_height),
            caldera_ratio: self.caldera_ratio,
            caldera_depth: self.caldera_depth,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::feature::{FeatureLayer, WorldBounds};

    #[test]
    fn cone_and_caldera_join_surrounding_terrain_continuously() {
        let volcano = Volcano {
            x: 0.0,
            y: 0.0,
            radius: 100.0,
            height: 0.4,
            caldera_ratio: 0.2,
            caldera_depth: 0.25,
        };
        let base = 0.3;
        let center = volcano.apply_height(0.0, 0.0, base);
        let rim = volcano.apply_height(20.0, 0.0, base);
        assert!(rim > center && center > base);
        assert!(volcano.apply_height(50.0, 0.0, base) > base);
        assert_eq!(volcano.apply_height(100.0, 0.0, base), base);
        assert_eq!(volcano.apply_height(101.0, 0.0, base), base);
        assert!((volcano.apply_height(99.999, 0.0, base) - base).abs() < 1e-9);
        assert!((volcano.apply_height(19.999, 0.0, base) - rim).abs() < 1e-5);
        let solid = Volcano {
            caldera_ratio: 0.0,
            ..volcano
        };
        assert!((solid.apply_height(0.0, 0.0, base) - 0.7).abs() < 1e-12);
    }

    #[test]
    fn placement_is_seeded_and_includes_features_from_neighboring_cells() {
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;
        let generator = VolcanoGenerator {
            probability: 1.0,
            min_radius: 1024.0,
            ..Default::default()
        };
        let first = generator.generate_cell(0, 0, &mut ChaCha8Rng::seed_from_u64(4));
        let again = generator.generate_cell(0, 0, &mut ChaCha8Rng::seed_from_u64(4));
        assert_eq!(first[0].center(), again[0].center());
        let disabled = VolcanoGenerator {
            probability: 0.0,
            ..generator
        };
        assert!(
            disabled
                .generate_cell(0, 0, &mut ChaCha8Rng::seed_from_u64(4))
                .is_empty()
        );
        let layer = FeatureLayer {
            seed: 42,
            generator,
        };
        let bounds = WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 4096.0,
            max_y: 4096.0,
        };
        let owned = layer
            .features_in(bounds)
            .into_iter()
            .find(|feature| {
                feature.x >= 0.0 && feature.x < 4096.0 && feature.y >= 0.0 && feature.y < 4096.0
            })
            .unwrap();
        let x = owned.x + 100.0;
        let y = owned.y;
        let point = WorldBounds {
            min_x: x,
            max_x: x,
            min_y: y,
            max_y: y,
        };
        assert!(
            layer
                .features_in(point)
                .iter()
                .any(|feature| feature.center() == owned.center())
        );
        // A larger radius guarantees that neighboring generation cells contribute.
        let wide = FeatureLayer {
            seed: 42,
            generator: VolcanoGenerator {
                min_radius: 8192.0,
                max_radius: 8192.0,
                ..generator
            },
        };
        assert!(
            wide.features_in(bounds)
                .iter()
                .any(|feature| feature.x < 0.0
                    || feature.y < 0.0
                    || feature.x >= 4096.0
                    || feature.y >= 4096.0)
        );
    }
}
