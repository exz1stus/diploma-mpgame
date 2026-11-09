use std::{collections::HashMap, sync::Arc};

use crate::{
    BiomeRepository, ChunkData, ChunkPos, GenerationError, GenerationStage, ObjectPlacer, Tile,
    TileData, TileDraft, TileMapper, TilePos, TileRect, WorldGeneratorConfig,
};

pub struct WorldGenerator {
    config: WorldGeneratorConfig,
    bounds: TileRect,
    stages: Vec<Box<dyn GenerationStage>>,
    biomes: Arc<BiomeRepository>,
    tile_mapper: TileMapper,
    object_placer: ObjectPlacer,
}

impl WorldGenerator {
    pub fn new(
        config: WorldGeneratorConfig,
        stages: Vec<Box<dyn GenerationStage>>,
        biomes: Arc<BiomeRepository>,
        tile_mapper: TileMapper,
        object_placer: ObjectPlacer,
    ) -> Self {
        let bounds = config
            .tile_bounds()
            .expect("world dimensions must be valid");
        Self {
            config,
            bounds,
            stages,
            biomes,
            tile_mapper,
            object_placer,
        }
    }

    pub fn draft_at(&self, pos: TilePos) -> TileDraft {
        let mut draft = TileDraft::default();
        for stage in &self.stages {
            stage.apply(pos, &mut draft);
        }
        draft
    }

    /// Sample completed terrain data in global coordinates, independently of chunk bounds.
    /// Pass the result to a TileMapper to select a global tile ID.
    /// Panics if no stage assigned a biome for the generated height and temperature.
    pub fn tile_data_at(&self, pos: TilePos) -> TileData {
        self.try_tile_data_at(pos)
            .expect("no biome covers the generated tile")
    }

    /// Fallible terrain sampling, also usable for diagnostics outside world bounds.
    pub fn try_tile_data_at(&self, pos: TilePos) -> Result<TileData, GenerationError> {
        let draft = self.draft_at(pos);
        Ok(TileData {
            height: draft.height,
            temperature: draft.temperature,
            biome: draft.biome.ok_or(GenerationError::NoBiome(pos))?,
        })
    }

    /// Sample and map a ground tile inside the finite world.
    pub fn tile_at(&self, pos: TilePos) -> Result<Tile, GenerationError> {
        if !self.bounds.contains(pos) {
            return Err(GenerationError::TileOutsideWorld(pos));
        }
        self.tile_mapper
            .map(&self.try_tile_data_at(pos)?)
            .ok_or(GenerationError::NoTile(pos))
    }

    /// Generate one complete chunk, independently of other requests or worker order.
    /// Objects crossing its bounds are included with their global instance IDs.
    pub fn chunk_at(&self, chunk_pos: ChunkPos) -> Result<ChunkData, GenerationError> {
        let count = i64::from(self.config.world_size_chunks);
        if !(0..count).contains(&chunk_pos.x) || !(0..count).contains(&chunk_pos.y) {
            return Err(GenerationError::ChunkOutsideWorld(chunk_pos));
        }
        let size = i64::from(self.config.chunk_size);
        let min = TilePos {
            x: chunk_pos.x * size,
            y: chunk_pos.y * size,
        };
        let bounds = TileRect {
            min,
            max: TilePos {
                x: min.x + size,
                y: min.y + size,
            },
        };
        let mut terrain: HashMap<TilePos, TileData> = HashMap::new();
        let mut sample = |pos| match terrain.entry(pos) {
            std::collections::hash_map::Entry::Occupied(entry) => Ok(entry.get().clone()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let data = self.try_tile_data_at(pos)?;
                Ok(entry.insert(data).clone())
            }
        };
        let mut tiles = Vec::new();
        for pos in bounds.positions() {
            let data = sample(pos)?;
            tiles.push(
                self.tile_mapper
                    .map(&data)
                    .ok_or(GenerationError::NoTile(pos))?,
            );
        }
        let objects = self
            .object_placer
            .place_in(bounds, self.config, &mut sample)?;
        Ok(ChunkData {
            position: chunk_pos,
            size: self.config.chunk_size,
            tiles,
            objects,
        })
    }

    pub fn biomes(&self) -> &BiomeRepository {
        &self.biomes
    }

    pub fn tile_mapper(&self) -> &TileMapper {
        &self.tile_mapper
    }

    pub fn object_placer(&self) -> &ObjectPlacer {
        &self.object_placer
    }

    pub fn config(&self) -> WorldGeneratorConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BasinGenerator, BiomeMap, BiomeMapDefinition, BiomeStage, ClampHeightStage,
        CraterGenerator, ElevationStage, NoiseSource, PlanetParams, TemperatureCoefficients,
        TemperatureModel, TemperatureStage, VolcanoGenerator, WorldGeneratorConfig, WorldGeography,
        build_world_generator,
    };

    #[test]
    fn custom_stages_feed_climate_and_defer_biome_maps() {
        struct ConeStage(Arc<BiomeMap>);
        impl GenerationStage for ConeStage {
            fn apply(&self, pos: TilePos, tile: &mut TileDraft) {
                if pos.x == 0 {
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
        ];
        let generator = WorldGenerator::new(
            WorldGeneratorConfig::default(),
            stages,
            Arc::clone(&biomes),
            TileMapper::parse(&biomes),
            ObjectPlacer::empty(),
        );
        let cone = generator.tile_data_at(TilePos { x: 0, y: 0 });
        assert_eq!(cone.height, 1.0);
        assert_eq!(cone.temperature, 2.0);
        assert_eq!(cone.biome, biomes.id("volcano").unwrap());
        let mapper = crate::TileMapper::parse(&biomes);
        assert_eq!(mapper.map(&cone), mapper.palette().tile("volcano_3"));
        let plain = generator.tile_data_at(TilePos { x: 1, y: 0 });
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
                let tile = small_chunks.tile_data_at(pos);
                assert_eq!(
                    tile,
                    large_chunks.tile_data_at(pos),
                    "chunk boundaries must not affect {pos:?}"
                );
                assert_eq!(
                    tile,
                    small_chunks.tile_data_at(pos),
                    "sampling must be repeatable"
                );
                assert!((0.0..=1.0).contains(&tile.height));
                assert!((tile.temperature + tile.height * 10.0).abs() < 1e-12);
                let baseline = disabled.tile_data_at(pos);
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

    #[test]
    fn cold_feature_caches_produce_identical_chunks_across_workers() {
        let reference = volcanic_world(16, 1.0);
        let shared = Arc::new(volcanic_world(16, 1.0));
        let positions: Vec<_> = (0..4)
            .flat_map(|y| (0..4).map(move |x| ChunkPos { x, y }))
            .collect();
        let jobs: Vec<_> = positions
            .iter()
            .map(|&pos| {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || shared.chunk_at(pos).unwrap())
            })
            .collect();
        for (job, pos) in jobs.into_iter().zip(positions) {
            assert_eq!(job.join().unwrap(), reference.chunk_at(pos).unwrap());
        }
    }
}
