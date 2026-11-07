use rand::{Rng, RngExt};

use crate::features::feature::{FeatureGenerator, TerrainFeature};

pub struct Crater {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub depth: f64,
    pub rim_height: f64,
}

impl TerrainFeature for Crater {
    fn center(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    fn influence_radius(&self) -> f64 {
        self.radius * 1.5
    }

    fn apply_height(&self, x: f64, y: f64, height: f64) -> f64 {
        let dx = x - self.x;
        let dy = y - self.y;

        let r = (dx * dx + dy * dy).sqrt() / self.radius;

        if r >= 1.5 {
            return height;
        }

        let bowl = -self.depth * (1.0 - r * r).max(0.0).powi(2);

        let rim = self.rim_height * (-((r - 1.0) / 0.12).powi(2)).exp();

        height + bowl + rim
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CraterGenerator {
    pub cell_size: i64,
    pub min_radius: f64,
    pub max_radius: f64,
    /// Expected craters per cell, in [0, 4]. Zero disables crater generation.
    pub density: f64,
}

impl Default for CraterGenerator {
    fn default() -> Self {
        Self {
            cell_size: 1024,
            min_radius: 32.0,
            max_radius: 512.0,
            density: 1.0,
        }
    }
}

impl FeatureGenerator for CraterGenerator {
    type Feature = Crater;

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
            let radius = rng.random_range(self.min_radius..=self.max_radius);

            craters.push(Crater {
                x: origin_x + rng.random::<f64>() * self.cell_size as f64,
                y: origin_y + rng.random::<f64>() * self.cell_size as f64,
                radius,
                depth: radius * 0.001,
                rim_height: radius * 0.0002,
            });
        }

        craters
    }
}
