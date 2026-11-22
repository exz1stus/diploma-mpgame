//! The ground texture array: one layer per tile texture, cropped from the atlas; textures
//! without a region are drawn with their placeholder color.

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
};

use super::{atlas::PixelRect, catalog::TextureEntry};

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
    for layer in data.chunks_exact_mut(layer_bytes) {
        bake_pixel_heights(layer);
    }
    array_image(size, entries.len().max(1) as u32, data)
}

/// Height of a flat texture's pixels, and the middle of the height range.
const FLAT_HEIGHT: u8 = 128;

/// Writes every pixel's height into its alpha: its brightness (r + g + b) relative to the
/// layer's mean, scaled so the layer's furthest pixel from the mean reaches 0 or 255. Every
/// texture spans the full range whatever its contrast; flat ones stay at [`FLAT_HEIGHT`].
/// The worldgen preview (`terrain.mjs`, `pixelHeights`) computes the same bytes.
pub fn bake_pixel_heights(layer: &mut [u8]) {
    let brightness =
        |px: &[u8; 4]| f64::from(u16::from(px[0]) + u16::from(px[1]) + u16::from(px[2]));
    let (pixels, _) = layer.as_chunks_mut::<4>();
    let count = pixels.len().max(1) as f64;
    let mean = pixels.iter().map(brightness).sum::<f64>() / count;
    let spread = pixels
        .iter()
        .map(|px| (brightness(px) - mean).abs())
        .fold(0.0, f64::max);
    for px in pixels {
        px[3] = if spread < 0.5 {
            FLAT_HEIGHT
        } else {
            (127.5 + 127.5 * (brightness(px) - mean) / spread)
                .round()
                .clamp(0.0, 255.0) as u8
        };
    }
}

/// Makes art neutral so tints color it: every pixel becomes `color` scaled by its red channel
/// over the mean red of the art (opaque pixels only). Keeps the art's light and shade, drops
/// its hue. The worldgen preview applies the same formula.
pub fn recolor(image: &mut Image, color: Srgba) {
    let size = image.size();
    recolor_regions(image, &[PixelRect(0, 0, size.x, size.y)], color);
}

/// [`recolor`] applied to each region of an RGBA8 image on its own, such as every sprite of
/// a sheet; regions reaching outside the image are skipped.
pub fn recolor_regions(image: &mut Image, regions: &[PixelRect], color: Srgba) {
    let width = image.width();
    let height = image.height();
    let Some(data) = image.data.as_mut() else {
        return;
    };
    let rgb = color.to_u8_array();
    for &PixelRect(x0, y0, w, h) in regions {
        if x0 + w > width || y0 + h > height {
            continue;
        }
        let pixel = |x: u32, y: u32| ((y * width + x) * 4) as usize;
        let region = || (y0..y0 + h).flat_map(move |y| (x0..x0 + w).map(move |x| (x, y)));
        let (sum, count) = region()
            .map(|(x, y)| &data[pixel(x, y)..pixel(x, y) + 4])
            .filter(|px| px[3] == 255)
            .fold((0.0, 0u32), |(sum, count), px| {
                (sum + f32::from(px[0]), count + 1)
            });
        let mean = (sum / count.max(1) as f32).max(1.0);
        for (x, y) in region() {
            let at = pixel(x, y);
            let light = f32::from(data[at]) / mean;
            for channel in 0..3 {
                data[at + channel] =
                    (f32::from(rgb[channel]) * light).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
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
        // Flat layers: alpha holds the middle height.
        assert_eq!(
            &data[0..4],
            &[255, 0, 0, 128],
            "missing file uses placeholder"
        );
        assert_eq!(&data[16..20], &[1, 2, 3, 128], "real texture is copied");
        assert_eq!(
            &data[32..36],
            &[255, 0, 0, 128],
            "wrong size uses placeholder"
        );
    }

    #[test]
    fn pixel_heights_span_the_full_range_around_the_mean() {
        // Brightness 0, 30, 30, 60: mean 30, spread 30.
        let mut layer = vec![
            0, 0, 0, 255, 10, 10, 10, 255, 30, 0, 0, 255, 20, 20, 20, 255,
        ];
        bake_pixel_heights(&mut layer);
        let heights: Vec<_> = layer.chunks(4).map(|px| px[3]).collect();
        assert_eq!(heights, [0, 128, 128, 255]);
        assert_eq!(&layer[4..7], &[10, 10, 10], "colors are kept");
        // Low contrast art still spans the range: 100 vs 103.
        let mut faint = vec![100, 0, 0, 255, 103, 0, 0, 255];
        bake_pixel_heights(&mut faint);
        assert_eq!((faint[3], faint[7]), (0, 255));
    }

    #[test]
    fn recolor_keeps_relative_light_in_the_given_color() {
        let mut image = texture(2, [100, 50, 10, 255]);
        let data = image.data.as_mut().unwrap();
        data[0] = 200; // first pixel twice as bright in red as the other three
        recolor(&mut image, Srgba::rgb_u8(160, 160, 160));
        let data = image.data.unwrap();
        // Mean red is (200 + 3 × 100) / 4 = 125.
        assert_eq!(
            &data[0..4],
            &[0xFF, 0xFF, 0xFF, 255],
            "200/125 × 160 clamps to white"
        );
        assert_eq!(&data[4..8], &[128, 128, 128, 255], "100/125 × 160");
    }

    #[test]
    fn regions_recolor_on_their_own_mean_and_leave_the_rest() {
        // 2×1 image: a dim and a bright pixel, recolored as separate regions.
        let mut image = texture(1, [0, 0, 0, 255]);
        image.texture_descriptor.size.width = 2;
        image.data = Some(vec![40, 0, 0, 255, 200, 9, 9, 255]);
        recolor_regions(
            &mut image,
            &[PixelRect(1, 0, 1, 1)],
            Srgba::rgb_u8(100, 100, 100),
        );
        assert_eq!(image.data.unwrap(), [40, 0, 0, 255, 100, 100, 100, 255]);
    }

    #[test]
    fn placeholder_only_catalogs_use_one_pixel_layers() {
        let image = stack_layers(&[entry("a", Srgba::WHITE)], &[None]);
        assert_eq!(image.size(), UVec2::ONE);
        assert_eq!(image.data.unwrap(), [255, 255, 255, 128]);
        let empty = stack_layers(&[], &[]);
        assert_eq!(empty.texture_descriptor.size.depth_or_array_layers, 1);
    }
}
