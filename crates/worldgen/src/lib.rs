//! Deterministic world generation, independent of engines and transports.

pub mod biomes;
mod config;
mod elevation;
mod features;
mod generator;
mod geographical;
mod noise;
mod pipeline;
mod stage;
mod temperature;
mod tile;

pub use biomes::{Biome, BiomeId, BiomeMap, BiomeMapError, BiomeStage, Range};
pub use config::WorldGeneratorConfig;
pub use elevation::{ClampHeightStage, ElevationStage};
pub use features::basin::BasinGenerator;
pub use features::crater::CraterGenerator;
pub use features::volcano::VolcanoGenerator;
pub use features::{FeatureGenerator, FeatureStage, TerrainFeature};
pub use generator::WorldGenerator;
pub use geographical::{GeoPosition, WorldGeography};
pub use noise::{ElevationNoise, NoiseConfig, NoiseSource, TemperatureRegionsNoise};
pub use pipeline::build_world_generator;
pub use stage::GenerationStage;
pub use temperature::{PlanetParams, TemperatureCoefficients, TemperatureModel, TemperatureStage};
pub use tile::{ChunkPos, Tile, TileDraft, TilePos, WorldBounds};
