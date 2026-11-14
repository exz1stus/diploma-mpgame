//! Deterministic world generation, independent of engines and transports.

pub mod biomes;
mod chunk;
mod config;
mod elevation;
mod error;
mod features;
mod generator;
mod geographical;
mod hash;
mod noise;
pub mod objects;
mod pipeline;
mod preset;
mod range;
mod stage;
mod temperature;
mod tile;
pub mod tiles;

pub use biomes::{
    Biome, BiomeId, BiomeMap, BiomeMapDefinition, BiomeMapError, BiomeRepository,
    BiomeRepositoryError, BiomeRule, BiomeStage, ResolvedBiomeRule,
};
pub use chunk::{ChunkData, ChunkPos, TileRect, WorldBounds};
pub use config::WorldGeneratorConfig;
pub use elevation::{ClampHeightStage, ElevationStage};
pub use error::GenerationError;
pub use features::basin::BasinGenerator;
pub use features::crater::CraterGenerator;
pub use features::volcano::VolcanoGenerator;
pub use features::{FeatureGenerator, FeatureStage, TerrainFeature};
pub use generator::WorldGenerator;
pub use geographical::{GeoPosition, WorldGeography};
pub use noise::{ElevationNoise, NoiseConfig, NoiseSource, TemperatureRegionsNoise};
pub use objects::{
    Footprint, ObjectDefinition, ObjectId, ObjectInstanceId, ObjectPlacer, ObjectRepository,
    ObjectRule, ObjectsError, PlacedObject,
};
pub use pipeline::{assemble_world_generator, build_world_generator};
pub use preset::{PresetError, PresetFile, WorldPreset};
pub use range::Range;
pub use stage::GenerationStage;
pub use temperature::{PlanetParams, TemperatureCoefficients, TemperatureModel, TemperatureStage};
pub use tile::{TileData, TileDraft, TilePos};
pub use tiles::{
    ResolvedTileRule, Tile, TileDefinition, TileMapper, TileMappingError, TilePalette, TileRule,
};

#[cfg(test)]
mod chunk_tests;
