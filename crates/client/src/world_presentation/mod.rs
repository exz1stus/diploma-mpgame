//! Presentation of the streamed world: textured ground and object sprites.
//!
//! World data (chunks, objects, [`WorldInfo`]) is owned by the streaming layer; this plugin
//! only reads it. Names from the manifest are mapped to textures by
//! `assets/world/{tiles,objects}.ron`, and textures are looked up in
//! `assets/textures/{tiles,objects}/<name>.png`.
//!
//! Tile appearance is composed by [`TileLayer`]s. The plugin registers [`TextureLayer`],
//! which picks each tile's texture and overlay layer; more layers are added with
//! [`TileLayerAppExt::add_tile_layer`] from their own plugins, which must be added after
//! this one. Neighboring textures are blended per pixel by `terrain.wgsl`, following the
//! `layers` and `blend` settings in `tiles.ron`.

mod catalog;
mod chunk_mesh;
mod chunks;
mod config;
mod layers;
mod material;
mod objects;
mod textures;

use bevy::{
    asset::{embedded_asset, io::file::FileAssetReader},
    prelude::*,
    sprite_render::Material2dPlugin,
};

#[allow(unused_imports, reason = "API for future tile layer plugins")]
pub use layers::{TextureLayer, TileContext, TileLayer, TileLayerAppExt, TileLayers, TileVisual};

use catalog::ObjectCatalog;
use config::PresentationConfig;

use crate::game::world::WorldInfo;
use chunks::{GroundTextures, PendingTileTextures};
use textures::AssetRoot;

pub struct WorldPresentationPlugin;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PresentationSet {
    /// Resolve names and request textures for a new world.
    Catalog,
    /// Attach visuals to new chunks and objects.
    Visuals,
}

impl Plugin for WorldPresentationPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");
        let assets = FileAssetReader::get_base_path().join("assets");
        app.add_plugins(Material2dPlugin::<material::TerrainMaterial>::default())
            .insert_resource(PresentationConfig::load(&assets))
            .insert_resource(AssetRoot(assets))
            .init_resource::<TileLayers>()
            .add_tile_layer(TextureLayer)
            .configure_sets(
                Update,
                (
                    PresentationSet::Catalog,
                    // The stream may end (and its manifest vanish) before catalogs are cleared.
                    PresentationSet::Visuals.run_if(resource_exists::<WorldInfo>),
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    (
                        chunks::prepare_tile_catalog,
                        objects::prepare_object_catalog,
                    )
                        .run_if(resource_exists_and_changed::<WorldInfo>),
                    (chunks::clear_ground, objects::clear_objects)
                        .run_if(resource_removed::<WorldInfo>),
                )
                    .in_set(PresentationSet::Catalog),
            )
            .add_systems(
                Update,
                (
                    chunks::assemble_ground_textures.run_if(resource_exists::<PendingTileTextures>),
                    chunks::spawn_chunk_visuals.run_if(resource_exists::<GroundTextures>),
                    chunks::stitch_chunk_borders,
                    objects::spawn_object_visuals.run_if(resource_exists::<ObjectCatalog>),
                )
                    .chain()
                    .in_set(PresentationSet::Visuals),
            );
    }
}
