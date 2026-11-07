//! Assembly of the default world-generation pipeline.

use std::sync::Arc;

use crate::{
    BasinGenerator, BiomeMap, BiomeStage, ClampHeightStage, CraterGenerator, ElevationNoise,
    ElevationStage, FeatureStage, GenerationStage, NoiseSource, TemperatureModel, TemperatureStage,
    VolcanoGenerator, WorldGenerator, WorldGeneratorConfig,
};

/// Build the default stage order with a custom elevation noise source.
pub fn build_world_generator(
    config: WorldGeneratorConfig,
    biomes: BiomeMap,
    temperature: TemperatureModel,
    craters: CraterGenerator,
    basins: BasinGenerator,
    volcanoes: VolcanoGenerator,
) -> WorldGenerator {
    assert!(craters.cell_size > 0, "crater cell size must be positive");
    assert!(craters.min_radius.is_finite() && craters.min_radius > 0.0);
    assert!(craters.max_radius.is_finite() && craters.max_radius >= craters.min_radius);
    assert!(craters.density.is_finite() && (0.0..=4.0).contains(&craters.density));
    volcanoes.validate();

    let biomes = Arc::new(biomes);
    let mut stages: Vec<Box<dyn GenerationStage>> = vec![Box::new(ElevationStage::new(
        ElevationNoise::new(config.seed),
    ))];
    if basins.probability > 0.0 {
        stages.push(Box::new(FeatureStage::new(
            config.seed ^ 0x4352_4152,
            basins,
            config.chunk_size,
        )));
    }
    if craters.density > 0.0 {
        stages.push(Box::new(FeatureStage::new(
            config.seed ^ 0x4352_4154,
            craters,
            config.chunk_size,
        )));
    }
    if volcanoes.probability > 0.0 {
        stages.push(Box::new(FeatureStage::new(
            config.seed ^ 0x564F_4C43,
            volcanoes,
            config.chunk_size,
        )));
    }
    stages.push(Box::new(ClampHeightStage));
    stages.push(Box::new(TemperatureStage::new(temperature)));
    stages.push(Box::new(BiomeStage::new(Arc::clone(&biomes))));
    WorldGenerator::new(stages, biomes)
}
