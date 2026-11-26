# Server world config overrides

World generation defaults live in [`crates/worldgen/config/`](../../worldgen/config). A file you
put in this folder **replaces** the default with the same relative path. Any file not here comes
from the defaults.

| File                     | Declares                                                       |
| ------------------------ | -------------------------------------------------------------- |
| `world.ron`              | seed, world size, chunk size, elevation noise, climate model   |
| `biomes.ron`             | biomes (list order = biome ID) and the objects each one spawns |
| `biome_map.ron`          | terrain biome ranges (height × temperature, first match wins)  |
| `objects.ron`            | object kinds and footprints (order = object ID), shared spawn sets |
| `features/craters.ron`   | crater settings and their territory biome map                  |
| `features/basins.ron`    | basin settings and their territory biome map                   |
| `features/volcanoes.ron` | volcano settings and their territory biome map                 |
| `placements.ron`         | fixed objects placed once per world (the ship at spawn, its engine at a pole) |

To change the world seed, for example, copy `world.ron` here and edit `seed`. Biome and object
IDs are list positions, so append new entries to keep saved chunks valid.

Point the server at a different folder with `WORLDGEN_CONFIG=/path/to/folder`.

Appearance (textures, colors) belongs to the client: see `crates/client/assets/world/`.
