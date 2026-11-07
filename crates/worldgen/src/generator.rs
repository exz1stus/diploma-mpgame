use crate::features::feature::{FeatureLayer, TerrainFeature, WorldBounds};
use crate::{
    BasinGenerator, Biome, BiomeMap, CraterGenerator, ElevationSampler, PlanetParams,
    TemperatureSampler, TileSampler, VolcanoGenerator, WorldGeneratorConfig, WorldGeography,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TilePos {
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i64,
    pub y: i64,
}

impl ChunkPos {
    pub fn world_bounds(&self, chunk_size: u32) -> WorldBounds {
        let min_x = self.x * chunk_size as i64;
        let min_y = self.y * chunk_size as i64;

        WorldBounds {
            min_x: min_x as f64,
            min_y: min_y as f64,
            max_x: (min_x + chunk_size as i64) as f64,
            max_y: (min_y + chunk_size as i64) as f64,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    /// Normalized elevation after terrain features, in [0, 1].
    pub height: f64,
    /// Daily average Celsius temperature for the configured season.
    pub temperature: f64,
    /// None when no biome definition covers this height and temperature.
    pub biome: Option<Biome>,
}

pub struct WorldGenerator {
    biomes: BiomeMap,
    world_geography: WorldGeography,
    planet: PlanetParams,
    elevation: ElevationSampler,
    temperature: TemperatureSampler,
    craters: FeatureLayer<CraterGenerator>,
    basins: FeatureLayer<BasinGenerator>,
    volcanoes: FeatureLayer<VolcanoGenerator>,
}

impl WorldGenerator {
    pub fn new(
        config: WorldGeneratorConfig,
        biomes: BiomeMap,
        planet: PlanetParams,
        elevation: ElevationSampler,
        temperature: TemperatureSampler,
        craters: CraterGenerator,
        basins: BasinGenerator,
        volcanoes: VolcanoGenerator,
    ) -> Self {
        assert!(craters.cell_size > 0, "crater cell size must be positive");
        assert!(craters.min_radius.is_finite() && craters.min_radius > 0.0);
        assert!(craters.max_radius.is_finite() && craters.max_radius >= craters.min_radius);
        assert!(craters.density.is_finite() && (0.0..=4.0).contains(&craters.density));
        volcanoes.validate();
        Self {
            biomes,
            world_geography: WorldGeography::new(config.chunk_size, config.world_size_chunks),
            planet,
            elevation,
            temperature,
            craters: FeatureLayer {
                seed: config.seed ^ 0x4352_4154,
                generator: craters,
            },
            basins: FeatureLayer {
                seed: config.seed ^ 0x4352_4152,
                generator: basins,
            },
            volcanoes: FeatureLayer {
                seed: config.seed ^ 0x564F_4C43,
                generator: volcanoes,
            },
        }
    }

    /// Sample elevation, basins, volcanoes, craters, average temperature, and biome.
    /// Coordinates are global tiles. Sampling is independent of chunk boundaries.
    pub fn tile_at(&self, pos: TilePos) -> Tile {
        let x = pos.x as f64;
        let y = pos.y as f64;
        let mut height = (self.elevation.sample(pos) + 1.0) * 0.5;
        let mut biome = None;
        let bounds = self
            .world_geography
            .global_to_chunk(pos)
            .world_bounds(self.world_geography.chunk_size);

        if self.basins.generator.probability > 0.0 {
            for basin in self.basins.features_in(bounds) {
                height = basin.apply_height(x, y, height);
            }
        }
        if self.craters.generator.density > 0.0 {
            for crater in self.craters.features_in(bounds) {
                height = crater.apply_height(x, y, height);
            }
        }
        if self.volcanoes.generator.probability > 0.0 {
            for volcano in self.volcanoes.features_in(bounds) {
                height = volcano.apply_height(x, y, height);
            }
        }
        let height = height.clamp(0.0, 1.0);
        let temperature =
            self.temperature
                .sample_average(pos, height, &self.world_geography, &self.planet);
        biome = self.biomes.determine_biome(temperature, height).cloned();
        Tile {
            height,
            temperature,
            biome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NoiseSource, Range, TemperatureCoefficients};

    struct FlatNoise;
    impl NoiseSource for FlatNoise {
        fn sample(&self, _: f64, _: f64) -> f64 {
            0.0
        }
    }

    fn volcanic_world(chunk_size: u32, probability: f64) -> WorldGenerator {
        let mut basins = BasinGenerator::new(FlatNoise);
        basins.probability = 0.0;
        let biome = |name: &str, min, max| Biome {
            name: name.into(),
            color: "#123456".into(),
            height: Range { min, max },
            temperature: Range {
                min: -100.0,
                max: 100.0,
            },
        };
        WorldGenerator::new(
            WorldGeneratorConfig {
                seed: 42,
                chunk_size,
                world_size_chunks: 64 / chunk_size,
                ..Default::default()
            },
            BiomeMap::new(vec![
                biome("low", 0.0, 0.55),
                biome("volcanic_highlands", 0.55, 1.0),
            ])
            .unwrap(),
            PlanetParams::default(),
            ElevationSampler::new(FlatNoise),
            TemperatureSampler::with_config(
                TemperatureCoefficients {
                    base: 0.0,
                    solar: 0.0,
                    region: 0.0,
                    altitude: 10.0,
                },
                FlatNoise,
            ),
            CraterGenerator {
                density: 0.0,
                ..Default::default()
            },
            basins,
            VolcanoGenerator {
                cell_size: 64,
                probability,
                min_radius: 48.0,
                max_radius: 64.0,
                ..Default::default()
            },
        )
    }

    #[test]
    fn volcanoes_cross_chunks_and_feed_temperature_and_biomes() {
        let small_chunks = volcanic_world(16, 1.0);
        let large_chunks = volcanic_world(32, 1.0);
        let disabled = volcanic_world(16, 0.0);
        let mut raised = 0;
        for y in (0..64).step_by(3) {
            for x in [0, 15, 16, 31, 32, 47, 48, 63] {
                let pos = TilePos { x, y };
                let tile = small_chunks.tile_at(pos);
                assert_eq!(
                    tile,
                    large_chunks.tile_at(pos),
                    "chunk boundaries must not affect {pos:?}"
                );
                assert_eq!(
                    tile,
                    small_chunks.tile_at(pos),
                    "sampling must be repeatable"
                );
                assert!((0.0..=1.0).contains(&tile.height));
                assert!((tile.temperature + tile.height * 10.0).abs() < 1e-12);
                let baseline = disabled.tile_at(pos);
                assert_eq!(baseline.height, 0.5);
                if tile.height > 0.55 {
                    raised += 1;
                    assert_eq!(tile.biome.as_ref().unwrap().name, "volcanic_highlands");
                    assert!(tile.temperature < baseline.temperature);
                }
            }
        }
        assert!(
            raised > 0,
            "enabled volcanoes must raise terrain and change biomes"
        );
    }
}
