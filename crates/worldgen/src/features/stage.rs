use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::feature::{FeatureGenerator, FeatureLayer, TerrainFeature};
use crate::{BiomeMap, BiomeRepository, ChunkPos, GenerationStage, TileDraft, TilePos};

pub struct FeatureStage<G: FeatureGenerator> {
    layer: FeatureLayer<G>,
    chunk_size: u32,
    biome_map: Arc<BiomeMap>,
    // cache features for each chuck, to avoid mulitple intersection lookups
    chunk_features: Mutex<HashMap<ChunkPos, Arc<[G::Feature]>>>,
}

impl<G: FeatureGenerator> FeatureStage<G> {
    pub fn new(seed: u64, generator: G, chunk_size: u32, biomes: &BiomeRepository) -> Self {
        assert!(chunk_size > 0, "chunk_size must be positive");
        let biome_map = Arc::new(
            generator
                .biome_map()
                .resolve(biomes)
                .expect("feature biome map must reference valid biomes and ranges"),
        );
        Self {
            layer: FeatureLayer::new(seed, generator),
            chunk_size,
            biome_map,
            chunk_features: Mutex::new(HashMap::new()),
        }
    }

    fn features_for_chunk(&self, chunk: ChunkPos) -> Arc<[G::Feature]> {
        let cache = self
            .chunk_features
            .lock()
            .expect("chunk feature cache poisoned");
        if let Some(features) = cache.get(&chunk) {
            return Arc::clone(features);
        }
        drop(cache);
        let features: Arc<[G::Feature]> = self
            .layer
            .features_in(chunk.world_bounds(self.chunk_size))
            .into();
        let mut cache = self
            .chunk_features
            .lock()
            .expect("chunk feature cache poisoned");
        Arc::clone(cache.entry(chunk).or_insert(features))
    }
}

impl<G: FeatureGenerator> GenerationStage for FeatureStage<G> {
    fn apply(&self, pos: TilePos, tile: &mut TileDraft) {
        let size = i64::from(self.chunk_size);
        let chunk = ChunkPos {
            x: pos.x.div_euclid(size),
            y: pos.y.div_euclid(size),
        };
        for feature in self.features_for_chunk(chunk).iter() {
            if feature.apply(pos, tile) {
                tile.biome_map = Some(Arc::clone(&self.biome_map));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct BorderFeature;

    impl TerrainFeature for BorderFeature {
        fn center(&self) -> (f64, f64) {
            (15.0, 8.0)
        }

        fn influence_radius(&self) -> f64 {
            4.0
        }

        fn apply(&self, pos: TilePos, tile: &mut TileDraft) -> bool {
            let distance = (pos.x as f64 - 15.0).hypot(pos.y as f64 - 8.0);
            tile.height += (self.influence_radius() - distance).max(0.0);
            distance < self.influence_radius()
        }
    }

    struct CountingGenerator {
        queries: Arc<AtomicUsize>,
        biome_map: crate::BiomeMapDefinition,
    }

    impl FeatureGenerator for CountingGenerator {
        type Feature = BorderFeature;

        fn biome_map(&self) -> crate::BiomeMapDefinition {
            self.biome_map.clone()
        }

        fn cell_size(&self) -> i64 {
            16
        }

        fn max_influence_radius(&self) -> f64 {
            self.queries.fetch_add(1, Ordering::Relaxed);
            4.0
        }

        fn generate_cell(
            &self,
            cell_x: i64,
            cell_y: i64,
            _rng: &mut impl rand::Rng,
        ) -> Vec<Self::Feature> {
            if cell_x == 0 && cell_y == 0 {
                vec![BorderFeature]
            } else {
                Vec::new()
            }
        }
    }

    #[test]
    fn chunk_queries_are_reused_and_include_features_beyond_the_first_tile() {
        let queries = Arc::new(AtomicUsize::new(0));
        let stage = FeatureStage::new(
            42,
            CountingGenerator {
                queries: Arc::clone(&queries),
                biome_map: Default::default(),
            },
            16,
            &BiomeRepository::parse(),
        );
        let sample = |x| {
            let mut tile = TileDraft::default();
            stage.apply(TilePos { x, y: 8 }, &mut tile);
            tile.height
        };

        // The first tile is far from the feature, but its chunk intersects it.
        assert_eq!(sample(0), 0.0);
        assert_eq!(sample(15), 4.0);
        assert_eq!(queries.load(Ordering::Relaxed), 1);

        // A feature owned by the neighboring cell also reaches this chunk.
        assert_eq!(sample(16), 3.0);
        assert_eq!(sample(17), 2.0);
        assert_eq!(queries.load(Ordering::Relaxed), 2);

        // Negative coordinates use Euclidean division and empty lists are cached.
        assert_eq!(sample(-1), 0.0);
        assert_eq!(sample(-16), 0.0);
        assert_eq!(queries.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn maps_apply_only_inside_features_and_last_map_gaps_use_default() {
        let biomes = BiomeRepository::parse();
        let default_map = Arc::new(BiomeMap::parse(&biomes));
        let make_stage = |source: &str| {
            FeatureStage::new(
                42,
                CountingGenerator {
                    queries: Arc::new(AtomicUsize::new(0)),
                    biome_map: crate::BiomeMapDefinition::from_ron(source).unwrap(),
                },
                16,
                &biomes,
            )
        };
        let first = make_stage(
            r#"[(biome: "volcano", height: (min: 0.0, max: 1.0), temperature: (min: -50.0, max: 40.0))]"#,
        );
        let last = make_stage(
            r#"[(biome: "crater", height: (min: 0.0, max: 1.0), temperature: (min: 20.0, max: 30.0))]"#,
        );
        let pos = TilePos { x: 15, y: 8 };
        let mut tile = TileDraft::default();
        first.apply(pos, &mut tile);
        assert!(
            tile.biome.is_none(),
            "features must not select final biomes"
        );
        assert!(Arc::ptr_eq(
            tile.biome_map.as_ref().unwrap(),
            &first.biome_map
        ));
        last.apply(TilePos { x: 0, y: 8 }, &mut tile);
        assert!(
            Arc::ptr_eq(tile.biome_map.as_ref().unwrap(), &first.biome_map),
            "unaffected tiles retain the earlier map"
        );
        last.apply(pos, &mut tile);
        assert_eq!(
            tile.height, 8.0,
            "territory is marked even when the visible height is already clipped"
        );
        assert!(Arc::ptr_eq(
            tile.biome_map.as_ref().unwrap(),
            &last.biome_map
        ));
        tile.height = 0.5;
        tile.temperature = 0.0;
        crate::BiomeStage::new(Arc::clone(&default_map)).apply(pos, &mut tile);
        assert_eq!(tile.biome, default_map.determine_biome(0.0, 0.5));
        assert_ne!(tile.biome, biomes.id("volcano"));
    }
}
