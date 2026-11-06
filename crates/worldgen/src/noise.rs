use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

pub trait NoiseSource: Send + Sync + 'static {
    fn sample(&self, x: f64, y: f64) -> f64;
}

#[derive(Clone, Copy, Debug)]
pub struct NoiseConfig {
    pub frequency: f64,
    pub octaves: usize,
    pub lacunarity: f64,
    pub persistence: f64,
}

impl NoiseConfig {
    fn build(self, seed: u64) -> Fbm<Perlin> {
        assert!(self.frequency.is_finite() && self.frequency > 0.0);
        assert!((1..=Fbm::<Perlin>::MAX_OCTAVES).contains(&self.octaves));
        assert!(self.lacunarity.is_finite() && self.lacunarity > 0.0);
        assert!(self.persistence.is_finite() && self.persistence > 0.0 && self.persistence <= 1.0);
        let seed = (seed ^ (seed >> 32)) as u32;
        Fbm::<Perlin>::new(seed)
            .set_frequency(self.frequency)
            .set_octaves(self.octaves)
            .set_lacunarity(self.lacunarity)
            .set_persistence(self.persistence)
    }
}

pub struct ElevationNoise(Fbm<Perlin>);

impl ElevationNoise {
    pub fn new(seed: u64) -> Self {
        Self::with_config(seed, Self::default_config())
    }

    pub fn default_config() -> NoiseConfig {
        NoiseConfig {
            frequency: 0.002,
            octaves: 10,
            lacunarity: 3.0,
            persistence: 0.5,
        }
    }

    pub fn with_config(seed: u64, config: NoiseConfig) -> Self {
        Self(config.build(seed))
    }
}

impl NoiseSource for ElevationNoise {
    fn sample(&self, x: f64, y: f64) -> f64 {
        self.0.get([x, y])
    }
}

pub struct TemperatureRegionsNoise(Fbm<Perlin>);

impl TemperatureRegionsNoise {
    pub fn new(seed: u64) -> Self {
        Self::with_config(seed, Self::default_config())
    }

    pub fn default_config() -> NoiseConfig {
        NoiseConfig {
            frequency: 0.001,
            octaves: 5,
            lacunarity: 3.0,
            persistence: 0.5,
        }
    }

    pub fn with_config(seed: u64, config: NoiseConfig) -> Self {
        Self(config.build(seed))
    }
}

impl NoiseSource for TemperatureRegionsNoise {
    fn sample(&self, x: f64, y: f64) -> f64 {
        self.0.get([x, y])
    }
}
