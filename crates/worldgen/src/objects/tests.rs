use std::collections::HashSet;

use super::*;
use crate::{
    BiomeId, BiomeRepository, GenerationError, TileData, TilePos, TileRect, WorldGeneratorConfig,
};

fn biomes() -> BiomeRepository {
    BiomeRepository::from_ron(r#"[(name: "plain"), (name: "other")]"#).unwrap()
}

fn repository() -> ObjectRepository {
    ObjectRepository::from_ron(
        r#"[
        (name: "small", footprint: (width: 1, height: 1)),
        (name: "wide", footprint: (width: 3, height: 2)),
        (name: "square", footprint: (width: 2, height: 2)),
        (name: "huge", footprint: (width: 7, height: 5)),
    ]"#,
    )
    .unwrap()
}

fn placer(source: &str) -> ObjectPlacer {
    ObjectPlacer::from_ron(source, repository(), &biomes()).unwrap()
}

fn config(size: u32, chunks: u32) -> WorldGeneratorConfig {
    WorldGeneratorConfig {
        chunk_size: size,
        world_size_chunks: chunks,
        seed: 42,
        ..Default::default()
    }
}

fn rect(x: i64, y: i64, width: i64, height: i64) -> TileRect {
    TileRect {
        min: TilePos { x, y },
        max: TilePos {
            x: x + width,
            y: y + height,
        },
    }
}

fn flat(_: TilePos) -> Result<TileData, GenerationError> {
    Ok(TileData {
        height: 0.5,
        temperature: 0.0,
        biome: BiomeId(0),
    })
}

#[test]
fn local_queries_match_whole_world_placement_including_large_footprints() {
    let placer = placer(
        r#"[
        (id: 11, object: "wide", density: 0.3),
        (id: 12, object: "small", density: 0.1),
        (id: 13, object: "huge", density: 0.04),
    ]"#,
    );
    let config = config(4, 6);
    let world = config.tile_bounds().unwrap();
    let all = placer.place_in(world, config, flat).unwrap();
    assert!(!all.is_empty());
    for cy in 0..6 {
        for cx in 0..6 {
            let chunk = rect(cx * 4, cy * 4, 4, 4);
            let expected: Vec<_> = all
                .iter()
                .filter(|object| {
                    object
                        .footprint
                        .bounds(object.id.anchor)
                        .unwrap()
                        .intersects(chunk)
                })
                .cloned()
                .collect();
            assert_eq!(
                placer.place_in(chunk, config, flat).unwrap(),
                expected,
                "{chunk:?}"
            );
        }
    }
    for (index, object) in all.iter().enumerate() {
        let bounds = object.footprint.bounds(object.id.anchor).unwrap();
        assert!(world.contains_rect(bounds));
        for other in &all[index + 1..] {
            assert!(!bounds.intersects(other.footprint.bounds(other.id.anchor).unwrap()));
        }
    }
}

#[test]
fn an_object_larger_than_a_chunk_can_be_loaded_from_any_intersecting_chunk() {
    let placer = placer(r#"[(id: 77, object: "huge", density: 1.0, biomes: ["plain"])]"#);
    let config = config(4, 4);
    let suitable = rect(3, 3, 7, 5);
    let sample = |pos| {
        Ok(TileData {
            biome: if suitable.contains(pos) {
                BiomeId(0)
            } else {
                BiomeId(1)
            },
            ..flat(pos).unwrap()
        })
    };
    let all = placer
        .place_in(config.tile_bounds().unwrap(), config, sample)
        .unwrap();
    assert_eq!(all.len(), 1);
    for cy in 0..4 {
        for cx in 0..4 {
            let bounds = rect(cx * 4, cy * 4, 4, 4);
            let objects = placer.place_in(bounds, config, sample).unwrap();
            assert_eq!(
                objects,
                if bounds.intersects(suitable) {
                    all.clone()
                } else {
                    vec![]
                }
            );
        }
    }
}

#[test]
fn a_corner_object_is_identical_in_all_four_chunk_responses() {
    let placer = placer(r#"[(id: 55, object: "square", density: 1.0, biomes: ["plain"])]"#);
    let config = config(4, 2);
    let suitable = rect(3, 3, 2, 2);
    let sample = |pos| {
        Ok(TileData {
            biome: if suitable.contains(pos) {
                BiomeId(0)
            } else {
                BiomeId(1)
            },
            ..flat(pos).unwrap()
        })
    };
    let mut results = Vec::new();
    for y in [0, 4] {
        for x in [0, 4] {
            let objects = placer.place_in(rect(x, y, 4, 4), config, sample).unwrap();
            assert_eq!(objects.len(), 1);
            assert_eq!(objects[0].id.anchor, suitable.min);
            assert_eq!(objects[0].owner_chunk(4), crate::ChunkPos { x: 0, y: 0 });
            results.push(objects);
        }
    }
    assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
    let unique: HashSet<_> = results
        .into_iter()
        .flatten()
        .map(|object| object.id)
        .collect();
    assert_eq!(unique.len(), 1);
}

#[test]
fn every_occupied_tile_must_qualify_and_height_difference_is_checked() {
    let source = r#"[(id: 1, object: "square", density: 1.0, biomes: ["plain"],
        height: (min: 0.4, max: 1.0), temperature: (min: -10.0, max: 10.0),
        max_height_difference: Some(0.25))]"#;
    let placer = placer(source);
    let config = config(4, 2);
    let world = config.tile_bounds().unwrap();
    let area = rect(2, 2, 2, 2);
    let base = |pos| TileData {
        biome: if area.contains(pos) {
            BiomeId(0)
        } else {
            BiomeId(1)
        },
        ..flat(pos).unwrap()
    };
    let good = placer.place_in(world, config, |pos| Ok(base(pos))).unwrap();
    assert_eq!(good.len(), 1);
    assert_eq!(good[0].id.anchor, area.min);

    // Changing only the tile farthest from the anchor invalidates the whole object.
    for broken in [
        TileData {
            biome: BiomeId(1),
            ..flat(area.min).unwrap()
        },
        TileData {
            height: 0.2,
            ..flat(area.min).unwrap()
        },
        TileData {
            temperature: 10.0,
            ..flat(area.min).unwrap()
        },
        TileData {
            height: 0.875,
            ..flat(area.min).unwrap()
        },
        TileData {
            temperature: f64::NAN,
            ..flat(area.min).unwrap()
        },
    ] {
        let objects = placer
            .place_in(world, config, |pos| {
                Ok(if pos == (TilePos { x: 3, y: 3 }) {
                    broken.clone()
                } else {
                    base(pos)
                })
            })
            .unwrap();
        assert!(objects.is_empty(), "{broken:?}");
    }
    let endpoint = placer
        .place_in(world, config, |pos| {
            Ok(TileData {
                height: 1.0,
                ..base(pos)
            })
        })
        .unwrap();
    assert_eq!(
        endpoint.len(),
        1,
        "height 1 must be allowed by a range ending at 1"
    );
}

#[test]
fn density_uses_independent_global_rolls_and_zero_density_does_no_sampling() {
    let config = config(16, 2);
    let bounds = config.tile_bounds().unwrap();
    let zero = placer(r#"[(id: 1, object: "small", density: 0.0)]"#);
    assert!(
        zero.place_in(bounds, config, |_| panic!("disabled rules must not sample"))
            .unwrap()
            .is_empty()
    );
    let full = placer(r#"[(id: 1, object: "small", density: 1.0)]"#);
    assert_eq!(full.place_in(bounds, config, flat).unwrap().len(), 32 * 32);
    let sparse = placer(r#"[(id: 1, object: "small", density: 0.2)]"#);
    let baseline = sparse.place_in(bounds, config, flat).unwrap();
    assert!(!baseline.is_empty());
    assert!(baseline.len() < 32 * 32);
    assert_eq!(baseline, sparse.place_in(bounds, config, flat).unwrap());
    assert_ne!(
        baseline,
        sparse
            .place_in(bounds, WorldGeneratorConfig { seed: 43, ..config }, flat)
            .unwrap()
    );
    assert_ne!(
        baseline,
        sparse
            .place_in(
                bounds,
                WorldGeneratorConfig {
                    generator_version: 2,
                    ..config
                },
                flat
            )
            .unwrap()
    );
    let unrelated = placer(
        r#"[
        (id: 2, object: "square", density: 1.0, biomes: ["other"]),
        (id: 1, object: "small", density: 0.2),
    ]"#,
    );
    assert_eq!(baseline, unrelated.place_in(bounds, config, flat).unwrap());
}

#[test]
fn rule_order_and_chunk_partition_do_not_change_global_placement() {
    let source = r#"[
        (id: 10, object: "wide", density: 0.4),
        (id: 20, object: "small", density: 0.3),
    ]"#;
    let mut rules: Vec<ObjectRule> = ron::from_str(source).unwrap();
    let original = ObjectPlacer::new(rules.clone(), repository(), &biomes()).unwrap();
    rules.reverse();
    let reordered = ObjectPlacer::new(rules, repository(), &biomes()).unwrap();
    let config = config(8, 2);
    let world = config.tile_bounds().unwrap();
    let expected = original.place_in(world, config, flat).unwrap();
    assert_eq!(expected, reordered.place_in(world, config, flat).unwrap());
    assert_eq!(
        expected,
        original
            .place_in(
                world,
                WorldGeneratorConfig {
                    chunk_size: 4,
                    world_size_chunks: 4,
                    ..config
                },
                flat
            )
            .unwrap()
    );
}

#[test]
fn world_edges_reject_partial_objects_and_invalid_queries_fail() {
    let config = config(4, 1);
    let world = config.tile_bounds().unwrap();
    let huge = placer(r#"[(id: 1, object: "huge", density: 1.0)]"#);
    assert!(
        huge.place_in(world, config, |_| panic!(
            "oversized object must be rejected before sampling"
        ))
        .unwrap()
        .is_empty()
    );
    let square = placer(r#"[(id: 1, object: "square", density: 1.0)]"#);
    for invalid in [rect(-1, 0, 4, 4), rect(0, 0, 5, 4), rect(0, 0, 0, 4)] {
        assert_eq!(
            square.place_in(invalid, config, flat),
            Err(GenerationError::InvalidRegion)
        );
    }
    let objects = square.place_in(world, config, flat).unwrap();
    assert!(!objects.is_empty());
    assert!(
        objects
            .iter()
            .all(|object| world.contains_rect(object.footprint.bounds(object.id.anchor).unwrap()))
    );
    let error = GenerationError::NoBiome(TilePos { x: 0, y: 0 });
    assert_eq!(
        square.place_in(world, config, |_| Err(error.clone())),
        Err(error)
    );
}

#[test]
fn invalid_configuration_is_rejected_before_generation() {
    for source in [
        r#"[(id: 1, object: "missing", density: 0.1)]"#,
        r#"[(id: 1, object: "small", density: -0.1)]"#,
        r#"[(id: 1, object: "small", density: 1.1)]"#,
        r#"[(id: 1, object: "small", density: NaN)]"#,
        r#"[(id: 1, object: "small", density: 0.1, biomes: ["missing"])]"#,
        r#"[(id: 1, object: "small", density: 0.1, height: (min: 0.0, max: 1.1))]"#,
        r#"[(id: 1, object: "small", density: 0.1, temperature: (min: 5.0, max: 5.0))]"#,
        r#"[(id: 1, object: "small", density: 0.1, max_height_difference: Some(-1.0))]"#,
        r#"[(id: 1, object: "small", density: 0.1, max_height_difference: Some(NaN))]"#,
        r#"[(id: 1, object: "small", density: 0.1), (id: 1, object: "wide", density: 0.1)]"#,
    ] {
        assert!(
            ObjectPlacer::from_ron(source, repository(), &biomes()).is_err(),
            "{source}"
        );
    }
    assert!(
        ObjectRepository::from_ron(r#"[(name: "bad", footprint: (width: 0, height: 1))]"#).is_err()
    );
    let object = repository().objects()[0].clone();
    assert!(matches!(
        ObjectRepository::new(vec![object.clone(); 2]),
        Err(ObjectsError::DuplicateName(_))
    ));
    assert!(matches!(
        ObjectRepository::new(vec![object; 65537]),
        Err(ObjectsError::TooManyObjects(_))
    ));
}

#[test]
fn bundled_definitions_resolve_and_appending_preserves_object_ids() {
    let biomes = BiomeRepository::parse();
    let placer = ObjectPlacer::parse(&biomes);
    assert_eq!(placer.objects().objects().len(), 4);
    let original = repository();
    let mut definitions = original.objects().to_vec();
    definitions.push(ObjectDefinition {
        name: "new".into(),
        footprint: Footprint {
            width: 1,
            height: 1,
        },
    });
    let appended = ObjectRepository::new(definitions).unwrap();
    for definition in original.objects() {
        assert_eq!(original.id(&definition.name), appended.id(&definition.name));
    }
}
