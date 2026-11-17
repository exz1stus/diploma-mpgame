//! Ground: one mesh per streamed chunk, styled by the tile layers and drawn with the
//! terrain material once the tile texture array is assembled.

use bevy::prelude::*;
use shared::world::TILE_SIZE;

use super::{
    catalog::TileCatalog,
    chunk_mesh::build_chunk_mesh,
    layers::TileLayers,
    material::TerrainMaterial,
    textures::{AssetRoot, TILE_FOLDER, all_settled, request_textures, stack_layers},
};
use crate::game::world::{ChunkTiles, WorldInfo};

/// Draw order of the ground; objects and players render above it.
const CHUNK_Z: f32 = -10.0;

/// Tile textures requested for the current catalog, until they are assembled.
#[derive(Resource)]
pub struct PendingTileTextures(Vec<Option<Handle<Image>>>);

/// The material shared by every chunk of the current world.
#[derive(Resource)]
pub struct GroundMaterial(Handle<TerrainMaterial>);

/// Marks a chunk whose ground mesh has been spawned.
#[derive(Component)]
pub struct ChunkVisual;

/// Resolves tile textures for a newly streamed world and starts loading them.
pub fn prepare_tile_catalog(
    mut commands: Commands,
    world: Res<WorldInfo>,
    config: Res<super::config::PresentationConfig>,
    root: Res<AssetRoot>,
    asset_server: Res<AssetServer>,
) {
    let catalog = TileCatalog::build(&world, &config.tiles);
    let handles = request_textures(&root, TILE_FOLDER, &catalog.textures, &asset_server);
    commands.remove_resource::<GroundMaterial>();
    commands.insert_resource(PendingTileTextures(handles));
    commands.insert_resource(catalog);
}

/// Builds the texture array and ground material once every texture has loaded.
pub fn assemble_ground_material(
    mut commands: Commands,
    pending: Res<PendingTileTextures>,
    catalog: Res<TileCatalog>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    if !all_settled(&pending.0, &asset_server) {
        return;
    }
    let textures: Vec<_> = pending
        .0
        .iter()
        .map(|handle| handle.as_ref().and_then(|handle| images.get(handle)))
        .collect();
    let array = stack_layers(&catalog.textures, &textures);
    let material = materials.add(TerrainMaterial {
        tiles: images.add(array),
    });
    commands.remove_resource::<PendingTileTextures>();
    commands.insert_resource(GroundMaterial(material));
}

/// Gives every chunk without a ground mesh its mesh.
pub fn spawn_chunk_visuals(
    mut commands: Commands,
    chunks: Query<(Entity, &ChunkTiles), Without<ChunkVisual>>,
    world: Res<WorldInfo>,
    catalog: Res<TileCatalog>,
    layers: Res<TileLayers>,
    material: Res<GroundMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, tiles) in &chunks {
        let visuals = layers.style_chunk(tiles, &world, &catalog);
        let mesh = meshes.add(build_chunk_mesh(tiles.size, TILE_SIZE, &visuals));
        commands.entity(entity).insert(ChunkVisual);
        commands.spawn((
            Mesh2d(mesh),
            MeshMaterial2d(material.0.clone()),
            Transform::from_xyz(0.0, 0.0, CHUNK_Z),
            ChildOf(entity),
        ));
    }
}

pub fn clear_ground(mut commands: Commands) {
    commands.remove_resource::<TileCatalog>();
    commands.remove_resource::<PendingTileTextures>();
    commands.remove_resource::<GroundMaterial>();
}
