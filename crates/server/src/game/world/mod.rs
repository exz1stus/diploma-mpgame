use bevy::app::{App, Plugin};
use bevy_ecs::resource::Resource;
use std::sync::Arc;
use worldgen::WorldGeneratorConfig;

pub(crate) mod jobs;
use jobs::ChunkGenerationJobs;

/// Engine integration belongs here; the generator itself has no Bevy dependency.
#[derive(Resource)]
pub(crate) struct WorldGenerator(pub Arc<worldgen::WorldGenerator>);

impl std::ops::Deref for WorldGenerator {
    type Target = worldgen::WorldGenerator;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(crate) struct WorldGeneration;

impl Plugin for WorldGeneration {
    fn build(&self, app: &mut App) {
        let mut config = WorldGeneratorConfig::default();

        if let Some(seed) = std::env::var_os("WORLD_SEED") {
            config.seed = seed
                .to_str()
                .expect("WORLD_SEED must contain valid text")
                .parse::<u64>()
                .expect("WORLD_SEED must be an unsigned 64-bit integer");
        }
        let mut coarse_elevation = worldgen::ElevationNoise::default_config();
        coarse_elevation.octaves = 2;
        let biomes = worldgen::BiomeRepository::parse();
        let biome_map = worldgen::BiomeMap::parse(&biomes);
        let generator = Arc::new(worldgen::build_world_generator(
            config,
            biomes,
            biome_map,
            worldgen::ElevationNoise::new(config.seed),
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
        ));
        let workers = std::thread::available_parallelism()
            .map_or(2, |count| count.get())
            .clamp(1, 8);
        app.insert_resource(ChunkGenerationJobs::new(
            Arc::clone(&generator),
            workers,
            64,
        ));
        app.insert_resource(WorldGenerator(generator));
    }
}
