//! Builds a [`WorldGenerator`] from a declarative [`WorldConfig`].

use std::sync::Arc;

use crate::objects::feature_rules;
use crate::{
    BasinGenerator, BiomeMap, BiomeRepository, BiomeStage, ClampHeightStage, ConfigError,
    ConfigFile, ElevationStage, FbmNoise, FeatureGenerator, FeatureKind, FeatureSpawns,
    FeatureStage, GenerationStage, NoiseSource, ObjectPlacer, ObjectRepository, ObjectsError,
    TemperatureModel, TemperatureStage, WorldConfig, WorldGenerator, WorldGeography,
};

const BASIN_SEED: u64 = 0x4352_4152;
const CRATER_SEED: u64 = 0x4352_4154;
const VOLCANO_SEED: u64 = 0x564F_4C43;
const REGION_SEED: u64 = 0x5445_4d50;

/// The noise fields a generator samples. [`TerrainSources::from_config`] gives the
/// configured ones; tools and tests may substitute their own.
pub struct TerrainSources {
    pub elevation: Box<dyn NoiseSource>,
    /// Coarse elevation that basin floors are smoothed towards.
    pub basin_floor: Box<dyn NoiseSource>,
    pub temperature_regions: Box<dyn NoiseSource>,
}

impl TerrainSources {
    /// Panics on invalid noise parameters; [`WorldConfig::build`] validates them first.
    pub fn from_config(config: &WorldConfig) -> Self {
        let world = &config.world;
        let mut basin_floor = world.elevation;
        basin_floor.octaves = config.basins.smoothing_octaves.min(world.elevation.octaves);
        Self {
            elevation: Box::new(FbmNoise::new(world.seed, world.elevation)),
            basin_floor: Box::new(FbmNoise::new(world.seed, basin_floor)),
            temperature_regions: Box::new(FbmNoise::new(
                world.seed ^ REGION_SEED,
                world.climate.region_noise,
            )),
        }
    }
}

impl WorldConfig {
    /// Validates the whole config and assembles the generation pipeline.
    pub fn build(&self) -> Result<WorldGenerator, ConfigError> {
        self.validate_noise()?;
        self.build_with(TerrainSources::from_config(self))
    }

    /// Like [`WorldConfig::build`], sampling the given noise fields.
    pub fn build_with(&self, sources: TerrainSources) -> Result<WorldGenerator, ConfigError> {
        self.validate_features()?;
        let dimensions = self.world.dimensions();
        dimensions
            .tile_bounds()
            .map_err(|error| ConfigError::invalid(ConfigFile::World, error))?;
        let biomes = Arc::new(
            BiomeRepository::from_definitions(&self.biomes.biomes)
                .map_err(|error| ConfigError::invalid(ConfigFile::Biomes, error))?,
        );
        let biome_map = self
            .biome_map
            .resolve(&biomes)
            .map_err(|error| ConfigError::invalid(ConfigFile::BiomeMap, error))?;
        self.check_feature_biome_maps(&biomes)?;
        self.check_feature_spawns(&biomes)?;
        let object_placer =
            ObjectPlacer::from_config(&self.biomes, &self.objects, &self.feature_spawns(), &biomes)
                .map_err(|error| ConfigError::invalid(objects_error_file(&error), error))?;
        let stages = self.stages(sources, &biomes, biome_map);
        Ok(WorldGenerator::new(
            dimensions,
            stages,
            biomes,
            object_placer,
        ))
    }

    /// Elevation, enabled features in order (basins, craters, volcanoes), clamp,
    /// temperature, then biome classification.
    fn stages(
        &self,
        sources: TerrainSources,
        biomes: &BiomeRepository,
        biome_map: BiomeMap,
    ) -> Vec<Box<dyn GenerationStage>> {
        let seed = self.world.seed;
        let chunk_size = self.world.chunk_size;
        let mut stages: Vec<Box<dyn GenerationStage>> =
            vec![Box::new(ElevationStage::new(sources.elevation))];
        if self.basins.probability > 0.0 {
            stages.push(feature_stage(
                FeatureKind::Basin,
                seed ^ BASIN_SEED,
                BasinGenerator::new(self.basins.clone(), sources.basin_floor),
                chunk_size,
                biomes,
            ));
        }
        if self.craters.density > 0.0 {
            stages.push(feature_stage(
                FeatureKind::Crater,
                seed ^ CRATER_SEED,
                self.craters.clone(),
                chunk_size,
                biomes,
            ));
        }
        if self.volcanoes.probability > 0.0 {
            stages.push(feature_stage(
                FeatureKind::Volcano,
                seed ^ VOLCANO_SEED,
                self.volcanoes.clone(),
                chunk_size,
                biomes,
            ));
        }
        stages.push(Box::new(ClampHeightStage));
        stages.push(Box::new(TemperatureStage::new(
            self.temperature_model(sources.temperature_regions),
        )));
        stages.push(Box::new(BiomeStage::new(Arc::new(biome_map))));
        stages
    }

    /// Objects declared in each feature file, whether or not the feature is enabled.
    fn feature_spawns(&self) -> [FeatureSpawns<'_>; 3] {
        [
            (FeatureKind::Basin, &self.basins.objects),
            (FeatureKind::Crater, &self.craters.objects),
            (FeatureKind::Volcano, &self.volcanoes.objects),
        ]
    }

    fn temperature_model(&self, regions: Box<dyn NoiseSource>) -> TemperatureModel {
        let climate = &self.world.climate;
        TemperatureModel::with_config(
            climate.coefficients,
            regions,
            WorldGeography::new(self.world.chunk_size, self.world.world_size_chunks),
            climate.planet,
        )
    }

    fn validate_noise(&self) -> Result<(), ConfigError> {
        self.world
            .elevation
            .validate()
            .and_then(|()| self.world.climate.region_noise.validate())
            .map_err(|error| ConfigError::invalid(ConfigFile::World, error))
    }

    fn validate_features(&self) -> Result<(), ConfigError> {
        let invalid = |file| move |error| ConfigError::invalid(file, error);
        self.craters
            .validate()
            .map_err(invalid(ConfigFile::Craters))?;
        self.basins
            .validate()
            .map_err(invalid(ConfigFile::Basins))?;
        self.volcanoes
            .validate()
            .map_err(invalid(ConfigFile::Volcanoes))
    }

    /// Validates each feature's spawns on their own, so errors name the feature file.
    fn check_feature_spawns(&self, biomes: &BiomeRepository) -> Result<(), ConfigError> {
        let files = [
            ConfigFile::Basins,
            ConfigFile::Craters,
            ConfigFile::Volcanoes,
        ];
        for (file, (kind, spawns)) in files.into_iter().zip(self.feature_spawns()) {
            let objects = ObjectRepository::new(self.objects.kinds.clone())
                .map_err(|error| ConfigError::invalid(ConfigFile::Objects, error))?;
            ObjectPlacer::new(feature_rules(kind, spawns).collect(), objects, biomes)
                .map_err(|error| ConfigError::invalid(file, error))?;
        }
        Ok(())
    }

    fn check_feature_biome_maps(&self, biomes: &BiomeRepository) -> Result<(), ConfigError> {
        for (file, map) in [
            (ConfigFile::Craters, &self.craters.biome_map),
            (ConfigFile::Basins, &self.basins.biome_map),
            (ConfigFile::Volcanoes, &self.volcanoes.biome_map),
        ] {
            map.resolve(biomes)
                .map_err(|error| ConfigError::invalid(file, error))?;
        }
        Ok(())
    }
}

fn feature_stage<G: FeatureGenerator + 'static>(
    kind: FeatureKind,
    seed: u64,
    generator: G,
    chunk_size: u32,
    biomes: &BiomeRepository,
) -> Box<dyn GenerationStage> {
    Box::new(FeatureStage::new(kind, seed, generator, chunk_size, biomes))
}

/// Spawn references are written in `biomes.ron`; kinds and sets in `objects.ron`.
fn objects_error_file(error: &ObjectsError) -> ConfigFile {
    match error {
        ObjectsError::UnknownSet(_) | ObjectsError::UnknownBiome(_) => ConfigFile::Biomes,
        _ => ConfigFile::Objects,
    }
}
