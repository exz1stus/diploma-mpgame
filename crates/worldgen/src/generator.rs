use std::sync::Arc;

use crate::{BiomeRepository, GenerationStage, Tile, TileDraft, TilePos};

pub struct WorldGenerator {
    stages: Vec<Box<dyn GenerationStage>>,
    biomes: Arc<BiomeRepository>,
}

impl WorldGenerator {
    /// Execute stages in the supplied order. All biome maps must be resolved
    /// against this repository; pipeline assembly determines stage dependencies.
    pub fn new(stages: Vec<Box<dyn GenerationStage>>, biomes: Arc<BiomeRepository>) -> Self {
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

    pub fn biomes(&self) -> &BiomeRepository {
        &self.biomes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BasinGenerator, BiomeMap, BiomeMapDefinition, BiomeStage, ClampHeightStage,
        CraterGenerator, ElevationStage, NoiseSource, PaletteStage, PlanetParams,
        TemperatureCoefficients, TemperatureModel, TemperatureStage, VolcanoGenerator,
        WorldGeneratorConfig, WorldGeography, build_world_generator,
    };

    #[test]
    fn custom_stages_feed_climate_and_defer_biome_maps() {
        struct ConeStage(Arc<BiomeMap>);
        impl GenerationStage for ConeStage {
            fn apply(&self, pos: TilePos, tile: &mut TileDraft) {
                if pos.x == 0 {
                    tile.environment_height = tile.height;
                    tile.height += 2.0;
                    tile.temperature_offset += 12.0;
                    tile.biome_map = Some(Arc::clone(&self.0));
                }
            }
        }

        let biomes = Arc::new(BiomeRepository::parse());
        let stages: Vec<Box<dyn GenerationStage>> = vec![
            Box::new(ElevationStage::new(FlatNoise)),
            Box::new(ConeStage(Arc::new(BiomeMap::from_ron(
                r#"[(biome: "volcano", height: (min: 0.8, max: 1.0), temperature: (min: 1.0, max: 3.0))]"#,
                &biomes,
            ).unwrap()))),
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
            Box::new(BiomeStage::new(Arc::new(BiomeMap::parse(&biomes)))),
            Box::new(PaletteStage::new(Arc::clone(&biomes))),
        ];
        let generator = WorldGenerator::new(stages, Arc::clone(&biomes));
        let cone = generator.tile_at(TilePos { x: 0, y: 0 });
        assert_eq!(cone.height, 1.0);
        assert_eq!(cone.temperature, 2.0);
        assert_eq!(cone.biome, biomes.id("volcano").unwrap());
        assert_eq!(
            cone.palette_index, 3,
            "palette must use clamped height and final temperature"
        );
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
        let biomes = BiomeRepository::parse();
        let biome_map = BiomeMap::from_ron(
            r#"[
                (biome: "dark_crater_floors", height: (min: 0.0, max: 0.55), temperature: (min: -100.0, max: 100.0)),
                (biome: "volcano", height: (min: 0.55, max: 1.0), temperature: (min: -100.0, max: 100.0)),
            ]"#,
            &biomes,
        ).unwrap();
        build_world_generator(
            WorldGeneratorConfig {
                seed: 42,
                chunk_size,
                world_size_chunks: 64 / chunk_size,
                ..Default::default()
            },
            biomes,
            biome_map,
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
                biome_map: BiomeMapDefinition::default(),
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
                        "volcano"
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
