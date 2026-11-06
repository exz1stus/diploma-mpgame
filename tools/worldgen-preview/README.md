# Local world generator preview

Run from the repository root:

```sh
node tools/worldgen-preview/dev.mjs
```

Open <http://127.0.0.1:4173>. The watcher rebuilds the `worldgen` crate's preview example when Rust sources, biome RON definitions, its manifest, or the example change. The frontend stays available during builds. The previous generator remains available if compilation fails. The browser polls the build ID and refreshes its cache after a successful restart, keeping previous images as placeholders. Restart the dev runner after editing `dev.mjs` itself.

The HTTP adapter is `crates/worldgen/examples/worldgen_preview.rs`. It imports the same `worldgen` library used by the server, without building Bevy or the game server. Frequency is a preview adapter control; elevation is evaluated at frequency-scaled global coordinates. Tile colors come directly from the generator (`#RGB` or `#RRGGBB`) and are decoded in browser workers without biome-specific color logic. Hover a tile to inspect normalized height and daily average Celsius temperature. Unclassified tiles use the legend's neutral color and retain their height and temperature.

The sidebar adjusts all temperature coefficients, orbital phase, axial tilt, and both fBm noise sources (frequency, octaves, lacunarity, persistence, and independent optional seed overrides). Blank seed overrides follow the world seed, with a separate derived seed for temperature regions. Terrain coordinate scale multiplies elevation noise frequency; regional noise is sampled directly in global tile coordinates. Defaults and validation limits come from Rust metadata. Changes regenerate the visible map and are stored in the URL; Reset settings restores the current generator defaults.

Temperature display defaults to the daily average. Choose **At time of day** to inspect daily heating and cooling and enable the time control. Time is a normalized UTC day: 0 is midnight and 0.5 is noon at the prime meridian; other longitudes have different local times. The model's three-hour thermal lag still applies. In this mode, biome colors and tooltip temperatures both use instantaneous temperature. Orbital phase 0 / 0.5 represents equinox, 0.25 northern summer, and 0.75 northern winter.

Panning, drawing, and requests stay within the world dimensions reported by the generator. Zooming out stops at a full-world view; the square button also fits the world. At LOD 0, cells represent exact samples. Below one pixel per tile, each cell samples a representative tile from a larger region; the tooltip identifies that tile and LOD.

The preview API is versioned by `protocol: 2` in `/api/meta?size=32`, alongside `build`, `world: {width, height}` in tiles, and `controls` describing the parameter keys, labels, groups, defaults, limits, and input steps. `/api/region` takes `min_x`, `min_y`, `max_x`, `max_y` (inclusive chunk addresses), `lod`, and any parameter keys from `controls`; omitted settings use Rust defaults. Noise keys use `elevation_` / `region_` prefixes; temperature keys use `temperature_`. Planet/time keys are `orbit`, `axial_tilt`, `instant` (0 average, 1 instantaneous), and `time`. Each row-major sample is 12 bytes: status u8 (0 unclassified, 1 colored, 255 outside-world padding), RGB u8[3], normalized height f32 little-endian, and Celsius temperature f32 little-endian. Padding carries NaN metrics. Out-of-world chunks and invalid parameters are rejected. The frontend can use another implementation of this contract through its `?api=...` base URL (cross-origin providers must allow CORS).

To run a second preview without stopping an existing one:

```sh
node tools/worldgen-preview/dev.mjs --port 4183 --api-port 4184
```

Checks: `cargo test --offline -p worldgen --all-targets`, `node tools/worldgen-preview/preview.test.mjs`, and `node tools/worldgen-preview/zoom.test.mjs`.
