//! Declarative world configuration.
//!
//! Defaults live in `worldgen/config/` (one file per concern, see [`ConfigFile`]).
//! A consumer may override any file by putting a file with the same relative path
//! into its own folder and loading it with [`ConfigSource::with_overrides`].

mod dimensions;
mod error;
mod schema;
mod source;

pub use dimensions::WorldGeneratorConfig;
pub use error::ConfigError;
pub(crate) use schema::parse_ron;
pub use schema::{
    BiomeDefaults, BiomeDefinition, BiomesFile, ClimateSettings, ObjectSpawn, ObjectsFile,
    WorldConfig, WorldSettings,
};
pub use source::{ConfigFile, ConfigSource};

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;
    use crate::ChunkPos;

    fn small(mut config: WorldConfig) -> WorldConfig {
        config.world.world_size_chunks = 8;
        config.world.chunk_size = 8;
        config
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("worldgen-config-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("features")).unwrap();
        dir
    }

    #[test]
    fn bundled_config_builds_a_generator() {
        let generator = small(WorldConfig::bundled()).build().unwrap();
        assert!(generator.chunk_at(ChunkPos { x: 1, y: 1 }).is_ok());
        assert_eq!(
            generator.manifest().biomes.len(),
            WorldConfig::bundled().biomes.biomes.len()
        );
    }

    #[test]
    fn folder_overrides_only_the_files_it_contains() {
        let dir = temp_dir("override");
        // Crater spawns in the bundled features/craters.ron need rock and large_rock.
        let objects = r#"(kinds: [(name: "rock", footprint: (width: 1, height: 1)), (name: "large_rock", footprint: (width: 2, height: 2)), (name: "tree", footprint: (width: 1, height: 1))], sets: {"common_rocks": [(object: "rock", density: 0.5)], "iron_deposits": []})"#;
        let volcanoes = ConfigFile::Volcanoes
            .bundled()
            .replace("probability: 0.2", "probability: 0.0");
        fs::write(dir.join("objects.ron"), objects).unwrap();
        fs::write(dir.join("features/volcanoes.ron"), &volcanoes).unwrap();

        let source = ConfigSource::with_overrides(&dir).unwrap();
        assert_eq!(
            source.overridden(),
            [ConfigFile::Objects, ConfigFile::Volcanoes]
        );
        assert_eq!(source.text(ConfigFile::Objects), objects);
        assert_eq!(
            source.text(ConfigFile::Biomes),
            ConfigFile::Biomes.bundled()
        );
        let config = WorldConfig::load(&source).unwrap();
        assert_eq!(config.volcanoes.probability, 0.0);
        let generator = small(config.clone()).build().unwrap();
        assert_eq!(generator.manifest().objects.len(), 3);

        // A kind a feature still spawns cannot be removed; the error names the feature file.
        let mut missing = config;
        missing
            .objects
            .kinds
            .retain(|kind| kind.name != "large_rock");
        assert!(matches!(
            small(missing).build(),
            Err(ConfigError::Invalid {
                file: ConfigFile::Craters,
                ..
            })
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_files_name_the_file_and_missing_folders_are_reported() {
        let mut source = ConfigSource::bundled();
        source.set(ConfigFile::BiomeMap, "not ron");
        let Err(error) = WorldConfig::load(&source) else {
            panic!("invalid biome map must fail");
        };
        assert!(
            matches!(
                error,
                ConfigError::Invalid {
                    file: ConfigFile::BiomeMap,
                    ..
                }
            ),
            "{error}"
        );

        let mut unknown = WorldConfig::bundled();
        unknown.biomes.biomes[0].object_sets = Some(vec!["missing".into()]);
        assert!(matches!(
            unknown.build(),
            Err(ConfigError::Invalid {
                file: ConfigFile::Biomes,
                ..
            })
        ));

        let mut craters = WorldConfig::bundled();
        craters.craters.min_radius = -1.0;
        assert!(matches!(
            craters.build(),
            Err(ConfigError::Invalid {
                file: ConfigFile::Craters,
                ..
            })
        ));

        assert!(matches!(
            ConfigSource::with_overrides(temp_dir("missing").join("nope")),
            Err(ConfigError::MissingDir(_))
        ));
    }

    #[test]
    fn optional_fields_may_omit_some() {
        let spawn: ObjectSpawn =
            parse_ron("(object: \"rock\", density: 0.1, max_height_difference: 0.2)").unwrap();
        assert_eq!(spawn.max_height_difference, Some(0.2));
        let explicit: ObjectSpawn =
            parse_ron("(object: \"rock\", density: 0.1, max_height_difference: Some(0.2))")
                .unwrap();
        assert_eq!(explicit, spawn);
    }
}
