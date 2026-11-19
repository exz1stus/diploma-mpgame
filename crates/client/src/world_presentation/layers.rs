//! Tile visuals are composed by an ordered list of [`TileLayer`]s. Each layer owns one
//! concern and writes only its part of [`TileVisual`], so layers can be added, replaced or
//! removed independently. The terrain shader blends each pixel from the visuals of the four
//! nearest tiles (see `terrain.wgsl`).

use bevy::prelude::*;
use shared::world::{TileSample, WorldManifest};

use super::catalog::TileCatalog;
use crate::game::world::ChunkTiles;

/// Everything the terrain shader receives per tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileVisual {
    /// Layer in the tile texture array.
    pub texture_layer: u32,
    /// Overlay layer: drawn over neighbors with a lower one, blended with neighbors on the
    /// same one.
    pub overlay: u32,
    /// sRGB color the ground is multiplied by; smoothed across tiles by the tile data.
    pub tint: [u8; 3],
}

impl Default for TileVisual {
    fn default() -> Self {
        Self {
            texture_layer: 0,
            overlay: 0,
            tint: [255; 3],
        }
    }
}

/// What a layer may look at while styling one tile.
#[allow(dead_code, reason = "API for future tile layers")]
pub struct TileContext<'a> {
    pub manifest: &'a WorldManifest,
    pub catalog: &'a TileCatalog,
    /// The whole chunk, so layers can inspect neighbors (e.g. for relief shading).
    pub chunk: &'a ChunkTiles,
    /// Chunk-local tile coordinates, +Y up.
    pub local: UVec2,
}

#[allow(dead_code, reason = "API for future tile layers")]
impl TileContext<'_> {
    /// A tile of the same chunk, if inside it.
    pub fn neighbor(&self, dx: i32, dy: i32) -> Option<TileSample> {
        let x = self.local.x.checked_add_signed(dx)?;
        let y = self.local.y.checked_add_signed(dy)?;
        let size = self.chunk.size;
        (x < size && y < size).then(|| self.chunk.tiles[(y * size + x) as usize])
    }
}

/// One step of tile styling.
pub trait TileLayer: Send + Sync + 'static {
    fn apply(&self, sample: TileSample, context: &TileContext, visual: &mut TileVisual);
}

/// Layers in application order.
#[derive(Resource, Default)]
pub struct TileLayers(Vec<Box<dyn TileLayer>>);

impl TileLayers {
    pub fn push(&mut self, layer: impl TileLayer) {
        self.0.push(Box::new(layer));
    }

    /// Visuals of every tile of a chunk, in chunk tile order.
    pub fn style_chunk(
        &self,
        chunk: &ChunkTiles,
        manifest: &WorldManifest,
        catalog: &TileCatalog,
    ) -> Vec<TileVisual> {
        chunk
            .tiles
            .iter()
            .enumerate()
            .map(|(index, &sample)| {
                let index = index as u32;
                let context = TileContext {
                    manifest,
                    catalog,
                    chunk,
                    local: UVec2::new(index % chunk.size, index / chunk.size),
                };
                self.style_tile(sample, &context)
            })
            .collect()
    }

    fn style_tile(&self, sample: TileSample, context: &TileContext) -> TileVisual {
        let mut visual = TileVisual::default();
        for layer in &self.0 {
            layer.apply(sample, context, &mut visual);
        }
        visual
    }
}

/// Registers tile layers; they run in registration order.
pub trait TileLayerAppExt {
    fn add_tile_layer(&mut self, layer: impl TileLayer) -> &mut Self;
}

impl TileLayerAppExt for App {
    fn add_tile_layer(&mut self, layer: impl TileLayer) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<TileLayers>()
            .push(layer);
        self
    }
}

/// Picks each tile's texture from its biome, or from its ground biome when the biome
/// has no tiles of its own (see [`TileCatalog`]), the texture's overlay layer, and the tint.
pub struct TextureLayer;

impl TileLayer for TextureLayer {
    fn apply(&self, sample: TileSample, context: &TileContext, visual: &mut TileVisual) {
        visual.texture_layer = context.catalog.layer(sample);
        visual.overlay = context.catalog.overlay(visual.texture_layer);
        visual.tint = context.catalog.tint(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_presentation::catalog::{TextureCatalog, TextureEntry};
    use shared::world::BiomeId;

    /// Raises tiles on the west edge and high tiles.
    struct Raise;
    impl TileLayer for Raise {
        fn apply(&self, sample: TileSample, context: &TileContext, visual: &mut TileVisual) {
            let edge = context.neighbor(-1, 0).is_none();
            visual.overlay += if edge {
                5
            } else {
                sample.height().round() as u32
            };
        }
    }

    fn chunk() -> ChunkTiles {
        ChunkTiles {
            size: 2,
            tiles: vec![
                TileSample::new(BiomeId(0), 0.0),
                TileSample::new(BiomeId(1), 1.0),
                TileSample::new(BiomeId(1), 0.0),
                TileSample::new(BiomeId(0), 1.0),
            ],
        }
    }

    fn catalog() -> TileCatalog {
        let entry = |name: &str| TextureEntry {
            name: name.into(),
            placeholder: Srgba::WHITE,
        };
        TileCatalog {
            set: TextureCatalog {
                textures: vec![entry("a"), entry("b")],
                texture_of: vec![1, 0],
            },
            own_tiles: vec![true, true],
            overlay: vec![0, 1],
            tints: vec![[255; 3], [10, 20, 30]],
        }
    }

    fn manifest() -> WorldManifest {
        WorldManifest {
            chunk_size: 2,
            world_size_chunks: 1,
            biomes: vec!["first".into(), "second".into()],
            objects: vec![],
        }
    }

    #[test]
    fn layers_run_in_order_over_every_tile() {
        let mut layers = TileLayers::default();
        layers.push(TextureLayer);
        layers.push(Raise);
        let visuals = layers.style_chunk(&chunk(), &manifest(), &catalog());
        let textures: Vec<_> = visuals.iter().map(|v| v.texture_layer).collect();
        assert_eq!(textures, [1, 0, 0, 1]);
        let overlays: Vec<_> = visuals.iter().map(|v| v.overlay).collect();
        // Texture 1 is on layer 1; x = 0 has no west neighbor.
        assert_eq!(overlays, [6, 1, 5, 2]);
        let tints: Vec<_> = visuals.iter().map(|v| v.tint).collect();
        assert_eq!(tints, [[255; 3], [10, 20, 30], [10, 20, 30], [255; 3]]);
    }

    #[test]
    fn without_layers_tiles_keep_default_visuals() {
        let visuals = TileLayers::default().style_chunk(&chunk(), &manifest(), &catalog());
        assert_eq!(visuals, vec![TileVisual::default(); 4]);
    }
}
