use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::*;

type Samples = Arc<Mutex<HashMap<TilePos, usize>>>;

struct TestTerrain(Samples);

impl GenerationStage for TestTerrain {
    fn apply(&self, pos: TilePos, draft: &mut TileDraft) {
        *self.0.lock().unwrap().entry(pos).or_default() += 1;
        draft.height = if pos.x % 2 == 0 { 0.25 } else { 0.75 };
        draft.temperature = 0.0;
        draft.biome = Some(BiomeId(0));
    }
}

fn mapper(biomes: &BiomeRepository, reversed: bool) -> TileMapper {
    let source = if reversed {
        r#"[(tile: "high", height: (min: 0.0, max: 0.5)), (tile: "low", height: (min: 0.5, max: 1.0))]"#
    } else {
        r#"[(tile: "low", height: (min: 0.0, max: 0.5)), (tile: "high", height: (min: 0.5, max: 1.0))]"#
    };
    TileMapper::from_ron(
        source,
        TilePalette::from_ron(
            r##"[
        (name: "low", color: "#123456"), (name: "high", color: "#abcdef"),
    ]"##,
        )
        .unwrap(),
        biomes,
    )
    .unwrap()
}

fn generator(size: u32, samples: Samples, reversed: bool) -> WorldGenerator {
    let config = WorldGeneratorConfig {
        chunk_size: size,
        world_size_chunks: 16 / size,
        seed: 42,
        ..Default::default()
    };
    let biomes = Arc::new(BiomeRepository::from_ron(r#"[(name: "plain")]"#).unwrap());
    let objects = ObjectPlacer::from_ron(
        r#"[
        (id: 3, object: "rock", density: 0.3),
        (id: 5, object: "tree", density: 0.15),
    ]"#,
        ObjectRepository::from_ron(
            r#"[
        (name: "rock", footprint: (width: 2, height: 2)),
        (name: "tree", footprint: (width: 1, height: 1)),
    ]"#,
        )
        .unwrap(),
        &biomes,
    )
    .unwrap();
    WorldGenerator::new(
        config,
        vec![Box::new(TestTerrain(samples))],
        Arc::clone(&biomes),
        mapper(&biomes, reversed),
        objects,
    )
}

#[test]
fn chunk_reuses_terrain_samples_and_returns_row_major_tiles() {
    let samples = Samples::default();
    let generator = generator(4, Arc::clone(&samples), false);
    let position = ChunkPos { x: 1, y: 1 };
    let chunk = generator.chunk_at(position).unwrap();
    assert_eq!(chunk.position, position);
    assert_eq!(chunk.size, 4);
    assert_eq!(
        chunk.tiles.iter().map(|tile| tile.id).collect::<Vec<_>>(),
        vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1]
    );
    assert!(!chunk.objects.is_empty());
    let counts = samples.lock().unwrap();
    assert!(counts.len() >= 16);
    assert!(
        counts.values().all(|&count| count == 1),
        "each terrain position must run stages at most once per request"
    );
    drop(counts);
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(
                chunk.tiles[y * 4 + x],
                generator
                    .tile_at(TilePos {
                        x: 4 + x as i64,
                        y: 4 + y as i64,
                    })
                    .unwrap()
            );
        }
    }
    let encoded = serde_json::to_vec(&chunk).unwrap();
    assert_eq!(
        serde_json::from_slice::<ChunkData>(&encoded).unwrap(),
        chunk
    );
}

#[test]
fn chunk_results_are_identical_across_worker_and_request_orders() {
    let generator = Arc::new(generator(4, Samples::default(), false));
    let positions: Vec<_> = (0..4)
        .flat_map(|y| (0..4).map(move |x| ChunkPos { x, y }))
        .collect();
    let expected: Vec<_> = positions
        .iter()
        .map(|&pos| generator.chunk_at(pos).unwrap())
        .collect();
    let jobs: Vec<_> = positions
        .iter()
        .rev()
        .map(|&pos| {
            let generator = Arc::clone(&generator);
            std::thread::spawn(move || generator.chunk_at(pos).unwrap())
        })
        .collect();
    for (job, expected) in jobs.into_iter().zip(expected.iter().rev()) {
        assert_eq!(job.join().unwrap(), *expected);
    }
}

#[test]
fn palette_changes_preserve_objects_and_chunk_partitions_preserve_global_placement() {
    let small = generator(4, Samples::default(), false);
    let alternate = generator(4, Samples::default(), true);
    let large = generator(8, Samples::default(), false);
    let big_chunk = large.chunk_at(ChunkPos { x: 0, y: 0 }).unwrap();
    for y in 0..2 {
        for x in 0..2 {
            let pos = ChunkPos { x, y };
            let original = small.chunk_at(pos).unwrap();
            let changed = alternate.chunk_at(pos).unwrap();
            assert_eq!(original.objects, changed.objects);
            assert_ne!(original.tiles, changed.tiles);
            let bounds = TileRect {
                min: TilePos { x: x * 4, y: y * 4 },
                max: TilePos {
                    x: x * 4 + 4,
                    y: y * 4 + 4,
                },
            };
            let expected: Vec<_> = big_chunk
                .objects
                .iter()
                .filter(|object| {
                    object
                        .footprint
                        .bounds(object.id.anchor)
                        .unwrap()
                        .intersects(bounds)
                })
                .cloned()
                .collect();
            assert_eq!(original.objects, expected);
        }
    }
}

#[test]
fn invalid_chunk_requests_and_classification_gaps_return_errors() {
    let generator = generator(4, Samples::default(), false);
    for position in [
        ChunkPos { x: -1, y: 0 },
        ChunkPos { x: 0, y: 4 },
        ChunkPos { x: i64::MAX, y: 0 },
    ] {
        assert_eq!(
            generator.chunk_at(position),
            Err(GenerationError::ChunkOutsideWorld(position))
        );
    }
    for position in [
        TilePos { x: -1, y: 0 },
        TilePos { x: 16, y: 0 },
        TilePos { x: 0, y: i64::MAX },
    ] {
        assert_eq!(
            generator.tile_at(position),
            Err(GenerationError::TileOutsideWorld(position))
        );
    }
    let biomes = Arc::new(BiomeRepository::from_ron(r#"[(name: "plain")]"#).unwrap());
    let no_biome = WorldGenerator::new(
        generator.config(),
        vec![],
        Arc::clone(&biomes),
        mapper(&biomes, false),
        ObjectPlacer::empty(),
    );
    let origin = TilePos { x: 0, y: 0 };
    assert_eq!(
        no_biome.chunk_at(ChunkPos { x: 0, y: 0 }),
        Err(GenerationError::NoBiome(origin))
    );
    let no_tile = WorldGenerator::new(
        generator.config(),
        vec![Box::new(TestTerrain(Samples::default()))],
        Arc::clone(&biomes),
        TileMapper::from_ron("[]", TilePalette::new(vec![]).unwrap(), &biomes).unwrap(),
        ObjectPlacer::empty(),
    );
    assert_eq!(
        no_tile.chunk_at(ChunkPos { x: 0, y: 0 }),
        Err(GenerationError::NoTile(origin))
    );
    assert_eq!(
        no_tile.tile_at(origin),
        Err(GenerationError::NoTile(origin))
    );
    for config in [
        WorldGeneratorConfig {
            chunk_size: 0,
            ..Default::default()
        },
        WorldGeneratorConfig {
            world_size_chunks: 0,
            ..Default::default()
        },
        WorldGeneratorConfig {
            chunk_size: u32::MAX,
            world_size_chunks: 2,
            ..Default::default()
        },
    ] {
        assert_eq!(config.tile_bounds(), Err(GenerationError::InvalidRegion));
    }
}
