//! Presentation of the streamed world: textured ground and object sprites.
//!
//! World data (chunks, objects, [`WorldInfo`]) is owned by the streaming layer; this plugin
//! only reads it. All art comes from one sprite sheet whose named regions are listed in
//! `assets/world/atlas.ron`; `tiles.ron` and `objects.ron` choose which regions biomes and
//! object kinds use and how objects are placed ([`placement`]). Changing the config, by
//! F5 or later from the settings UI, re-styles the loaded world ([`reload`]).
//!
//! Tile appearance is composed by [`TileLayer`]s. The plugin registers [`TextureLayer`],
//! which picks each tile's texture and overlay layer; more layers are added with
//! [`TileLayerAppExt::add_tile_layer`] from their own plugins, which must be added after
//! this one. Neighboring textures are blended per pixel by `terrain.wgsl`, following the
//! `layers` and `blend` settings in `tiles.ron`.

mod atlas;
mod catalog;
mod chunk_mesh;
mod chunks;
mod config;
mod layers;
mod material;
mod objects;
mod placement;
mod reload;
mod shadows;
mod textures;

use bevy::{asset::embedded_asset, prelude::*, sprite_render::Material2dPlugin};

#[allow(unused_imports, reason = "API for future tile layer plugins")]
pub use layers::{TextureLayer, TileContext, TileLayer, TileLayerAppExt, TileLayers, TileVisual};

use catalog::ObjectCatalog;
use config::PresentationConfig;

use crate::{assets::AssetRoot, schedule::ClientSet, world::WorldInfo};
use chunks::{GroundTextures, PendingTileTextures};
use objects::ObjectSheet;

pub struct WorldPresentationPlugin;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PresentationSet {
    /// Reload the config, then resolve names and load the sheet for a new or re-styled world.
    Catalog,
    /// Attach visuals to new chunks and objects.
    Visuals,
}

impl Plugin for WorldPresentationPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");
        embedded_asset!(app, "shadow.wgsl");
        let config = PresentationConfig::load(&app.world_mut().get_resource_or_init::<AssetRoot>());
        app.add_plugins((
            Material2dPlugin::<material::TerrainMaterial>::default(),
            Material2dPlugin::<shadows::ShadowMaterial>::default(),
        ))
        .insert_resource(config)
        .init_resource::<TileLayers>()
        .init_resource::<shadows::SunLight>()
        .init_resource::<shadows::ShadowAssets>()
        .add_tile_layer(TextureLayer)
        .configure_sets(
            Update,
            (
                PresentationSet::Catalog,
                // The stream may end (and its manifest vanish) before catalogs are cleared.
                PresentationSet::Visuals.run_if(resource_exists::<WorldInfo>),
            )
                .chain()
                .in_set(ClientSet::Present),
        )
        .add_systems(
            Update,
            (
                reload::reload_config_on_key,
                reload::restyle_world.run_if(
                    resource_changed::<PresentationConfig>
                        .and_then(not(resource_added::<PresentationConfig>)),
                ),
                (
                    chunks::prepare_tile_catalog,
                    objects::prepare_object_catalog,
                )
                    .run_if(resource_exists_and_changed::<WorldInfo>),
                (chunks::clear_ground, objects::clear_objects)
                    .run_if(resource_removed::<WorldInfo>),
            )
                .chain()
                .in_set(PresentationSet::Catalog),
        )
        .add_systems(
            Update,
            (
                chunks::assemble_ground_textures.run_if(resource_exists::<PendingTileTextures>),
                chunks::spawn_chunk_visuals.run_if(resource_exists::<GroundTextures>),
                chunks::stitch_chunk_borders,
                objects::recolor_object_sheet.run_if(resource_exists::<ObjectSheet>),
                objects::spawn_object_visuals.run_if(resource_exists::<ObjectCatalog>),
                shadows::update_sun,
                shadows::apply_sun_to_shadows,
            )
                .chain()
                .in_set(PresentationSet::Visuals),
        );
    }
}
