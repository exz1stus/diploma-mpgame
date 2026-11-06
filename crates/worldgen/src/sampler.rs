use crate::TilePos;

use super::noise::NoiseSource;

pub trait TileSampler {
    fn sample(&self, pos: TilePos) -> f64;
}

pub struct ElevationSampler {
    noise: Box<dyn NoiseSource>,
}

impl ElevationSampler {
    pub fn new(noise: impl NoiseSource) -> Self {
        Self {
            noise: Box::new(noise),
        }
    }
}

impl TileSampler for ElevationSampler {
    fn sample(&self, pos: TilePos) -> f64 {
        self.noise.sample(pos.x as f64, pos.y as f64)
    }
}
