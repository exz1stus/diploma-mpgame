use bevy::app::{App, Plugin};
use bevy_ecs::resource::Resource;
use worldgen::WorldGeneratorConfig;

/// Engine integration belongs here; the generator itself has no Bevy dependency.
#[derive(Resource)]
pub(crate) struct WorldGenerator(pub worldgen::WorldGenerator);

impl std::ops::Deref for WorldGenerator {
    type Target = worldgen::WorldGenerator;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(crate) struct WorldGeneration;

impl Plugin for WorldGeneration {
    fn build(&self, app: &mut App) {
        let config = WorldGeneratorConfig::default();
        let mut coarse_elevation = worldgen::ElevationNoise::default_config();
        coarse_elevation.octaves = 2;
        app.insert_resource(WorldGenerator(worldgen::build_world_generator(
            config,
            worldgen::BiomeMap::parse(),
            worldgen::TemperatureModel::new(
                worldgen::TemperatureRegionsNoise::new(config.seed ^ 0x5445_4d50),
                worldgen::WorldGeography::new(config.chunk_size, config.world_size_chunks),
                worldgen::PlanetParams::default(),
            ),
            worldgen::CraterGenerator::default(),
            worldgen::BasinGenerator::new(worldgen::ElevationNoise::with_config(
                config.seed,
                coarse_elevation,
            )),
            worldgen::VolcanoGenerator::default(),
        )));
    }
}
