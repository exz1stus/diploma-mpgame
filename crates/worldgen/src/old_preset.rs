//! Test access to the `old` preset (`config/presets/old/`): the biome set many mechanics tests
//! were written against. They test map, feature and stage behavior with its biome names, which
//! the default preset no longer has.

use crate::{BiomeMap, BiomeRepository, ConfigFile, ConfigSource, WorldConfig};

fn source() -> ConfigSource {
    ConfigSource::with_overrides(concat!(env!("CARGO_MANIFEST_DIR"), "/config/presets/old"))
        .expect("the old preset folder exists")
}

pub fn config() -> WorldConfig {
    WorldConfig::load(&source()).expect("the old preset is valid")
}

pub fn biomes() -> BiomeRepository {
    BiomeRepository::from_definitions(&config().biomes.biomes).expect("old biomes are valid")
}

/// The old preset's terrain biome map.
pub fn map(repository: &BiomeRepository) -> BiomeMap {
    BiomeMap::from_ron(source().text(ConfigFile::BiomeMap), repository)
        .expect("the old terrain map is valid")
}
