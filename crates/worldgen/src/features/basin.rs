use std::sync::Arc;

use rand::{Rng, RngExt};

use crate::NoiseSource;
use crate::features::feature::{FeatureGenerator, TerrainFeature};

/// A broad, shallow impact with a worn rim and reduced interior terrain detail.
/// Height, depth, and rim height use the normalized terrain-height scale.
pub struct Basin {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub depth: f64,
    pub rim_height: f64,
    pub smoothing: f64,
    coarse_elevation: Arc<dyn NoiseSource>,
}

fn smoothstep(start: f64, end: f64, value: f64) -> f64 {
    let t = ((value - start) / (end - start)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl TerrainFeature for Basin {
    fn center(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    fn influence_radius(&self) -> f64 {
        self.radius * 1.5
    }

    fn apply_height(&self, x: f64, y: f64, height: f64) -> f64 {
        let u = (x - self.x).hypot(y - self.y) / self.radius;
        if u >= 1.5 {
            return height;
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
        smoothed + edge * (depression + rim)
    }
}

#[derive(Clone)]
pub struct BasinGenerator {
    pub cell_size: i64,
    pub min_radius: f64,
    pub max_radius: f64,
    /// Probability of one basin per generation cell, in [0, 1].
    pub probability: f64,
    pub min_depth: f64,
    pub max_depth: f64,
    pub min_rim_height: f64,
    pub max_rim_height: f64,
    /// Fraction of fine terrain detail removed at the center, in [0, 1].
    pub smoothing: f64,
    coarse_elevation: Arc<dyn NoiseSource>,
}

impl BasinGenerator {
    pub fn new(coarse_elevation: impl NoiseSource) -> Self {
        Self {
            cell_size: 8192,
            min_radius: 800.0,
            max_radius: 2400.0,
            probability: 0.12,
            min_depth: 0.06,
            max_depth: 0.18,
            min_rim_height: 0.005,
            max_rim_height: 0.02,
            smoothing: 0.8,
            coarse_elevation: Arc::new(coarse_elevation),
        }
    }
}

impl FeatureGenerator for BasinGenerator {
    type Feature = Basin;

    fn cell_size(&self) -> i64 {
        assert!(self.cell_size > 0, "basin cell size must be positive");
        self.cell_size
    }

    fn max_influence_radius(&self) -> f64 {
        assert!(self.max_radius.is_finite() && self.max_radius > 0.0);
        self.max_radius * 1.5
    }

    fn generate_cell(&self, cx: i64, cy: i64, rng: &mut impl Rng) -> Vec<Basin> {
        assert!(self.cell_size > 0, "basin cell size must be positive");
        assert!(self.min_radius.is_finite() && self.min_radius > 0.0);
        assert!(self.max_radius.is_finite() && self.max_radius >= self.min_radius);
        assert!(self.probability.is_finite() && (0.0..=1.0).contains(&self.probability));
        assert!(self.smoothing.is_finite() && (0.0..=1.0).contains(&self.smoothing));
        assert!(self.min_depth.is_finite() && self.min_depth >= 0.0);
        assert!(self.max_depth.is_finite() && self.max_depth >= self.min_depth);
        assert!(self.min_rim_height.is_finite() && self.min_rim_height >= 0.0);
        assert!(self.max_rim_height.is_finite() && self.max_rim_height >= self.min_rim_height);

        if self.probability == 0.0 || rng.random::<f64>() >= self.probability {
            return Vec::new();
        }

        vec![Basin {
            x: (cx as f64 + rng.random::<f64>()) * self.cell_size as f64,
            y: (cy as f64 + rng.random::<f64>()) * self.cell_size as f64,
            radius: rng.random_range(self.min_radius..=self.max_radius),
            depth: rng.random_range(self.min_depth..=self.max_depth),
            rim_height: rng.random_range(self.min_rim_height..=self.max_rim_height),
            smoothing: self.smoothing,
            coarse_elevation: Arc::clone(&self.coarse_elevation),
        }]
    }
}
