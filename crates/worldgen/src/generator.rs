use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use crate::{
    BiomeRepository, ChunkData, ChunkPos, GenerationError, GenerationStage, ObjectKind,
    ObjectPlacer, TileData, TileDraft, TilePos, TileRect, TileSample, WorldGeneratorConfig,
    WorldManifest,
};

/// Samples terrain through ordered stages and serves complete chunks.
/// Immutable and shareable between worker threads.
pub struct WorldGenerator {
    config: WorldGeneratorConfig,
    bounds: TileRect,
    stages: Vec<Box<dyn GenerationStage>>,
    biomes: Arc<BiomeRepository>,
    object_placer: ObjectPlacer,
}

impl WorldGenerator {
    pub fn new(
        config: WorldGeneratorConfig,
        stages: Vec<Box<dyn GenerationStage>>,
        biomes: Arc<BiomeRepository>,
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

    /// Completed terrain data in global coordinates, independently of chunk bounds.
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

    /// One tile of the finite world, as stored in chunks.
    pub fn sample_at(&self, pos: TilePos) -> Result<TileSample, GenerationError> {
        if !self.bounds.contains(pos) {
            return Err(GenerationError::TileOutsideWorld(pos));
        }
        Ok(TileSample::from_data(&self.try_tile_data_at(pos)?))
    }

    /// Generate one complete chunk, independently of other requests or worker order.
    /// Objects crossing its bounds are included with their global instance IDs.
    pub fn chunk_at(&self, chunk_pos: ChunkPos) -> Result<ChunkData, GenerationError> {
        let bounds = self.chunk_bounds(chunk_pos)?;
        let mut terrain: HashMap<TilePos, TileData> = HashMap::new();
        let mut sample = |pos| match terrain.entry(pos) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => Ok(entry.insert(self.try_tile_data_at(pos)?).clone()),
        };
        let tiles = bounds
            .positions()
            .map(|pos| sample(pos).map(|data| TileSample::from_data(&data)))
            .collect::<Result<_, _>>()?;
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

    /// Names behind the IDs in chunk data, for consumers that present the world.
    pub fn manifest(&self) -> WorldManifest {
        WorldManifest {
            chunk_size: self.config.chunk_size,
            world_size_chunks: self.config.world_size_chunks,
            biomes: self
                .biomes
                .biomes()
                .iter()
                .map(|biome| biome.name.clone())
                .collect(),
            objects: self
                .object_placer
                .objects()
                .objects()
                .iter()
                .map(|object| ObjectKind {
                    name: object.name.clone(),
                    footprint: object.footprint,
                })
                .collect(),
        }
    }

    pub fn biomes(&self) -> &BiomeRepository {
        &self.biomes
    }

    pub fn object_placer(&self) -> &ObjectPlacer {
        &self.object_placer
    }

    pub fn config(&self) -> WorldGeneratorConfig {
        self.config
    }

    fn chunk_bounds(&self, chunk_pos: ChunkPos) -> Result<TileRect, GenerationError> {
        let count = i64::from(self.config.world_size_chunks);
        if !(0..count).contains(&chunk_pos.x) || !(0..count).contains(&chunk_pos.y) {
            return Err(GenerationError::ChunkOutsideWorld(chunk_pos));
        }
        let size = i64::from(self.config.chunk_size);
        let min = TilePos {
            x: chunk_pos.x * size,
            y: chunk_pos.y * size,
        };
        Ok(TileRect {
            min,
            max: TilePos {
                x: min.x + size,
                y: min.y + size,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BiomeMap, BiomeMapDefinition, BiomeStage, ClampHeightStage, ElevationStage, NoiseSource,
        PlanetParams, TemperatureCoefficients, TemperatureModel, TemperatureStage, TerrainSources,
        WorldConfig, WorldGeneratorConfig, WorldGeography,
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
                cooling_only(),
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
            ObjectPlacer::empty(),
        );
        let cone = generator.tile_data_at(TilePos { x: 0, y: 0 });
        assert_eq!(cone.height, 1.0);
        assert_eq!(cone.temperature, 2.0);
        assert_eq!(cone.biome, biomes.id("volcano").unwrap());
        let plain = generator.tile_data_at(TilePos { x: 1, y: 0 });
        assert_eq!(plain.height, 0.5);
        assert_eq!(plain.temperature, -5.0);
        assert_ne!(plain.biome, cone.biome);
    }

    #[test]
    fn samples_and_manifest_describe_chunk_ids_by_name() {
        let generator = volcanic_world(16, 0.0);
        let manifest = generator.manifest();
        assert_eq!(manifest.chunk_size, 16);
        assert_eq!(manifest.world_size_chunks, 4);
        let chunk = generator.chunk_at(ChunkPos { x: 1, y: 2 }).unwrap();
        assert_eq!(chunk.tiles.len(), 16 * 16);
        for sample in &chunk.tiles {
            assert_eq!(
                manifest.biome_name(sample.biome),
                Some("dark_crater_floors")
            );
            assert!((sample.height() - 0.5).abs() < 1e-4);
        }
        assert_eq!(
            generator.sample_at(TilePos { x: 20, y: 40 }).unwrap(),
            chunk.tiles[8 * 16 + 4]
        );
        for object in &chunk.objects {
            assert_eq!(
                manifest.object(object.kind).unwrap().footprint,
                object.footprint
            );
        }
        assert!(matches!(
            generator.sample_at(TilePos { x: 64, y: 0 }),
            Err(GenerationError::TileOutsideWorld(_))
        ));
    }

    struct FlatNoise;
    impl NoiseSource for FlatNoise {
        fn sample(&self, _: f64, _: f64) -> f64 {
            0.0
        }
    }

    fn cooling_only() -> TemperatureCoefficients {
        TemperatureCoefficients {
            base: 0.0,
            solar: 0.0,
            region: 0.0,
            altitude: 10.0,
        }
    }

    fn flat_sources() -> TerrainSources {
        TerrainSources {
            elevation: Box::new(FlatNoise),
            basin_floor: Box::new(FlatNoise),
            temperature_regions: Box::new(FlatNoise),
        }
    }

    fn volcanic_world(chunk_size: u32, probability: f64) -> WorldGenerator {
        let mut config = WorldConfig::bundled();
        config.world.seed = 42;
        config.world.chunk_size = chunk_size;
        config.world.world_size_chunks = 64 / chunk_size;
        config.world.climate.coefficients = cooling_only();
        config.biome_map = BiomeMapDefinition::from_ron(
            r#"[
                (biome: "dark_crater_floors", height: (min: 0.0, max: 0.55), temperature: (min: -100.0, max: 100.0)),
                (biome: "volcano", height: (min: 0.55, max: 1.0), temperature: (min: -100.0, max: 100.0)),
            ]"#,
        )
        .unwrap();
        config.basins.probability = 0.0;
        config.craters.density = 0.0;
        config.volcanoes.cell_size = 64;
        config.volcanoes.probability = probability;
        config.volcanoes.biome_map = BiomeMapDefinition::default();
        config.volcanoes.min_radius = 48.0;
        config.volcanoes.max_radius = 64.0;
        config.build_with(flat_sources()).unwrap()
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
