pub(crate) mod basin;
pub(crate) mod crater;
pub(crate) mod feature;
mod stage;
pub(crate) mod volcano;

pub use feature::{FeatureGenerator, TerrainFeature};
pub use stage::FeatureStage;

use serde::{Deserialize, Serialize};

/// The terrain feature types; object spawns can be limited to one of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureKind {
    Basin,
    Crater,
    Volcano,
}

impl FeatureKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Basin => "basin",
            Self::Crater => "crater",
            Self::Volcano => "volcano",
        }
    }
}

/// The feature whose territory contains a tile (the last one applied on overlap).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeatureMark {
    pub kind: FeatureKind,
    /// Distance from the feature centre divided by its radius: 0 at the centre,
    /// 1 at the rim (crater, basin) or foot (volcano).
    pub distance: f64,
}

/// The message of the first failed `(passed, message)` check.
pub(crate) fn first_failure(checks: &[(bool, &str)]) -> Result<(), String> {
    match checks.iter().find(|(passed, _)| !passed) {
        Some((_, message)) => Err((*message).to_owned()),
        None => Ok(()),
    }
}

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
        assert!(feature.apply(pos, &mut tile).is_some());
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
        let terrain = BiomeMap::parse(&repository);
        let ground = terrain.determine_biome(tile.temperature, tile.height);
        BiomeStage::new(Arc::new(terrain)).apply(pos, &mut tile);
        let finished = tile.finish();
        assert_eq!(finished.biome, repository.id(expected_biome).unwrap());
        assert_eq!(
            Some(finished.ground),
            ground,
            "the ground stays the terrain biome"
        );
        let mut outside = TileDraft {
            height: 0.5,
            ..Default::default()
        };
        assert!(
            feature
                .apply(
                    TilePos {
                        x: (x + feature.influence_radius() + 2.0).ceil() as i64,
                        y: y.round() as i64
                    },
                    &mut outside
                )
                .is_none()
        );
        assert_eq!(outside.height, 0.5);
        assert_eq!(outside.temperature_offset, 0.0);
    }

    #[test]
    fn each_feature_uses_its_configured_map_and_preserves_heating_until_biome_selection() {
        let config = crate::WorldConfig::bundled();
        let mut crater = config.craters;
        crater.density = 4.0;
        crater.temperature_offset = 20.0;
        check_feature(crater, "crater");
        let mut basin = config.basins;
        basin.probability = 1.0;
        basin.temperature_offset = 20.0;
        check_feature(
            basin::BasinGenerator::new(basin, FlatNoise),
            "weathered_basin",
        );
        let mut volcano = config.volcanoes;
        volcano.probability = 1.0;
        volcano.temperature_offset = 20.0;
        check_feature(volcano, "volcano");
    }

    #[test]
    fn bundled_feature_maps_cover_climate_bands() {
        let repository = BiomeRepository::parse();
        for definition in [
            crater::CraterGenerator::default().biome_map,
            basin::BasinSettings::default().biome_map,
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
