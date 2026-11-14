//! Assembly of the default world-generation pipeline.

use std::sync::Arc;

use crate::{
    BasinGenerator, BiomeMap, BiomeRepository, BiomeStage, ClampHeightStage, CraterGenerator,
    ElevationStage, FeatureStage, GenerationStage, NoiseSource, ObjectPlacer, TemperatureModel,
    TemperatureStage, TileMapper, VolcanoGenerator, WorldGenerator, WorldGeneratorConfig,
};

#[allow(clippy::too_many_arguments)]
/// Assemble terrain stages, tile mapping, and deterministic object placement.
pub fn build_world_generator(
    config: WorldGeneratorConfig,
    biomes: BiomeRepository,
    biome_map: BiomeMap,
    height_noise: impl NoiseSource,
    temperature: TemperatureModel,
    craters: CraterGenerator,
    basins: BasinGenerator,
    volcanoes: VolcanoGenerator,
) -> WorldGenerator {
    let tile_mapper = TileMapper::parse(&biomes);
    let object_placer = ObjectPlacer::parse(&biomes);
    assemble_world_generator(
        config,
        biomes,
        biome_map,
        height_noise,
        temperature,
        craters,
        basins,
        volcanoes,
        tile_mapper,
        object_placer,
    )
}

/// Like [`build_world_generator`], with explicit tile mapping and object placement
/// (as parsed from a [`crate::WorldPreset`]).
#[allow(clippy::too_many_arguments)]
pub fn assemble_world_generator(
    config: WorldGeneratorConfig,
    biomes: BiomeRepository,
    biome_map: BiomeMap,
    height_noise: impl NoiseSource,
    temperature: TemperatureModel,
    craters: CraterGenerator,
    basins: BasinGenerator,
    volcanoes: VolcanoGenerator,
    tile_mapper: TileMapper,
    object_placer: ObjectPlacer,
) -> WorldGenerator {
    assert!(craters.cell_size > 0, "crater cell size must be positive");
    assert!(craters.min_radius.is_finite() && craters.min_radius > 0.0);
    assert!(craters.max_radius.is_finite() && craters.max_radius >= craters.min_radius);
    assert!(craters.density.is_finite() && (0.0..=4.0).contains(&craters.density));
    assert!(craters.temperature_offset.is_finite());
    volcanoes.validate();

    let biomes = Arc::new(biomes);
    let mut stages: Vec<Box<dyn GenerationStage>> =
        vec![Box::new(ElevationStage::new(height_noise))];
    if basins.probability > 0.0 {
        stages.push(Box::new(FeatureStage::new(
            config.seed ^ 0x4352_4152,
            basins,
            config.chunk_size,
            &biomes,
        )));
    }
    if craters.density > 0.0 {
        stages.push(Box::new(FeatureStage::new(
            config.seed ^ 0x4352_4154,
            craters,
            config.chunk_size,
            &biomes,
        )));
    }
    if volcanoes.probability > 0.0 {
        stages.push(Box::new(FeatureStage::new(
            config.seed ^ 0x564F_4C43,
            volcanoes,
            config.chunk_size,
            &biomes,
        )));
    }
    stages.push(Box::new(ClampHeightStage));
    stages.push(Box::new(TemperatureStage::new(temperature)));
    stages.push(Box::new(BiomeStage::new(Arc::new(biome_map))));
    WorldGenerator::new(config, stages, biomes, tile_mapper, object_placer)
}
