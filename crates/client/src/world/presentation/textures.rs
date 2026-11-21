//! Texture files for catalog entries: `assets/textures/<folder>/<name>.png`.
//! Entries without a file are drawn with their placeholder color.

use std::path::Path;

use bevy::{
    asset::{LoadState, RenderAssetUsages},
    image::ImageSampler,
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
};

use super::catalog::TextureEntry;
use crate::assets::{texture_file_exists, texture_path};

pub const TILE_FOLDER: &str = "tiles";
pub const OBJECT_FOLDER: &str = "objects";

/// One handle per catalog entry; `None` where no texture file exists.
pub fn request_textures(
    root: &Path,
    folder: &str,
    entries: &[TextureEntry],
    asset_server: &AssetServer,
) -> Vec<Option<Handle<Image>>> {
    entries
        .iter()
        .map(|entry| {
            texture_file_exists(root, folder, &entry.name)
                .then(|| asset_server.load(texture_path(folder, &entry.name)))
        })
        .collect()
}

/// Every requested texture finished loading, successfully or not.
pub fn all_settled(handles: &[Option<Handle<Image>>], asset_server: &AssetServer) -> bool {
    handles.iter().flatten().all(|handle| {
        matches!(
            asset_server.get_load_state(handle),
            Some(LoadState::Loaded | LoadState::Failed(_))
        )
    })
}

/// Stacks one layer per entry into a 2D array texture. All layers take the size of the
/// first texture; textures of another size, or missing ones, become placeholder layers.
pub fn stack_layers(entries: &[TextureEntry], textures: &[Option<&Image>]) -> Image {
    let rgba: Vec<_> = textures
        .iter()
        .map(|texture| texture.and_then(to_rgba8))
        .collect();
    let size = rgba
        .iter()
        .flatten()
        .next()
        .map_or(UVec2::ONE, |(size, _)| *size);
    let layer_bytes = (size.x * size.y * 4) as usize;
    let mut data = Vec::with_capacity(layer_bytes * entries.len());
    for (entry, texture) in entries.iter().zip(&rgba) {
        match texture {
            Some((texture_size, pixels)) if *texture_size == size => data.extend(pixels),
            Some((texture_size, _)) => {
                warn!(
                    "tile texture {:?} is {texture_size}, expected {size}; using its placeholder",
                    entry.name
                );
                data.extend(solid_layer(entry.placeholder, layer_bytes));
            }
            None => data.extend(solid_layer(entry.placeholder, layer_bytes)),
        }
    }
    array_image(size, entries.len().max(1) as u32, data)
}

fn to_rgba8(image: &Image) -> Option<(UVec2, Vec<u8>)> {
    let converted = image.convert(TextureFormat::Rgba8UnormSrgb)?;
    Some((converted.size(), converted.data?))
}

fn solid_layer(color: Srgba, bytes: usize) -> impl Iterator<Item = u8> {
    color.to_u8_array().into_iter().cycle().take(bytes)
}

fn array_image(size: UVec2, layers: u32, mut data: Vec<u8>) -> Image {
    // An empty catalog still needs one layer to bind.
    data.resize((size.x * size.y * 4 * layers) as usize, 255);
    let mut image = Image::new(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: layers,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // A single layer would otherwise default to a plain 2D view.
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    image.sampler = ImageSampler::nearest();
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, placeholder: Srgba) -> TextureEntry {
        TextureEntry {
            name: name.into(),
            placeholder,
        }
    }

    fn texture(size: u32, rgba: [u8; 4]) -> Image {
        Image::new_fill(
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::all(),
        )
    }

    #[test]
    fn missing_and_mismatched_textures_become_placeholder_layers() {
        let red = Srgba::rgb_u8(255, 0, 0);
        let blue = Srgba::rgb_u8(0, 0, 255);
        let entries = [entry("a", red), entry("b", blue), entry("c", red)];
        let real = texture(2, [1, 2, 3, 255]);
        let wrong_size = texture(4, [9, 9, 9, 255]);
        let image = stack_layers(&entries, &[None, Some(&real), Some(&wrong_size)]);
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 3);
        assert_eq!(image.size(), UVec2::splat(2));
        let data = image.data.unwrap();
        assert_eq!(
            &data[0..4],
            &[255, 0, 0, 255],
            "missing file uses placeholder"
        );
        assert_eq!(&data[16..20], &[1, 2, 3, 255], "real texture is copied");
        assert_eq!(
            &data[32..36],
            &[255, 0, 0, 255],
            "wrong size uses placeholder"
        );
    }

    #[test]
    fn placeholder_only_catalogs_use_one_pixel_layers() {
        let image = stack_layers(&[entry("a", Srgba::WHITE)], &[None]);
        assert_eq!(image.size(), UVec2::ONE);
        assert_eq!(image.data.unwrap(), [255, 255, 255, 255]);
        let empty = stack_layers(&[], &[]);
        assert_eq!(empty.texture_descriptor.size.depth_or_array_layers, 1);
    }
}
