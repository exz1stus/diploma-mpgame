//! The declarative world description: one struct per config file, plus [`WorldConfig`]
//! which holds all of them. Field docs describe the RON format.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::{ConfigError, ConfigFile, ConfigSource, WorldGeneratorConfig};
use crate::{
    BasinSettings, BiomeMapDefinition, CraterGenerator, NoiseConfig, ObjectDefinition,
    PlanetParams, Range, TemperatureCoefficients, VolcanoGenerator,
};

/// `world.ron`: dimensions, seed and the terrain/climate models.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldSettings {
    pub generator_version: u32,
    pub seed: u64,
    pub world_size_chunks: u32,
    pub chunk_size: u32,
    pub elevation: NoiseConfig,
    pub climate: ClimateSettings,
}

impl WorldSettings {
    pub fn dimensions(&self) -> WorldGeneratorConfig {
        WorldGeneratorConfig {
            generator_version: self.generator_version,
            seed: self.seed,
            world_size_chunks: self.world_size_chunks,
            chunk_size: self.chunk_size,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClimateSettings {
    pub coefficients: TemperatureCoefficients,
    pub region_noise: NoiseConfig,
    pub planet: PlanetParams,
}

/// `biomes.ron`: every biome and the objects it spawns. List order is the biome ID.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomesFile {
    #[serde(default)]
    pub defaults: BiomeDefaults,
    pub biomes: Vec<BiomeDefinition>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeDefaults {
    /// Sets included by biomes that do not list `object_sets` themselves.
    #[serde(default)]
    pub object_sets: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeDefinition {
    pub name: String,
    /// Object sets from `objects.ron`; `None` uses [`BiomeDefaults::object_sets`].
    #[serde(default)]
    pub object_sets: Option<Vec<String>>,
    /// Spawns specific to this biome.
    #[serde(default)]
    pub objects: Vec<ObjectSpawn>,
}

impl BiomeDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            object_sets: None,
            objects: Vec::new(),
        }
    }
}

impl BiomesFile {
    /// Sets a biome includes, after applying the defaults.
    pub fn object_sets_of<'a>(&'a self, biome: &'a BiomeDefinition) -> &'a [String] {
        biome
            .object_sets
            .as_deref()
            .unwrap_or(&self.defaults.object_sets)
    }
}

/// One object a biome (or set) may spawn, with its density and terrain limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectSpawn {
    pub object: String,
    /// Proposal probability per anchor tile, in [0, 1], before overlap rejection.
    pub density: f64,
    #[serde(default = "Range::full_height")]
    pub height: Range,
    #[serde(default = "Range::unbounded")]
    pub temperature: Range,
    /// Largest height spread allowed under the footprint.
    #[serde(default)]
    pub max_height_difference: Option<f64>,
    /// Only for spawns in `features/*.ron`: distance from the feature centre,
    /// where 1 is the feature radius (crater rim, volcano foot).
    #[serde(default)]
    pub distance: Option<Range>,
}

/// `objects.ron`: object kinds (list order is the object ID) and shared spawn sets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectsFile {
    pub kinds: Vec<ObjectDefinition>,
    #[serde(default)]
    pub sets: BTreeMap<String, Vec<ObjectSpawn>>,
}

/// The complete, declarative description of a world.
#[derive(Clone, Debug)]
pub struct WorldConfig {
    pub world: WorldSettings,
    pub biomes: BiomesFile,
    pub biome_map: BiomeMapDefinition,
    pub objects: ObjectsFile,
    pub craters: CraterGenerator,
    pub basins: BasinSettings,
    pub volcanoes: VolcanoGenerator,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self::bundled()
    }
}

impl WorldConfig {
    /// The defaults shipped in `worldgen/config/`.
    pub fn bundled() -> Self {
        Self::load(&ConfigSource::bundled()).expect("bundled world config must be valid")
    }

    /// Parses every file. Cross-file references are checked when the generator is built.
    pub fn load(source: &ConfigSource) -> Result<Self, ConfigError> {
        Ok(Self {
            world: parse(source, ConfigFile::World)?,
            biomes: parse(source, ConfigFile::Biomes)?,
            biome_map: parse(source, ConfigFile::BiomeMap)?,
            objects: parse(source, ConfigFile::Objects)?,
            craters: parse(source, ConfigFile::Craters)?,
            basins: parse(source, ConfigFile::Basins)?,
            volcanoes: parse(source, ConfigFile::Volcanoes)?,
        })
    }
}

fn parse<T: DeserializeOwned>(source: &ConfigSource, file: ConfigFile) -> Result<T, ConfigError> {
    parse_ron(source.text(file)).map_err(|error| ConfigError::invalid(file, error))
}

/// RON as used by every config file: `Option` fields may omit `Some(...)`.
pub(crate) fn parse_ron<T: DeserializeOwned>(text: &str) -> Result<T, ron::error::SpannedError> {
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(text)
}
