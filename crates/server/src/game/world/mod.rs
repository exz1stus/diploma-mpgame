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
        app.insert_resource(WorldGenerator(worldgen::WorldGenerator::perlin(
            WorldGeneratorConfig::default(),
        )));
    }
}
