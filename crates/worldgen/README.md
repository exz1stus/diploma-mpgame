# World generation

Engine-independent generation shared by the server and preview. A chunk request assembles terrain, mapped ground tiles, and placed objects:

```text
elevation → terrain features → height clamp → temperature → biome → TileData
                                                                     ├─ TileMapper   → Tile IDs
                                                                     └─ ObjectPlacer → instances
                                                                                         ↓
                                                                                     ChunkData
```

`build_world_generator(config, biomes, biome_map, noise, temperature, craters, basins, volcanoes)` assembles the terrain stages. `WorldGenerator::tile_data_at(pos)` returns only `TileData { height, temperature, biome }` at global coordinates. `draft_at(pos)` also supports inspecting locations with no matching biome. `tile_at(pos)` returns `Result<Tile, GenerationError>`, and `chunk_at(ChunkPos)` returns `Result<ChunkData, GenerationError>`. Both validate finite-world bounds and report missing classifications or mappings. `try_tile_data_at` supplies fallible raw terrain sampling; raw sampling also supports diagnostic coordinates outside the finite world.

Features deform height, accumulate Celsius `temperature_offset`, and mark their territory with a biome map. Every tile inside a feature footprint is marked, including weak effects and unchanged terrain. In overlaps, the last feature in pipeline/cell order marks the territory. Cell and chunk caches reuse deterministic feature generation and intersection results.

After features, height is clamped to [0, 1] and temperature is calculated from final height, geography, regional noise, planet settings, and feature offsets. `BiomeStage` selects a biome using the feature map, falling back to the default map if no rule matches. Biomes contain no colors, palette entries, or appearance thresholds.

`TileMapper` is a separate final system, called after generation. It reads completed `TileData` and returns `Option<Tile>`. `Tile { id: u16 }` indexes the whole `TilePalette`; it is independent of biome IDs. The first matching tile rule wins. Rules may restrict biome, final height, and final Celsius temperature; an omitted biome matches any biome. Uncovered data returns `None`, with no implicit tile-zero fallback.

```rust
let mapper = generator.tile_mapper();
let data = generator.tile_data_at(worldgen::TilePos { x: 10, y: 20 });
if let Some(tile) = mapper.map(&data) {
    let appearance = mapper.palette().get(tile).unwrap();
    // Use tile.id for game rendering, or appearance.color for the preview.
}
```

Changing palettes or mapping rules does not require regenerating terrain data. `TilePalette::from_ron` loads appearance independently of biomes. `TileMapper::from_ron(source, palette, biomes)` validates rule ranges and resolves tile and biome names before sampling. Multiple rules and biomes may reference the same global tile.

Definitions live in:

- `src/biomes/biomes.ron`: biome names and identity only.
- `src/biomes/maps/default.ron`: default height-temperature biome classification.
- `src/features/{basin,crater,volcano}.ron`: feature settings, temperature offsets, and territory biome maps.
- `src/tiles/palette.ron`: a flat palette of named tiles and their preview colors.
- `src/tiles/map.ron`: independent rules mapping final terrain data to palette tiles.
- `src/objects/objects.ron`: named object types and rectangular ground footprints.
- `src/objects/placement.ron`: stable rule IDs, proposal density, terrain restrictions, and optional footprint height limits.

```ron
// palette.ron
[
    (name: "ash", color: "#343C47"),
    (name: "summit", color: "#9AA4AA"),
]
// map.ron
[
    (tile: "summit", biome: Some("volcano"), height: (min: 0.65, max: 1.0)),
    (tile: "ash", biome: Some("volcano")),
]
```

Omitted tile-rule ranges cover all normalized heights and finite temperatures. All ranges use [min, max), except height 1 is included in ranges ending at 1. Bundled rules map volcanic biomes by final height and temperature; crater and basin territories use climate-based terrain fallback rules. The former pre-feature uplift threshold was removed: mapping depends only on the three completed terrain fields, so even a flat volcanic territory can select a volcanic tile.

`BiomeId` and `Tile.id` are independent zero-based `u16` indices. Biome IDs retain their existing order; tile IDs use the new flat palette order. Append definitions to preserve IDs. Reordering biome definitions requires regenerating saved biome data; reordering the tile palette requires remapping or migrating saved tile IDs.

`WorldGenerator::new(config, stages, Arc<BiomeRepository>, tile_mapper, object_placer)` supports custom pipelines and catalogs. Use `ObjectPlacer::empty()` to disable objects. Resolve every biome map against that repository. `FeatureStage::new(seed, generator, chunk_size, &biomes)` adapts a feature generator to the pipeline. `TerrainFeature::apply` returns whether a tile is inside the territory and must leave outside tiles unchanged. Tile mapping and placement run after terrain stages and remain separate systems owned by the generator. The server shares the complete generator through `Arc`.

A single-chunk call returns ground tiles in row-major order (`y * size + x`) and complete object instances:

```rust
let chunk = generator.chunk_at(worldgen::ChunkPos { x: 2, y: 3 })?;
for object in &chunk.objects {
    let anchor = object.id.anchor;
    let owner = object.owner_chunk(chunk.size);
    // Deduplicate object.id when multiple loaded chunks reference the same object.
}
```

Each chunk request caches sampled `TileData` locally. Ground mapping and object footprint checks reuse those samples, including terrain sampled just beyond the chunk. No global mutable occupancy grid is needed.

Object definitions and placement rules load independently of palettes:

```ron
// objects.ron
[(name: "large_rock", footprint: (width: 2, height: 3))]
// placement.ron
[(id: 101, object: "large_rock", density: 0.015,
  biomes: ["weathered_basalt", "volcanic_highlands"],
  height: (min: 0.4, max: 1.0),
  temperature: (min: -20.0, max: 30.0),
  max_height_difference: Some(0.03))]
```

`density` is proposal probability per eligible anchor, before overlap rejection; it is not a target occupied-area fraction. Zero disables a rule, and one proposes at every anchor. Empty biome lists allow every biome, omitted ranges cover normalized heights and finite temperatures, and an omitted height-difference limit allows any slope. Every occupied tile must pass the rule. Footprints extending beyond the world are rejected.

The anchor is the footprint's minimum X/Y tile. Object type IDs are append-only `u16` repository indices. Instance IDs combine an explicit placement rule ID with a global anchor, scoped to a world seed and generator version. Spawn rolls and priority rolls use separate deterministic hash streams derived from the seed, version, rule ID, and anchor. File order, chunk partitioning, worker scheduling, and palette changes do not change placement.

Overlaps use a bounded priority rule: accept a candidate only if no overlapping terrain-valid candidate has a higher priority, breaking ties by instance ID. Compare all valid proposals, including those that themselves lose. This deliberately leaves some additional gaps, but avoids acceptance chains extending arbitrarily far beyond a requested chunk. All object types currently share one occupancy layer.

Queries include potential cross-border anchors and all their overlapping competitors. The margin follows the maximum enabled footprint size, including objects larger than a chunk. Every response includes all accepted objects intersecting its bounds, even when anchored elsewhere. Deduplicate by instance ID and keep references from loaded chunks. The anchor's owner chunk is bookkeeping, not a prerequisite for displaying the object. Generation returns the immutable base world; gameplay persistence applies removed or modified object IDs afterward.

The server's `ChunkGenerationJobs` resource supplies bounded request and result queues backed by a fixed worker pool. Submit `ChunkRequest { id, position }` with `request`; a full queue returns the request for retry. Poll `try_recv` from a game system and match the correlation ID before consuming `ChunkResult`. Workers generate plain data; network transport, client object references, and ECS spawning consume this interface separately. Queue overload never blocks the main loop, and worker threads exit when the resource is dropped.

`ElevationNoise::with_config(seed, NoiseConfig)` and `TemperatureRegionsNoise::with_config(seed, NoiseConfig)` configure noise frequency, octaves, lacunarity, and persistence. `TemperatureModel::with_config(coefficients, noise, geography, planet)` configures daily-average Celsius climate. Use `WorldGeography::new(config.chunk_size, config.world_size_chunks)` to match the world. Normalized height is currently treated as kilometers for altitude cooling.

Crater defaults favor small impacts within 24–384 tiles, use density 1.25 per 1024-tile cell, and cap depth at 0.22 and rim height at 0.06. Basins use 6144-tile cells, probability 0.2, 768–2048-tile radii, and smoothing 0.9. Volcanoes use 4096-tile cells, probability 0.2, 192–896-tile radii, 0.14–0.32 cone heights, and a caldera radius ratio of 0.28 with depth 0.18. Generation is deterministic in global coordinates and independent of chunk boundaries and sampling order.

Run `cargo test --offline -p worldgen --all-targets` to check generation, placement, chunk borders, concurrency, serialization, and the preview adapter. `cargo test --offline -p server game::world::jobs::tests` checks the server worker queue.
