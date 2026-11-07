# Local world generator preview

Run from the repository root:

```sh
node tools/worldgen-preview/dev.mjs
```

Open <http://127.0.0.1:4173>. The watcher rebuilds the `worldgen` crate's preview example when Rust sources (including terrain features), biome RON definitions, its manifest, or the example change. The frontend stays available during builds. The previous generator remains available if compilation fails. The browser polls the build ID and refreshes its cache after a successful restart, keeping previous images as placeholders. Restart the dev runner after editing `dev.mjs` itself.

The HTTP adapter is `crates/worldgen/examples/worldgen_preview.rs`. It imports the same `worldgen` library used by the server, without building Bevy or the game server. Frequency is a preview adapter control; elevation is evaluated at frequency-scaled global coordinates. Tile colors come directly from the generator (`#RGB` or `#RRGGBB`) and are decoded in browser workers without biome-specific color logic. Hover a tile to inspect normalized height and daily average Celsius temperature. Unclassified tiles use the legend's neutral color and retain their height and temperature.

The sidebar adjusts average-temperature coefficients, orbital phase, axial tilt, crater sizes and density, impact basins, and both fBm noise sources (frequency, octaves, lacunarity, persistence, and independent optional seed overrides). Blank seed overrides follow the world seed, with a separate derived seed for temperature regions. Terrain coordinate scale multiplies elevation noise frequency; regional noise is sampled directly in global tile coordinates. Defaults and validation limits come from Rust metadata. Changes regenerate the visible map and are stored in the URL; Reset settings restores the current generator defaults.

Choose a **Biome file** from the RON files in `crates/worldgen/src/biomes` (default: `biomes.ron`). The **Height × temperature** chart plots the selected file's height and Celsius ranges using its colors; hover or focus a block for its name and exact ranges. The terrain key and tile tooltips also use names from that file. Switching files regenerates biome colors while preserving terrain settings, and the selection is saved as `biome` in the URL. Range upper bounds are exclusive; overlapping definitions use the first match, as in the generator.

The **Map mode** selector at the top of the map switches between **Biomes**, **Temperature**, and **Height**. Temperature uses a fixed blue-to-red scale from −50 to 50 °C, with 0 °C in the middle; values outside the scale use its end colors. Height uses grayscale from black at 0 to white at 1. Each metric view shows its scale and includes tiles with no matching biome. Hover still reports exact temperature and height. Modes reuse cached samples and switch without regenerating terrain, keep the current view, and are saved in the URL as `mode=biomes`, `mode=temperature`, or `mode=height`.

Temperature always uses the daily average for the selected season. Orbital phase 0 / 0.5 represents equinox, 0.25 northern summer, and 0.75 northern winter. Craters modify height before temperature and biome classification. Density 0 disables craters; density 4 generates four per cell. Minimum radius must not exceed maximum radius, and maximum radius × 1.5 must fit within a generation cell.

The **Impact basins** section adjusts cell size, probability, minimum/maximum radius, depth and rim height, interior smoothing, and smoothing octave count. Basins run before craters, temperature, and biome classification. Their smoothing source uses the current elevation seed, frequency, lacunarity, persistence, and terrain coordinate scale; its octave count is capped to the elevation octave count. Probability 0 disables basins, while 1 places one in every generation cell. Use **Fit world (□)** or zoom out to see these large features; the default probability 0.12 intentionally makes them rare. Minimum values must not exceed maximums, and maximum radius × 1.5 must fit within the basin cell size. Basin controls are also saved in the URL and restored by Reset settings.

The **Volcanoes** section controls cell size, probability, minimum/maximum cone radius and height, caldera radius as a fraction of cone radius, and caldera depth. Volcanoes build raised cones with summit depressions after basins and before younger craters; their height feeds temperature and biome classification. Probability 0 disables them, and 1 places one in every cell. Caldera radius 0 produces solid cones. Heights and depths use normalized height units. Maximum radius must fit within cell size, and minimum radius/height must not exceed their maximums. Controls use the `volcano_` API and URL prefix and support Reset settings. Use **Fit world (□)** to find sparse volcanoes; zoom in to see their calderas.

Panning, drawing, and requests stay within the world dimensions reported by the generator. Zooming out stops at a full-world view; the square button also fits the world. At LOD 0, cells represent exact samples. Below one pixel per tile, each cell samples a representative tile from a larger region; the tooltip identifies that tile and LOD.

The preview API is versioned by `protocol: 2` in `/api/meta?size=32`, alongside `build`, `world: {width, height}` in tiles, and `controls` describing the parameter keys, labels, groups, defaults, limits, and input steps. `/api/region` takes `min_x`, `min_y`, `max_x`, `max_y` (inclusive chunk addresses), `lod`, and any parameter keys from `controls`; omitted settings use Rust defaults. Noise keys use `elevation_` / `region_` prefixes; temperature keys use `temperature_`. Planet keys are `orbit` and `axial_tilt`. Crater keys are `crater_cell_size`, `crater_min_radius`, `crater_max_radius`, and `crater_density`. Basin controls use the `basin_` prefix, including `basin_probability`, `basin_smoothing`, and `basin_coarse_octaves`. The adapter uses only `WorldGenerator::tile_at` to obtain height, daily-average temperature, and biome. Each row-major sample is 12 bytes: status u8 (0 unclassified, 1 colored, 255 outside-world padding), RGB u8[3], normalized height f32 little-endian, and Celsius temperature f32 little-endian. Padding carries NaN metrics. Out-of-world chunks and invalid parameters are rejected. The frontend can use another implementation of this contract through its `?api=...` base URL (cross-origin providers must allow CORS).

To run a second preview without stopping an existing one:

```sh
node tools/worldgen-preview/dev.mjs --port 4183 --api-port 4184
```

Checks: `cargo test --offline -p worldgen --all-targets`, `node tools/worldgen-preview/preview.test.mjs`, and `node tools/worldgen-preview/zoom.test.mjs`.

http://127.0.0.1:4173/?seed=2653345494&chunk=32&frequency=0.025&elevation_seed=4&elevation_frequency=0.005&elevation_octaves=10&elevation_lacunarity=3&elevation_persistence=0.5&region_frequency=0.001&region_octaves=5&region_lacunarity=3&region_persistence=0.5&temperature_base=-30&temperature_solar=150&temperature_altitude=10&temperature_region=12&orbit=0.25&axial_tilt=4

http://127.0.0.1:4173/?seed=0&frequency=0.045001&water=-0.2&mountain=0.35&chunk=32
