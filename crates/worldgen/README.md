# World generation

Engine-independent generation shared by the server and local preview. Bevy resources, HTTP, rendering, chunks, and worker scheduling belong to consumers.

Construct `WorldGenerator::new(config, biomes, planet, elevation, temperature, craters, basins, volcanoes)` with the generation dependencies. Geography is derived from the world configuration, and crater, basin, and volcano placement use separately derived world seeds. After construction, `tile_at(TilePos)` is the only generation method.

The pipeline samples signed elevation noise, normalizes it to [0, 1], applies old impact basins, volcanic cones and calderas, and then younger crater bowls and rims in global tile coordinates, clamps height to [0, 1], calculates daily-average Celsius temperature for the configured season, and classifies the biome. `Tile` contains `height`, `temperature`, and `biome: Option<Biome>`; uncovered biome ranges still retain terrain metrics. Consumers iterate global positions to build their own chunks.

`ElevationNoise::with_config(seed, NoiseConfig)` and `TemperatureRegionsNoise::with_config(seed, NoiseConfig)` configure frequency, octaves, lacunarity, and persistence; each source exposes `default_config()`. The `new(seed)` constructors use these defaults. `ElevationSampler::new(source)` accepts any `NoiseSource`.

`TemperatureSampler::new(noise)` uses default coefficients; `with_config(coefficients, noise)` accepts base, solar, altitude, and regional coefficients. Regional noise is signed, approximately [-1, 1]. `sample_average` borrows geography and planet settings for the call. The pipeline currently treats normalized height as kilometers for altitude cooling, so the default 6.5 coefficient subtracts 0 to 6.5 degrees Celsius. There is no time-of-day mode.

`CraterGenerator` configures generation cell size, minimum and maximum radius in tiles, and density (the expected number per cell, between 0 and 4). Density zero disables craters. The default uses 1024-tile cells, 32–512-tile radii, and density 1. Features from neighboring cells are included wherever their influence reaches a sampled tile, so crater shapes cross cell and chunk boundaries. Generation uses deterministic cell seeds and does not depend on sampling order. A crater's depth is radius × 0.001, rim height is radius × 0.0002, and influence extends to 1.5 radii.

`BasinGenerator::new(coarse_elevation)` generates at most one impact basin per generation cell. Defaults are 8192-tile cells, probability 0.12, radii 800–2400, depths 0.06–0.18, rim heights 0.005–0.02, and smoothing 0.8. Probability zero disables the layer. Supply signed elevation noise with the same seed and coordinate scale as base terrain, using fewer octaves. Apply basins before craters so smoothing preserves younger crater detail. The preview exposes all basin parameters plus coarse octave count (default 2, capped at the elevation octave count).

`BiomeMap::parse()` loads bundled `src/biomes/biomes.ron`; `from_ron(source)` loads custom definitions. Ranges include their minimum and exclude their maximum; overlaps use the first match. Temperature ranges use Celsius and height ranges lie within [0, 1]. `WorldGeography` maps the square world's northwest origin to latitude/longitude, with signed chunk coordinates using Euclidean division.

`VolcanoGenerator::default()` places at most one volcano per 4096-tile cell with probability 0.15, radii 128–1024 tiles, and normalized cone heights 0.15–0.4. The caldera radius defaults to 0.2 of the cone radius, with depth 0.15. Probability zero disables the layer; caldera ratio zero produces solid cones. Cones and calderas join smoothly to surrounding terrain, including across cell and chunk boundaries. Volcanoes modify height before altitude cooling and biome classification; they do not add heat or override biome colors. The preview exposes every parameter with a `volcano_` prefix, limits maximum radius to cell size, and validates minimum/maximum ranges.

Run `cargo test --offline -p worldgen --all-targets` to check generation and the preview adapter.
