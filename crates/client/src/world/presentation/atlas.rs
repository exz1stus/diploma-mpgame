//! The sprite sheet and its named regions (`assets/world/atlas.ron`): ground tiles and
//! object art with variants. Facts about the art only; style lives in `tiles.ron` and
//! `objects.ron`.

use std::collections::HashMap;

use bevy::{
    asset::{LoadState, RenderAssetUsages},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use serde::Deserialize;

use crate::assets::load_pixel_art;

/// `(x, y, width, height)` in sheet pixels, y down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
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
    /// Art at `rect` standing on the bottom center of its region.
    pub fn bottom_centered(rect: PixelRect) -> Self {
        Self { rect, pivot: None }
    }

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
    /// Asset path of the main sheet.
    pub image: String,
    #[serde(default)]
    tiles: HashMap<String, PixelRect>,
    #[serde(default)]
    sprites: HashMap<String, Vec<SpriteVariant>>,
    /// Further sheets with sprites of their own, drawn in their own colors (never recolored).
    #[serde(default)]
    pub sheets: Vec<ExtraSheet>,
}

/// A further sheet of [`Atlas::sheets`].
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraSheet {
    /// Asset path of the sheet.
    pub image: String,
    #[serde(default)]
    sprites: HashMap<String, Vec<SpriteVariant>>,
}

/// The sheet a sprite's art is cut from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetRef {
    Main,
    /// Index into [`Atlas::sheets`].
    Extra(usize),
}

/// A named sprite's variants and the sheet they are on.
#[derive(Clone, Copy, Debug)]
pub struct AtlasSprite<'a> {
    pub sheet: SheetRef,
    pub variants: &'a [SpriteVariant],
}

impl Atlas {
    pub fn tile(&self, name: &str) -> Option<PixelRect> {
        self.tiles.get(name).copied()
    }

    /// A sprite from the main sheet, else from the first further sheet listing it; `None` when
    /// it is missing or has no variants.
    pub fn sprite(&self, name: &str) -> Option<AtlasSprite<'_>> {
        let main = std::iter::once((SheetRef::Main, &self.sprites));
        let extra = self
            .sheets
            .iter()
            .enumerate()
            .map(|(index, extra)| (SheetRef::Extra(index), &extra.sprites));
        main.chain(extra).find_map(|(sheet, sprites)| {
            variants_of(sprites, name).map(|variants| AtlasSprite { sheet, variants })
        })
    }

    /// Every sprite variant's rect on the main sheet, once.
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

fn variants_of<'a>(
    sprites: &'a HashMap<String, Vec<SpriteVariant>>,
    name: &str,
) -> Option<&'a [SpriteVariant]> {
    sprites
        .get(name)
        .map(Vec::as_slice)
        .filter(|variants| !variants.is_empty())
}

/// Loads the main sheet with nearest sampling, so pixel art stays crisp.
pub fn load_sheet(asset_server: &AssetServer, atlas: &Atlas) -> Handle<Image> {
    load_pixel_art(asset_server, &atlas.image)
}

/// Loads the further sheets, in [`Atlas::sheets`] order.
pub fn load_extra_sheets(asset_server: &AssetServer, atlas: &Atlas) -> Vec<Handle<Image>> {
    atlas
        .sheets
        .iter()
        .map(|extra| load_pixel_art(asset_server, &extra.image))
        .collect()
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
        assert_eq!(rocks.sheet, SheetRef::Main);
        assert!(rocks.variants.len() > 1);
        assert_eq!(rocks.variants[0].pivot(), Vec2::new(7.5, 16.0));
        assert!(atlas.sprite("nothing").is_none());
    }

    #[test]
    fn further_sheets_hold_their_own_sprites_outside_the_main_rects() {
        let atlas: Atlas = ron::from_str(BUNDLED_ATLAS).unwrap();
        let hull = atlas.sprite("ship_hull").unwrap();
        let SheetRef::Extra(index) = hull.sheet else {
            panic!("the ship is on its own sheet");
        };
        assert_eq!(atlas.sheets[index].image, "textures/ship_atlas.png");
        assert!(
            !atlas.sprite_rects().contains(&hull.variants[0].rect),
            "main-sheet recoloring leaves further sheets alone"
        );
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
