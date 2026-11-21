//! The sprite sheet and its named regions (`assets/world/atlas.ron`): ground tiles and
//! object art with variants. Facts about the art only; style lives in `tiles.ron` and
//! `objects.ron`.

use std::collections::HashMap;

use bevy::{
    asset::{LoadState, RenderAssetUsages},
    image::{ImageLoaderSettings, ImageSampler},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use serde::Deserialize;

/// `(x, y, width, height)` in sheet pixels, y down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct PixelRect(pub u32, pub u32, pub u32, pub u32);

impl PixelRect {
    pub fn size(self) -> UVec2 {
        UVec2::new(self.2, self.3)
    }

    pub fn to_rect(self) -> Rect {
        Rect::new(
            self.0 as f32,
            self.1 as f32,
            (self.0 + self.2) as f32,
            (self.1 + self.3) as f32,
        )
    }

    fn fits(self, sheet: UVec2) -> bool {
        self.2 > 0 && self.3 > 0 && self.0 + self.2 <= sheet.x && self.1 + self.3 <= sheet.y
    }
}

/// One variant of an object's art.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpriteVariant {
    pub rect: PixelRect,
    /// Foot point in pixels inside `rect`, y down; defaults to the bottom center.
    #[serde(default)]
    pivot: Option<(f32, f32)>,
}

impl SpriteVariant {
    pub fn pivot(&self) -> Vec2 {
        self.pivot.map_or(
            Vec2::new(self.rect.2 as f32 / 2.0, self.rect.3 as f32),
            |(x, y)| Vec2::new(x, y),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Atlas {
    /// Asset path of the sheet.
    pub image: String,
    #[serde(default)]
    tiles: HashMap<String, PixelRect>,
    #[serde(default)]
    sprites: HashMap<String, Vec<SpriteVariant>>,
}

impl Atlas {
    pub fn tile(&self, name: &str) -> Option<PixelRect> {
        self.tiles.get(name).copied()
    }

    /// Variants of a sprite; `None` when it is missing or has none.
    pub fn sprite(&self, name: &str) -> Option<&[SpriteVariant]> {
        self.sprites
            .get(name)
            .map(Vec::as_slice)
            .filter(|variants| !variants.is_empty())
    }

    /// Every sprite variant's rect, once.
    pub fn sprite_rects(&self) -> Vec<PixelRect> {
        let mut rects: Vec<PixelRect> = self
            .sprites
            .values()
            .flatten()
            .map(|variant| variant.rect)
            .collect();
        rects.sort_by_key(|rect| (rect.0, rect.1, rect.2, rect.3));
        rects.dedup();
        rects
    }
}

/// Loads the sheet with nearest sampling, so pixel art stays crisp.
pub fn load_sheet(asset_server: &AssetServer, atlas: &Atlas) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest();
        })
        .load(atlas.image.clone())
}

/// The sheet finished loading, successfully or not.
pub fn sheet_settled(sheet: &Handle<Image>, asset_server: &AssetServer) -> bool {
    matches!(
        asset_server.get_load_state(sheet),
        Some(LoadState::Loaded | LoadState::Failed(_))
    )
}

/// The sheet as RGBA8 pixels, for cropping many regions after one conversion.
pub struct SheetPixels {
    size: UVec2,
    data: Vec<u8>,
}

impl SheetPixels {
    pub fn new(sheet: &Image) -> Option<Self> {
        let converted = sheet.convert(TextureFormat::Rgba8UnormSrgb)?;
        Some(Self {
            size: converted.size(),
            data: converted.data?,
        })
    }

    /// The pixels of `rect` as their own image; `None` when it reaches outside the sheet.
    pub fn crop(&self, rect: PixelRect) -> Option<Image> {
        if !rect.fits(self.size) {
            warn!("atlas region {rect:?} lies outside the {} sheet", self.size);
            return None;
        }
        let row = (rect.2 * 4) as usize;
        let mut data = Vec::with_capacity(row * rect.3 as usize);
        for y in rect.1..rect.1 + rect.3 {
            let start = ((y * self.size.x + rect.0) * 4) as usize;
            data.extend_from_slice(&self.data[start..start + row]);
        }
        Some(Image::new(
            Extent3d {
                width: rect.2,
                height: rect.3,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::all(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLED_ATLAS: &str = include_str!("../../../assets/world/atlas.ron");

    fn sheet() -> SheetPixels {
        // 4×2 sheet; each pixel's red channel is its index.
        SheetPixels {
            size: UVec2::new(4, 2),
            data: (0..8u8).flat_map(|i| [i, 0, 0, 255]).collect(),
        }
    }

    #[test]
    fn bundled_atlas_parses_with_variants_and_default_pivots() {
        let atlas: Atlas = ron::from_str(BUNDLED_ATLAS).unwrap();
        assert_eq!(atlas.tile("basalt_0"), Some(PixelRect(32, 96, 32, 32)));
        let sizes: Vec<_> = atlas.tiles.values().map(|rect| rect.size()).collect();
        assert!(sizes.windows(2).all(|w| w[0] == w[1]), "tiles share a size");
        let rocks = atlas.sprite("regolith_rock").unwrap();
        assert!(rocks.len() > 1);
        assert_eq!(rocks[0].pivot(), Vec2::new(7.5, 16.0));
        assert!(atlas.sprite("nothing").is_none());
    }

    #[test]
    fn crops_copy_rows_and_reject_regions_outside_the_sheet() {
        let image = sheet().crop(PixelRect(1, 0, 2, 2)).unwrap();
        assert_eq!(image.size(), UVec2::new(2, 2));
        let red: Vec<u8> = image.data.unwrap().chunks(4).map(|px| px[0]).collect();
        assert_eq!(red, [1, 2, 5, 6]);
        assert!(sheet().crop(PixelRect(3, 0, 2, 1)).is_none());
        assert!(sheet().crop(PixelRect(0, 0, 0, 1)).is_none());
    }
}
