use rand::{Rng, RngExt};

use crate::features::feature::{FeatureGenerator, TerrainFeature};
use crate::{TileDraft, TilePos};

/// A volcanic cone with a summit caldera. Vertical dimensions use normalized height.
#[derive(Clone)]
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

    fn apply(&self, pos: TilePos, tile: &mut TileDraft) {
        let x = pos.x as f64;
        let y = pos.y as f64;
        let u = (x - self.x).hypot(y - self.y) / self.radius;
        if u >= 1.0 {
            return;
        }
        // Smooth summit and foot avoid a central spike or a seam at the base.
        let cone = self.height * (1.0 - u).powi(2) * (1.0 + 2.0 * u);
        let caldera = if self.caldera_ratio > 0.0 && u < self.caldera_ratio {
            let r = u / self.caldera_ratio;
            self.caldera_depth * (1.0 - r * r).powi(2)
        } else {
            0.0
        };
        tile.height = tile.height + cone - caldera;
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
