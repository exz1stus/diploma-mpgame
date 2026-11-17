# World generation

Engine-independent, deterministic world generation. The crate serves **chunk data only**:
the biome and height of every tile, plus the objects anchored there. It contains no appearance
data. Consumers (the server, which streams chunks, and the client, which draws them) map biome
and object *names* to their own assets.

```text
elevation → terrain features → height clamp → temperature → biome → TileData
                                                                     ├─ TileSample { biome, height }
                                                                     └─ ObjectPlacer → objects
                                                                                         ↓
                                                                                     ChunkData
```

## Configuration

The whole world is declared in [`config/`](config). Each file owns one concern:

| File                     | Declares                                                               |
| ------------------------ | ---------------------------------------------------------------------- |
| `world.ron`              | generator version, seed, world/chunk size, elevation noise, climate     |
| `biomes.ron`             | biomes (list order = `BiomeId`) and the objects each biome spawns       |
| `biome_map.ron`          | terrain biome ranges: height × temperature, first match wins            |
| `objects.ron`            | object kinds and footprints (order = `ObjectId`), shared spawn sets     |
| `features/craters.ron`   | crater settings and the biome map of crater territory                  |
| `features/basins.ron`    | basin settings and the biome map of basin territory                    |
| `features/volcanoes.ron` | volcano settings and the biome map of volcano territory                |

```ron
// biomes.ron
(
    defaults: (object_sets: ["common_rocks"]),   // sets for biomes that don't list their own
    biomes: [
        (name: "ochre_dust_plains", objects: [
            (object: "tree", density: 0.005, temperature: (min: 2.0, max: 20.0)),
        ]),
        (name: "iron_rich_regolith", object_sets: ["common_rocks", "iron_deposits"]),
        (name: "crater", object_sets: []),        // no objects
    ],
)
// objects.ron
(
    kinds: [(name: "rock", footprint: (width: 1, height: 1)), ...],
    sets: { "common_rocks": [(object: "rock", density: 0.002), ...] },
)
```

A spawn is `(object, density, [height], [temperature], [max_height_difference])`. `density` is the
proposal probability per anchor tile, before overlap rejection. Omitted ranges cover every normalized
height and finite temperature. Every tile under a footprint must allow the spawn. A biome's own spawn
is limited to that biome. A set's spawn may straddle every biome that includes the set. `Option`
fields may omit `Some(...)`.

Placement rule IDs come from a stable hash of `(biome or set name, object name)`. Reordering files
doesn't change placement, but renaming a biome, set or object does. Biome and object IDs are list
positions, so append new entries to keep saved worlds valid. Bump `generator_version` whenever the
output for a seed changes.

### Overrides

```rust
let source = worldgen::ConfigSource::with_overrides("crates/server/worldgen")?; // or ConfigSource::bundled()
let config = worldgen::WorldConfig::load(&source)?;   // parse every file
let generator = config.build()?;                      // validate references, assemble the pipeline
```

Each file in the override folder (same relative path) replaces its default. `source.overridden()`
lists which ones did. Errors name the offending file. `WorldConfig` is plain data, so tools can
modify fields before building. `config.build_with(TerrainSources { .. })` substitutes custom noise
fields (the preview uses this for its coordinate-frequency control).

## Chunks

```rust
let chunk = generator.chunk_at(worldgen::ChunkPos { x: 2, y: 3 })?;
let manifest = generator.manifest();  // chunk size + names behind biome and object IDs
for sample in &chunk.tiles {          // row-major, y * size + x, chunk-local
    let biome = manifest.biome_name(sample.biome);
    let height = sample.height();     // f32 in [0, 1], quantized to u16 on the wire
}
for object in &chunk.objects {
    // Includes objects anchored in neighbouring chunks; deduplicate by object.id.
}
```

`tile_data_at(pos)` and `try_tile_data_at(pos)` return full `TileData { height, temperature, biome }`
at global coordinates. `sample_at(pos)` returns the stored `TileSample` inside the finite world.
`draft_at(pos)` also covers locations that no biome map classifies.

Each chunk request caches its terrain samples locally. Object footprint checks reuse them,
including terrain sampled just beyond the chunk.

## Pipeline details

Features deform height, accumulate a Celsius `temperature_offset`, and mark their territory with
their own biome map. In overlaps, the last feature in pipeline/cell order marks the territory.
After features, height is clamped to [0, 1]. Temperature comes from final height, geography,
regional noise, planet settings and feature offsets. `BiomeStage` evaluates the feature map first
and falls back to `biome_map.ron`. All ranges are `[min, max)`, except that height 1 belongs to
ranges ending at 1.

Object overlaps use a bounded priority rule: a candidate is accepted only if no overlapping,
terrain-valid candidate has a higher priority (ties broken by instance ID). Instance IDs combine
the rule ID with the global anchor. Spawn and priority rolls are hashed from seed, version, rule ID
and anchor, so chunk partitioning and worker scheduling never change placement.

`WorldGenerator::new(config, stages, Arc<BiomeRepository>, object_placer)` builds custom pipelines,
for example in tests. `FeatureStage::new(seed, generator, chunk_size, &biomes)` adapts any
`FeatureGenerator`. `TerrainFeature::apply` must leave tiles outside its territory unchanged.

Run `cargo test --offline -p worldgen --all-targets` to check config loading, generation, placement,
chunk borders, concurrency, serialization and the preview adapter.
