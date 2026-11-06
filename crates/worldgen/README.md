# World generation

Engine-independent generation shared by the server and local preview. Dependencies provide noise generation and RON/Serde biome definitions; Bevy resources, HTTP, rendering, and worker scheduling belong to consumers.

`WorldGenerator::perlin(config)` installs elevation noise and separately seeded regional temperature noise. `WorldGenerator::with_source(config, source)` replaces the elevation source; `WorldGenerator::new(config, biomes, planet, elevation, temperature)` accepts all generation dependencies. Geography is derived from the configuration. The generator owns all stages, geography, and planet settings; there are no internal self-references.

`ElevationNoise::with_config(seed, NoiseConfig)` and `TemperatureRegionsNoise::with_config(seed, NoiseConfig)` configure frequency, octaves, lacunarity, and persistence; each source exposes `default_config()`. The existing `new(seed)` constructors retain their defaults. `WorldGenerator::sample_instant(pos, normalized_time)` and `sample_tile_instant(pos, normalized_time)` provide instantaneous temperature and classification using the same terrain and planet as daily-average sampling.

`height_at(x, y)` returns raw signed elevation at global tile coordinates. `sample(TilePos)` returns elevation normalized from [-1, 1] to [0, 1] and daily average Celsius temperature. Raw elevation currently also represents signed kilometers for temperature's altitude adjustment. `tile_at(TilePos)` classifies that data into `Option<Tile>`, where the tile contains a biome name. `generate_chunk(x, y)` returns a row-major `Vec<Option<Tile>>`; gaps in the biome definitions remain `None` while preserving every cell's position.

`TemperatureSampler::new(noise)` uses default coefficients; `TemperatureSampler::with_config(coefficients, noise)` accepts custom coefficients. Regional noise must be signed (approximately [-1, 1]). Pass `&WorldGeography` and `&PlanetParams` to `sample_average` or `sample_instant`, along with position and height in kilometers. Sampling borrows these inputs only for the call, so the sampler can be owned by a generator without lifetime constraints.

Configuration retains the generator version, u64 seed, world size, and chunk size. World size and version are metadata for consumers; sampling does not clip coordinates or select historical algorithms. The noise library uses u32 seeds, so both halves of the u64 seed are XOR-folded; different u64 seeds can produce the same noise seed.

The server wraps `WorldGenerator` in a local Bevy resource. The preview example imports this crate directly and adds its own sampling frequency and display thresholds.

```sh
cargo test --offline -p worldgen --all-targets
node tools/worldgen-preview/dev.mjs
```

Add future generation stages alongside the elevation sampler and compose them in `WorldGenerator`. Keep engine and transport dependencies outside this crate.

`BiomeMap::parse()` loads the bundled `src/biomes/biomes.ron`; `BiomeMap::from_ron(source)` loads custom definitions. Call `map.determine_biome(temperature, elevation)` with temperature in Celsius and elevation normalized to [0, 1) to receive `Option<&Biome>`. The generator supplies these units during classification. Temperature ranges can contain any finite ordered Celsius values, while height ranges must stay within [0, 1]. Ranges include their minimum and exclude their maximum. Overlaps use the first matching definition; gaps return `None` (the bundled definitions have a gap at elevation [0.75, 0.8), temperature [0, 13) Celsius).

The bundled temperature thresholds use `Celsius = (former_normalized - 0.45) * 100`. This chosen mapping places the icy boundary at 0°C and uses a 100°C span, referencing the temperature sampler's default base and solar coefficient. The old boundaries 0, 0.45, 0.58, 0.65, and 1 become -45°C, 0°C, 13°C, 20°C, and 55°C. These are fixed biome thresholds, not bounds on the sampler's possible output; temperatures outside [-45, 55) have no matching bundled biome.
