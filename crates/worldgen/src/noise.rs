use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

/// A 2D scalar field in roughly [-1, 1]. Implement it to plug custom terrain into a generator.
pub trait NoiseSource: Send + Sync + 'static {
    fn sample(&self, x: f64, y: f64) -> f64;
}

impl NoiseSource for Box<dyn NoiseSource> {
    fn sample(&self, x: f64, y: f64) -> f64 {
        self.as_ref().sample(x, y)
    }
}

/// Fractal Brownian motion parameters, as written in `world.ron`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoiseConfig {
    pub frequency: f64,
    pub octaves: usize,
    pub lacunarity: f64,
    pub persistence: f64,
}

impl NoiseConfig {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let valid = self.frequency.is_finite()
            && self.frequency > 0.0
            && (1..=Fbm::<Perlin>::MAX_OCTAVES).contains(&self.octaves)
            && self.lacunarity.is_finite()
            && self.lacunarity > 0.0
            && self.persistence.is_finite()
            && self.persistence > 0.0
            && self.persistence <= 1.0;
        if valid {
            Ok(())
        } else {
            Err(format!("invalid noise parameters {self:?}"))
        }
    }
}

/// Seeded Perlin fBm.
pub struct FbmNoise(Fbm<Perlin>);

impl FbmNoise {
    /// Panics on parameters rejected by [`NoiseConfig::validate`].
    pub fn new(seed: u64, config: NoiseConfig) -> Self {
        if let Err(error) = config.validate() {
            panic!("{error}");
        }
        let seed = (seed ^ (seed >> 32)) as u32;
        Self(
            Fbm::<Perlin>::new(seed)
                .set_frequency(config.frequency)
                .set_octaves(config.octaves)
                .set_lacunarity(config.lacunarity)
                .set_persistence(config.persistence),
        )
    }
}

impl NoiseSource for FbmNoise {
    fn sample(&self, x: f64, y: f64) -> f64 {
        self.0.get([x, y])
    }
}
