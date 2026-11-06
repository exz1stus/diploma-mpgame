use crate::temperature::{PlanetParams, TemperatureSampler};
use crate::{BiomeMap, TilePos, WorldGeography};

use super::config::WorldGeneratorConfig;
use super::noise::{ElevationNoise, NoiseSource, TemperatureRegionsNoise};
use super::sampler::{ElevationSampler, TileSampler};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainChunk {
    pub x: i32,
    pub y: i32,
    pub size: u32,
    /// Row-major tiles, starting at the chunk's lowest x and y coordinates.
    /// None preserves a cell whose climate/height has no configured biome.
    pub tiles: Vec<Option<Tile>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileData {
    /// Normalized elevation for biome classification, in [0, 1].
    pub height: f64,
    /// Daily average temperature in Celsius for the configured season.
    pub temperature: f64,
}

impl TerrainChunk {
    pub fn tile(&self, x: u32, y: u32) -> Option<Tile> {
        if x >= self.size || y >= self.size {
            return None;
        }
        self.tiles
            .get(y as usize * self.size as usize + x as usize)?
            .clone()
    }
}

pub struct WorldGenerator {
    config: WorldGeneratorConfig,
    biomes: BiomeMap,
    world_geography: WorldGeography,
    planet: PlanetParams,
    elevation: ElevationSampler,
    temperature: TemperatureSampler,
}

impl WorldGenerator {
    pub fn perlin(config: WorldGeneratorConfig) -> Self {
        Self::with_source(config, ElevationNoise::new(config.seed))
    }

    /// Use custom elevation noise while retaining seeded regional temperature noise.
    pub fn with_source(config: WorldGeneratorConfig, source: impl NoiseSource) -> Self {
        let temperature_noise = TemperatureRegionsNoise::new(config.seed ^ 0x5445_4d50);
        Self::new(
            config,
            BiomeMap::parse(),
            PlanetParams::default(),
            ElevationSampler::new(source),
            TemperatureSampler::new(temperature_noise),
        )
    }

    /// Geography is derived from the same configuration used to generate chunks.
    /// All stages are owned; sampling borrows geography/planet only for each call.
    pub fn new(
        config: WorldGeneratorConfig,
        biomes: BiomeMap,
        planet: PlanetParams,
        elevation: ElevationSampler,
        temperature: TemperatureSampler,
    ) -> Self {
        let world_geography = WorldGeography::new(config.chunk_size, config.world_size_chunks);
        Self {
            config,
            biomes,
            world_geography,
            planet,
            elevation,
            temperature,
        }
    }

    /// Raw signed elevation, also used as kilometers by the temperature stage.
    pub fn height_at(&self, x: i64, y: i64) -> f64 {
        self.elevation.sample(TilePos { x, y })
    }

    pub fn sample(&self, pos: TilePos) -> TileData {
        self.sample_with_time(pos, None)
    }

    /// Instantaneous Celsius temperature at a normalized global time in [0, 1).
    pub fn sample_instant(&self, pos: TilePos, normalized_time: f64) -> TileData {
        self.sample_with_time(pos, Some(normalized_time))
    }

    fn sample_with_time(&self, pos: TilePos, time: Option<f64>) -> TileData {
        let height = (self.elevation.sample(pos) + 1.0) * 0.5;
        TileData {
            height,
            temperature: if let Some(time) = time {
                self.temperature.sample_instant(
                    time,
                    pos,
                    height,
                    &self.world_geography,
                    &self.planet,
                )
            } else {
                self.temperature
                    .sample_average(pos, height, &self.world_geography, &self.planet)
            },
        }
    }

    pub fn tile_at(&self, pos: TilePos) -> Option<Tile> {
        self.sample_tile(pos).1
    }

    /// Sample climate/height and its display color with a single stage evaluation.
    pub fn sample_tile(&self, pos: TilePos) -> (TileData, Option<Tile>) {
        self.classify_sample(self.sample(pos))
    }

    pub fn sample_tile_instant(&self, pos: TilePos, time: f64) -> (TileData, Option<Tile>) {
        self.classify_sample(self.sample_instant(pos, time))
    }

    fn classify_sample(&self, sample: TileData) -> (TileData, Option<Tile>) {
        let tile = self
            .biomes
            .determine_biome(sample.temperature, sample.height)
            .map(|biome| Tile(biome.color.clone()));
        (sample, tile)
    }

    pub fn generate_chunk(&self, chunk_x: i32, chunk_y: i32) -> TerrainChunk {
        let size = self.config.chunk_size;
        let origin_x = i64::from(chunk_x) * i64::from(size);
        let origin_y = i64::from(chunk_y) * i64::from(size);
        let mut tiles = Vec::with_capacity(size as usize * size as usize);
        for local_y in 0..size {
            for local_x in 0..size {
                let tile = self.tile_at(TilePos {
                    x: origin_x + i64::from(local_x),
                    y: origin_y + i64::from(local_y),
                });
                tiles.push(tile);
            }
        }
        TerrainChunk {
            x: chunk_x,
            y: chunk_y,
            size,
            tiles,
        }
    }
}
