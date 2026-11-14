//! Small deterministic world for streaming tests: 16x16 chunks of 4x4 tiles, game chunks -8..=7.

use std::sync::Arc;

use bevy::{app::App, math::Vec2};
use worldgen::{
    BiomeId, BiomeRepository, GenerationStage, ObjectPlacer, ObjectRepository, TileDraft,
    TileMapper, TilePalette, TilePos, WorldGeneratorConfig,
};

use super::coords::TILE_SIZE;

pub(crate) const CHUNK: u32 = 4;

struct Flat;

impl GenerationStage for Flat {
    fn apply(&self, _: TilePos, draft: &mut TileDraft) {
        draft.height = 0.5;
        draft.biome = Some(BiomeId(0));
    }
}

pub(crate) fn test_generator() -> worldgen::WorldGenerator {
    let biomes = Arc::new(BiomeRepository::from_ron(r#"[(name: "plain")]"#).unwrap());
    let mapper = TileMapper::from_ron(
        r#"[(tile: "ground", height: (min: 0.0, max: 1.0))]"#,
        TilePalette::from_ron(r##"[(name: "ground", color: "#123456")]"##).unwrap(),
        &biomes,
    )
    .unwrap();
    let objects = ObjectPlacer::from_ron(
        r#"[(id: 1, object: "rock", density: 0.3)]"#,
        ObjectRepository::from_ron(r#"[(name: "rock", footprint: (width: 1, height: 1))]"#)
            .unwrap(),
        &biomes,
    )
    .unwrap();
    worldgen::WorldGenerator::new(
        WorldGeneratorConfig {
            chunk_size: CHUNK,
            world_size_chunks: 16,
            seed: 7,
            ..Default::default()
        },
        vec![Box::new(Flat)],
        biomes,
        mapper,
        objects,
    )
}

/// App with the test world installed and a task pool for generation.
pub(crate) fn world_app() -> App {
    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default());
    super::install(&mut app, Arc::new(test_generator()), 4);
    app
}

pub(crate) fn chunk_center(x: i64, y: i64) -> Vec2 {
    let span = TILE_SIZE * CHUNK as f32;
    Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * span
}
