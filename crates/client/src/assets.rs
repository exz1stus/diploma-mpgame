//! Where the client's asset files live and how its config tables are read.

use std::{
    fs,
    path::{Path, PathBuf},
};

use bevy::{
    asset::io::file::FileAssetReader,
    image::{ImageLoaderSettings, ImageSampler},
    prelude::*,
};
use serde::de::DeserializeOwned;

/// Filesystem root of the client's assets, used to skip loading files that do not exist.
#[derive(Resource, Clone, Debug, Deref)]
pub struct AssetRoot(pub PathBuf);

impl Default for AssetRoot {
    fn default() -> Self {
        Self(FileAssetReader::get_base_path().join("assets"))
    }
}

/// Asset path of a texture: `textures/<folder>/<name>.png`.
pub fn texture_path(folder: &str, name: &str) -> String {
    format!("textures/{folder}/{name}.png")
}

pub fn texture_file_exists(root: &Path, folder: &str, name: &str) -> bool {
    root.join(texture_path(folder, name)).is_file()
}

/// Reads a RON table, falling back to the copy bundled at build time when the file is
/// missing or invalid.
pub fn read_table<T: DeserializeOwned>(path: &Path, bundled: &str) -> T {
    let text = fs::read_to_string(path).unwrap_or_else(|_| {
        warn!("{} not found; using the bundled copy", path.display());
        bundled.to_owned()
    });
    ron::from_str(&text).unwrap_or_else(|error| {
        warn!(
            "invalid {}: {error}; using the bundled copy",
            path.display()
        );
        ron::from_str(bundled).expect("bundled config must be valid")
    })
}

/// Loads an image with nearest sampling, so pixel art stays crisp.
pub fn load_pixel_art(asset_server: &AssetServer, path: &str) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest();
        })
        .load(path.to_owned())
}
