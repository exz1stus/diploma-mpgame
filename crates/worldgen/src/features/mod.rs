pub(crate) mod basin;
pub(crate) mod crater;
pub(crate) mod feature;
mod stage;
pub(crate) mod volcano;

pub use feature::{FeatureGenerator, ScaledFeatures, TerrainFeature};
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
        BiomeStage, ClampHeightStage, GenerationStage, NoiseSource, PlanetParams,
        TemperatureCoefficients, TemperatureModel, TemperatureStage, TileDraft, TilePos,
        WorldGeography,
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
        let repository = crate::old_preset::biomes();
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
        let terrain = crate::old_preset::map(&repository);
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
        let config = crate::old_preset::config();
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
    fn default_maps_cover_the_climate_domain_with_the_preset_kinds() {
        let repository = crate::BiomeRepository::parse();
        let name = |id| repository.get(id).unwrap().name.clone();
        let terrain = crate::BiomeMap::parse(&repository);
        assert!(
            crater::CraterGenerator::default().biome_map.0.is_empty(),
            "craters only shape height and keep the terrain or basin biome map"
        );
        let settings = basin::BasinSettings::default();
        let variants: Vec<_> = settings
            .variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect();
        assert_eq!(variants, ["basalt", "sulfur"]);
        assert!(settings.shape_noise > 0.0, "basins have irregular outlines");
        let basins: Vec<_> = settings
            .biome_maps()
            .iter()
            .map(|map| map.resolve(&repository).unwrap())
            .collect();
        let volcanoes = volcano::VolcanoGenerator::default()
            .biome_map
            .resolve(&repository)
            .unwrap();
        for temperature in [
            -60.0, -35.0, -16.0, -12.0, -6.0, 0.0, 5.999, 6.0, 13.0, 14.0, 59.999,
        ] {
            for height in [0.0, 0.37, 0.38, 0.45, 0.5, 0.6, 0.62, 0.9, 1.0] {
                let ground = name(
                    terrain
                        .determine_biome(temperature, height)
                        .expect("terrain covers"),
                );
                let kind = match temperature {
                    t if t < -16.0 => "snow_",
                    t if t < -6.0 => "frost_",
                    t if t < 6.0 => "plain_",
                    _ => "desert_",
                };
                assert!(ground.starts_with(kind), "{ground} at {temperature} °C");
                assert!(
                    name(volcanoes.determine_biome(temperature, height).unwrap())
                        .starts_with("basalt_")
                );
                // Sulfur and basalt appear only in their basin types.
                for (map, kind) in basins.iter().zip(["basalt_", "sulfur_"]) {
                    match map.determine_biome(temperature, height) {
                        Some(id) => {
                            assert!(temperature >= -6.0);
                            assert!(name(id).starts_with(kind));
                        }
                        None => assert!(
                            temperature < -6.0,
                            "frozen basins fall back to the terrain map"
                        ),
                    }
                }
            }
        }
        // Lower ground gets the lower variant in every kind.
        assert_eq!(
            name(terrain.determine_biome(0.0, 0.1).unwrap()),
            "plain_floor"
        );
        assert_eq!(
            name(terrain.determine_biome(0.0, 0.7).unwrap()),
            "plain_ridge"
        );
    }
}
