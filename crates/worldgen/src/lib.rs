//! Deterministic world generation, independent of engines and transports.

pub mod biomes;
mod config;
mod features;
mod generator;
mod geographical;
mod noise;
mod sampler;
mod temperature;

pub use biomes::{Biome, BiomeMap, BiomeMapError, Range};
pub use config::WorldGeneratorConfig;
pub use features::basin::BasinGenerator;
pub use features::crater::CraterGenerator;
pub use features::volcano::VolcanoGenerator;
pub use generator::{ChunkPos, Tile, TilePos, WorldGenerator};
pub use geographical::{GeoPosition, WorldGeography};
pub use noise::{ElevationNoise, NoiseConfig, NoiseSource, TemperatureRegionsNoise};
pub use sampler::{ElevationSampler, TileSampler};
pub use temperature::{PlanetParams, TemperatureCoefficients, TemperatureSampler};
