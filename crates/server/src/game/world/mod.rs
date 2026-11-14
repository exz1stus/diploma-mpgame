use bevy::{
    app::{App, FixedPreUpdate},
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::resource::Resource;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use worldgen::WorldGeneratorConfig;

use crate::{config::AppConfig, networking::server::TickSet};

pub(crate) mod components;
pub(crate) mod coords;
pub(crate) mod objects;
pub(crate) mod storage;
pub(crate) mod streaming;
#[cfg(test)]
pub(crate) mod test_support;

pub(crate) use components::WorldObserver;

/// Recently unloaded chunks kept deserialized before they are archived.
const RECENT_CHUNKS: usize = 512;

/// Engine integration belongs here; the generator itself has no Bevy dependency.
#[derive(Resource)]
pub(crate) struct WorldGenerator(pub Arc<worldgen::WorldGenerator>);

impl std::ops::Deref for WorldGenerator {
    type Target = worldgen::WorldGenerator;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Bundled presets shipped with the server.
const PRESETS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/game/worldgen_presets");

pub(crate) fn plugin(app: &mut App) {
    let preset_name = app.world().get_resource::<AppConfig>().map_or_else(
        || "default".to_string(),
        |config| config.worldgen_preset.clone(),
    );
    let generator = Arc::new(load_generator(
        &preset_name,
        WorldGeneratorConfig::default(),
    ));
    install(app, generator, RECENT_CHUNKS);
    objects::default_kinds(app);
    app.configure_sets(
        FixedPreUpdate,
        streaming::ChunkStreamingSet.after(TickSet::DispatchMessages),
    );
}

/// Resolves a preset by name (folder in [`PRESETS_DIR`]) or by path, and builds the generator.
/// Panics with a readable message: a server cannot run without a valid world.
fn load_generator(preset: &str, config: WorldGeneratorConfig) -> worldgen::WorldGenerator {
    let dir = preset_dir(preset);
    worldgen::WorldPreset::from_dir(&dir)
        .and_then(|preset| {
            let overridden: Vec<_> = preset.overridden().iter().map(|f| f.file_name()).collect();
            println!(
                "World preset {}: {} from folder",
                dir.display(),
                overridden.join(", ")
            );
            preset.build(config)
        })
        .unwrap_or_else(|error| panic!("world preset {}: {error}", dir.display()))
}

fn preset_dir(preset: &str) -> PathBuf {
    let path = Path::new(preset);
    if path.components().count() > 1 || path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(PRESETS_DIR).join(preset)
    }
}

/// Generator, storage and streaming systems; separate from [`plugin`] so tests can use a small world.
pub(crate) fn install(
    app: &mut App,
    generator: Arc<worldgen::WorldGenerator>,
    recent_chunks: usize,
) {
    app.insert_resource(WorldGenerator(generator));
    app.insert_resource(storage::ChunkStorage::new(recent_chunks));
    app.add_plugins((objects::plugin, streaming::plugin));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preset_folder_builds_with_every_file_overridden() {
        let dir = preset_dir("default");
        let preset = worldgen::WorldPreset::from_dir(&dir).unwrap();
        assert_eq!(preset.overridden(), worldgen::PresetFile::ALL);
        let generator = preset
            .build(WorldGeneratorConfig {
                world_size_chunks: 8,
                chunk_size: 8,
                ..Default::default()
            })
            .unwrap();
        assert!(
            generator
                .chunk_at(worldgen::ChunkPos { x: 2, y: 2 })
                .is_ok()
        );
    }

    #[test]
    fn presets_resolve_by_name_or_path() {
        assert_eq!(
            preset_dir("default"),
            Path::new(PRESETS_DIR).join("default")
        );
        assert_eq!(
            preset_dir("./my_presets/arid"),
            Path::new("./my_presets/arid")
        );
    }
}
