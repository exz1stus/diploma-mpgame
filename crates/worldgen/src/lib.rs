//! Deterministic world generation, independent of engines and transports.
//!
//! The world is described declaratively by [`WorldConfig`] (defaults in `worldgen/config/`,
//! overridable per file). [`WorldConfig::build`] yields a [`WorldGenerator`] that serves
//! [`ChunkData`]: per-tile biome and height, plus anchored objects. Appearance is not
//! part of this crate; consumers map names from [`WorldManifest`] to their own assets.

pub mod biomes;
mod chunk;
mod config;
mod elevation;
mod error;
mod features;
mod generator;
mod geographical;
mod hash;
mod manifest;
mod noise;
pub mod objects;
#[cfg(test)]
mod old_preset;
mod pipeline;
mod range;
mod stage;
mod temperature;
mod tile;

pub use biomes::{
    Biome, BiomeId, BiomeMap, BiomeMapDefinition, BiomeMapError, BiomeRepository,
    BiomeRepositoryError, BiomeRule, BiomeStage, ResolvedBiomeRule,
};
pub use chunk::{ChunkData, ChunkPos, TileRect, WorldBounds};
pub use config::{
    BiomeDefaults, BiomeDefinition, BiomesFile, ClimateSettings, ConfigError, ConfigFile,
    ConfigSource, ObjectSpawn, ObjectsFile, WorldConfig, WorldGeneratorConfig, WorldSettings,
};
pub use elevation::{ClampHeightStage, ElevationStage};
pub use error::GenerationError;
pub use features::basin::{BasinGenerator, BasinSettings, BasinVariant};
pub use features::crater::CraterGenerator;
pub use features::volcano::VolcanoGenerator;
pub use features::{
    FeatureGenerator, FeatureKind, FeatureMark, FeatureStage, ScaledFeatures, TerrainFeature,
};
pub use generator::WorldGenerator;
pub use geographical::{GeoPosition, WorldGeography};
pub use manifest::{ObjectKind, WorldManifest};
pub use noise::{FbmNoise, NoiseConfig, NoiseSource};
pub use objects::{
    FeatureSpawns, Footprint, ObjectDefinition, ObjectId, ObjectInstanceId, ObjectPlacer,
    ObjectRepository, ObjectRule, ObjectsError, PlacedObject,
};
pub use pipeline::TerrainSources;
pub use range::Range;
pub use stage::GenerationStage;
pub use temperature::{PlanetParams, TemperatureCoefficients, TemperatureModel, TemperatureStage};
pub use tile::{TileData, TileDraft, TilePos, TileSample};

#[cfg(test)]
mod chunk_tests;
