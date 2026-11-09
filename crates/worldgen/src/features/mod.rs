pub(crate) mod basin;
pub(crate) mod crater;
pub(crate) mod feature;
mod stage;
pub(crate) mod volcano;

pub use feature::{FeatureGenerator, TerrainFeature};
pub use stage::FeatureStage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BiomeMap, BiomeRepository, BiomeStage, ClampHeightStage, GenerationStage, NoiseSource,
        PlanetParams, TemperatureCoefficients, TemperatureModel, TemperatureStage, TileDraft,
        TilePos, WorldGeography,
    };
    use rand::SeedableRng;
    use std::sync::Arc;

    struct FlatNoise;
    impl NoiseSource for FlatNoise {
        fn sample(&self, _: f64, _: f64) -> f64 {
            0.0
        }
    }

    fn check_feature<G: FeatureGenerator>(generator: G, expected_biome: &str) {
        let repository = BiomeRepository::parse();
        let map = Arc::new(generator.biome_map().resolve(&repository).unwrap());
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let features = generator.generate_cell(0, 0, &mut rng);
        let feature = features
            .first()
            .expect("forced probability generates a feature");
        let (x, y) = feature.center();
        let pos = TilePos {
            x: x.round() as i64,
            y: y.round() as i64,
        };
        let mut tile = TileDraft {
            height: 0.5,
            ..Default::default()
        };
        assert!(feature.apply(pos, &mut tile));
        assert!(tile.biome.is_none());
        assert!(tile.temperature_offset > 19.0);
        tile.biome_map = Some(map);
        ClampHeightStage.apply(pos, &mut tile);
        TemperatureStage::new(TemperatureModel::with_config(
            TemperatureCoefficients {
                base: 0.0,
                region: 0.0,
                solar: 0.0,
                altitude: 10.0,
            },
            FlatNoise,
            WorldGeography::new(16, 64),
            PlanetParams::default(),
        ))
        .apply(pos, &mut tile);
        assert_eq!(
            tile.temperature,
            tile.temperature_offset - 10.0 * tile.height
        );
        BiomeStage::new(Arc::new(BiomeMap::parse(&repository))).apply(pos, &mut tile);
        assert_eq!(tile.finish().biome, repository.id(expected_biome).unwrap());
        let mut outside = TileDraft {
            height: 0.5,
            ..Default::default()
        };
        assert!(!feature.apply(
            TilePos {
                x: (x + feature.influence_radius() + 2.0).ceil() as i64,
                y: y.round() as i64
            },
            &mut outside
        ));
        assert_eq!(outside.height, 0.5);
        assert_eq!(outside.temperature_offset, 0.0);
    }

    #[test]
    fn each_feature_loads_ron_maps_and_preserves_heating_until_biome_selection() {
        let heated =
            |source: &str| source.replace("temperature_offset: 0.0", "temperature_offset: 20.0");
        let mut crater =
            crater::CraterGenerator::from_ron(&heated(include_str!("crater.ron"))).unwrap();
        crater.density = 4.0;
        check_feature(crater, "crater");
        let mut basin =
            basin::BasinGenerator::from_ron(&heated(include_str!("basin.ron")), FlatNoise).unwrap();
        basin.probability = 1.0;
        check_feature(basin, "weathered_basin");
        let mut volcano =
            volcano::VolcanoGenerator::from_ron(&heated(include_str!("volcano.ron"))).unwrap();
        volcano.probability = 1.0;
        check_feature(volcano, "volcano");
    }

    #[test]
    fn bundled_feature_maps_cover_climate_bands() {
        let repository = BiomeRepository::parse();
        for definition in [
            crater::CraterGenerator::default().biome_map,
            basin::BasinGenerator::new(FlatNoise).biome_map,
            volcano::VolcanoGenerator::default().biome_map,
        ] {
            let map = definition.resolve(&repository).unwrap();
            for temperature in [
                -50.0, -35.0, -30.0, -20.0, -12.0, -5.0, -2.0, 0.0, 2.0, 5.0, 8.0, 14.0, 20.0,
                39.999,
            ] {
                for height in [0.0, 0.32, 0.4, 0.44, 0.5, 0.52, 0.65, 0.68, 0.78, 1.0] {
                    let id = map
                        .determine_biome(temperature, height)
                        .expect("feature map must cover the default climate domain");
                    if temperature < -20.0 {
                        assert!(repository.get(id).unwrap().name.starts_with("polar_"));
                    }
                }
            }
        }
    }
}
