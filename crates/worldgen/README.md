# World generation

Engine-independent generation shared by the server and preview. The pipeline is:

`elevation → terrain features → height clamp → temperature → biome → palette`

`build_world_generator(config, biomes, biome_map, noise, temperature, craters, basins, volcanoes)` assembles that sequence. Supply a shared `BiomeRepository`, a default `BiomeMap` resolved against it, an elevation `NoiseSource`, a `TemperatureModel`, and the feature generators. `ElevationNoise::new(config.seed)` provides default elevation noise. `WorldGenerator` executes the supplied stages through `draft_at(pos)` or `tile_at(pos)`; consumers assemble chunks themselves.

Features only deform height, accumulate Celsius `temperature_offset`, and mark their territory with a biome map and the height before that feature. Every tile inside a feature footprint is marked, including weak effects and unchanged terrain. There are no feature-level classification thresholds or volcano-specific override hooks. In overlaps, the last feature in pipeline/cell order marks the territory. Cell and chunk caches reuse deterministic feature generation and intersection results.

After all features, height is clamped to [0, 1] and climate temperature is calculated from final height, world geography, regional noise, and planet settings, plus feature temperature offsets. `BiomeStage` then selects the territory biome using the feature map, falling back to the default map if no rule matches. It also classifies the default terrain appearance from the same final height and temperature.

`PaletteStage` applies the selected biome's appearance policy:

- No palette: keep the default terrain appearance.
- An owned palette: use it when its `palette_height_threshold` is met; otherwise keep the default terrain appearance. Zero disables the height gate.
- Within the chosen palette, the first matching height-temperature entry wins. Entry 0 is the fallback if no entry matches.

The height threshold compares final clamped height with the height before the feature marking the territory. Volcanic biome definitions use `palette_height_threshold: 0.01`, so raised volcanic terrain receives its own appearance while shallow edges and depressions inherit terrain colors. Temperature changes never hide territory markers.

Craters and basins have no owned palettes. Their deformations affect default height-temperature classification, and their biome IDs still identify the territory through the API. They introduce no painted patches. Future biome behavior can use those IDs; no biome objects are implemented.

A completed `Tile` contains `height`, `temperature`, `biome`, `surface_biome`, and `palette_index`. `biome` identifies territory; `surface_biome` identifies the appearance source. `palette_index` indexes that source's palette. The same appearance entry can later refer to a texture in a game renderer. `tile_at` requires both a biome and an appearance; `draft_at` retains metrics and territory even when appearance is unclassified.

Definitions live in:

- `src/biomes/biomes.ron`: the single repository of biome names, optional palettes, and optional palette height thresholds.
- `src/biomes/maps/default.ron`: default height-temperature selection rules.
- `src/features/{basin,crater,volcano}.ron`: generation settings, temperature offsets, and territory biome maps.

```ron
// Territory only; use the default terrain's appearance.
(name: "crater"),

// Appearance is owned by the biome, not the feature generator.
(name: "volcano", palette_height_threshold: 0.01, palette: [
    (color: "#343C47", height: (min: 0.0, max: 0.65)),
    (color: "#9AA4AA", height: (min: 0.65, max: 1.0)),
]),
```

Palette entries may also specify Celsius `temperature` ranges. Omitted ranges cover all heights or all finite temperatures. All selection uses [min, max), except height 1 is included in ranges ending at 1. Repository loading validates ranges and thresholds; map loading resolves names to compact IDs before generation.

`BiomeId` is the zero-based `u16` index in the shared repository. Existing IDs are preserved, including palette-free feature territories. Append new definitions to preserve IDs; reordering or changing generation rules requires regenerating saved terrain. `BiomeRepository::id(name)` resolves a name, and `get(id)` retrieves the definition.

`WorldGenerator::new(stages, Arc<BiomeRepository>)` supports custom pipelines. Resolve every map against that repository and keep biome selection before palette selection. `FeatureStage::new(seed, generator, chunk_size, &biomes)` adapts a feature generator to the pipeline. `TerrainFeature::apply` returns whether a tile is inside the feature territory; it must leave outside tiles unchanged.

`ElevationNoise::with_config(seed, NoiseConfig)` and `TemperatureRegionsNoise::with_config(seed, NoiseConfig)` configure noise frequency, octaves, lacunarity, and persistence. `TemperatureModel::with_config(coefficients, noise, geography, planet)` configures daily-average Celsius climate. Use `WorldGeography::new(config.chunk_size, config.world_size_chunks)` to match the world. Normalized height is currently treated as kilometers for altitude cooling.

Crater defaults favor small impacts within 24–384 tiles, use density 1.25 per 1024-tile cell, and cap depth at 0.22 and rim height at 0.06. Basins use 6144-tile cells, probability 0.2, 768–2048-tile radii, and smoothing 0.9. Volcanoes use 4096-tile cells, probability 0.2, 192–896-tile radii, 0.14–0.32 cone heights, and a caldera radius ratio of 0.28 with depth 0.18. Generation is deterministic in global coordinates and independent of chunk boundaries and sampling order.

Run `cargo test --offline -p worldgen --all-targets` to check the generator and preview adapter.
