//! Deterministic world generation, independent of engines and transports.

pub mod biomes;
mod config;
mod generator;
mod geographical;
mod noise;
mod sampler;
mod temperature;

pub use biomes::{Biome, BiomeMap, BiomeMapError, Range};
pub use config::WorldGeneratorConfig;
pub use generator::{TerrainChunk, Tile, TileData, WorldGenerator};
pub use geographical::{ChunkPos, GeoPosition, TilePos, WorldGeography};
pub use noise::{ElevationNoise, NoiseConfig, NoiseSource, TemperatureRegionsNoise};
pub use sampler::{ElevationSampler, TileSampler};
pub use temperature::{PlanetParams, TemperatureCoefficients, TemperatureSampler};
