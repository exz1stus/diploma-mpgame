use bevy::{
    app::{App, FixedPreUpdate},
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::resource::Resource;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{config::AppConfig, net::server::TickSet};

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

/// Server overrides of the worldgen defaults (`worldgen/config/`), file by file.
const CONFIG_OVERRIDES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/worldgen");

pub(crate) fn install_world(app: &mut App, world_size_chunks: Option<u32>) {
    let config = app.world().get_resource::<AppConfig>();
    let overrides = config.map_or_else(
        || PathBuf::from(CONFIG_OVERRIDES_DIR),
        |config| config.worldgen_config.clone(),
    );
    let seed = config.and_then(|config| config.world_seed);
    let generator = Arc::new(load_generator(&overrides, seed, world_size_chunks));
    install(app, generator, RECENT_CHUNKS);
    objects::default_kinds(app);
    app.configure_sets(
        FixedPreUpdate,
        streaming::ChunkStreamingSet.after(TickSet::DispatchMessages),
    );
}

/// Builds the generator from the worldgen defaults plus the files in `overrides`,
/// optionally with another seed. Panics with a readable message: a server cannot run
/// without a valid world.
fn load_generator(
    overrides: &Path,
    seed: Option<u64>,
    world_size_chunks: Option<u32>,
) -> worldgen::WorldGenerator {
    worldgen::ConfigSource::with_overrides(overrides)
        .and_then(|source| {
            log_overrides(overrides, &source);
            let mut config = worldgen::WorldConfig::load(&source)?;
            if let Some(seed) = seed {
                config.world.seed = seed;
            }
            if let Some(size) = world_size_chunks {
                resize_world(&mut config.world, size);
            }
            config.build()
        })
        .unwrap_or_else(|error| panic!("world config {}: {error}", overrides.display()))
}

/// Changes the map size and scales the terrain with it, so the configured world is
/// magnified rather than extended: same layout, climate bands and feature count.
fn resize_world(world: &mut worldgen::WorldSettings, size_chunks: u32) {
    world.terrain_scale *= f64::from(size_chunks) / f64::from(world.world_size_chunks);
    world.world_size_chunks = size_chunks;
}

fn log_overrides(dir: &Path, source: &worldgen::ConfigSource) {
    let files: Vec<_> = source.overridden().iter().map(|file| file.path()).collect();
    let files = if files.is_empty() {
        "none".to_owned()
    } else {
        files.join(", ")
    };
    println!("World config overrides from {}: {files}", dir.display());
}

/// Generator, storage and streaming systems; separate from [`install_world`] so tests can use a small world.
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
    fn server_overrides_build_a_generator() {
        let generator = load_generator(Path::new(CONFIG_OVERRIDES_DIR), None, None);
        let manifest = generator.manifest();
        for name in ["rock", "large_rock", "iron_ore"] {
            assert!(
                manifest.objects.iter().any(|kind| kind.name == name),
                "default object kinds rely on {name:?}"
            );
        }
        let reseeded = load_generator(Path::new(CONFIG_OVERRIDES_DIR), Some(12345), Some(16));
        assert_eq!(reseeded.config().seed, 12345);
        assert_eq!(reseeded.config().world_size_chunks, 16);
    }

    #[test]
    fn resizing_scales_the_terrain_with_the_map() {
        let mut world = worldgen::WorldConfig::bundled().world;
        world.world_size_chunks = 250;
        world.terrain_scale = 0.5;
        resize_world(&mut world, 512);
        assert_eq!(world.world_size_chunks, 512);
        assert!((world.terrain_scale - 1.024).abs() < 1e-12);
    }
}
