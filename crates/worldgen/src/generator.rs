use std::sync::Arc;

use crate::{BiomeMap, GenerationStage, Tile, TileDraft, TilePos};

pub struct WorldGenerator {
    stages: Vec<Box<dyn GenerationStage>>,
    biomes: Arc<BiomeMap>,
}

impl WorldGenerator {
    /// Execute stages in the supplied order. Biome stages and feature IDs must
    /// use the supplied registry; pipeline assembly determines stage dependencies.
    pub fn new(stages: Vec<Box<dyn GenerationStage>>, biomes: Arc<BiomeMap>) -> Self {
        Self { stages, biomes }
    }

    /// Run every stage without requiring biome coverage, for tools such as previews.
    pub fn draft_at(&self, pos: TilePos) -> TileDraft {
        let mut draft = TileDraft::default();
        for stage in &self.stages {
            stage.apply(pos, &mut draft);
        }
        draft
    }

    /// Sample a completed tile in global coordinates, independently of chunk bounds.
    /// Panics if no stage assigned a biome for the generated height and temperature.
    pub fn tile_at(&self, pos: TilePos) -> Tile {
        self.draft_at(pos).finish()
    }

    pub fn biomes(&self) -> &BiomeMap {
        &self.biomes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BasinGenerator, Biome, BiomeId, BiomeStage, ClampHeightStage, CraterGenerator,
        ElevationStage, NoiseSource, PlanetParams, Range, TemperatureCoefficients,
        TemperatureModel, TemperatureStage, VolcanoGenerator, WorldGeneratorConfig, WorldGeography,
        build_world_generator_with_elevation,
    };

    #[test]
    fn custom_stages_feed_climate_and_preserve_biome_overrides() {
        struct ConeStage;
        impl GenerationStage for ConeStage {
            fn apply(&self, pos: TilePos, tile: &mut TileDraft) {
                if pos.x == 0 {
                    tile.height += 2.0;
                    tile.biome_override = Some(BiomeId(0));
                }
            }
        }

        let biomes = Arc::new(BiomeMap::parse());
        let stages: Vec<Box<dyn GenerationStage>> = vec![
            Box::new(ElevationStage::new(FlatNoise)),
            Box::new(ConeStage),
            Box::new(ClampHeightStage),
            Box::new(TemperatureStage::new(TemperatureModel::with_config(
                TemperatureCoefficients {
                    base: 0.0,
                    solar: 0.0,
                    region: 0.0,
                    altitude: 10.0,
                },
                FlatNoise,
                WorldGeography::new(16, 4),
                PlanetParams::default(),
            ))),
            Box::new(BiomeStage::new(Arc::clone(&biomes))),
        ];
        let generator = WorldGenerator::new(stages, biomes);
        let cone = generator.tile_at(TilePos { x: 0, y: 0 });
        assert_eq!(cone.height, 1.0);
        assert_eq!(cone.temperature, -10.0);
        // Height 1 is outside the classifier's exclusive upper limit, so the
        // final tile can only finish if its feature override was preserved.
        assert_eq!(cone.biome, BiomeId(0));
        let plain = generator.tile_at(TilePos { x: 1, y: 0 });
        assert_eq!(plain.height, 0.5);
        assert_eq!(plain.temperature, -5.0);
        assert_ne!(plain.biome, cone.biome);
    }

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
        build_world_generator_with_elevation(
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
            FlatNoise,
            TemperatureModel::with_config(
                TemperatureCoefficients {
                    base: 0.0,
                    solar: 0.0,
                    region: 0.0,
                    altitude: 10.0,
                },
                FlatNoise,
                WorldGeography::new(chunk_size, 64 / chunk_size),
                PlanetParams::default(),
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
                    assert_eq!(
                        small_chunks.biomes().get(tile.biome).unwrap().name,
                        "volcanic_highlands"
                    );
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
