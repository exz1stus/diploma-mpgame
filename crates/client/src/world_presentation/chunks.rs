//! Ground: one quad per streamed chunk, styled by the tile layers and drawn with a terrain
//! material once the tile texture array is assembled. A chunk's tile data carries a border
//! from its neighbors; it is rebuilt when a neighbor arrives so blending stays seamless.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use shared::world::{ChunkPos, TILE_SIZE};

use super::{
    catalog::TileCatalog,
    chunk_mesh::{Neighborhood, build_chunk_mesh, tile_data, tile_data_image},
    config::PresentationConfig,
    layers::{TileLayers, TileVisual},
    material::{TerrainMaterial, TerrainParams},
    textures::{
        AssetRoot, TILE_FOLDER, all_settled, request_textures, stack_layers, texture_file_exists,
    },
};
use crate::game::world::{ChunkTiles, WorldChunk, WorldInfo};

/// Draw order of the ground; objects and players render above it.
const CHUNK_Z: f32 = -10.0;

/// Tile textures requested for the current catalog, until they are assembled.
#[derive(Resource)]
pub struct PendingTileTextures(Vec<Option<Handle<Image>>>);

/// The tile texture array shared by every chunk of the current world.
#[derive(Resource)]
pub struct GroundTextures(Handle<Image>);

/// A chunk's styled tiles and the tile data textures its material reads.
#[derive(Component)]
pub struct ChunkGround {
    visuals: Vec<TileVisual>,
    data: Handle<Image>,
    tints: Handle<Image>,
}

/// Resolves tile textures for a newly streamed world and starts loading them.
pub fn prepare_tile_catalog(
    mut commands: Commands,
    world: Res<WorldInfo>,
    config: Res<PresentationConfig>,
    root: Res<AssetRoot>,
    asset_server: Res<AssetServer>,
) {
    let catalog = TileCatalog::build(&world, &config.tiles, |texture| {
        texture_file_exists(&root, TILE_FOLDER, texture)
    });
    let handles = request_textures(&root, TILE_FOLDER, &catalog.textures, &asset_server);
    commands.remove_resource::<GroundTextures>();
    commands.insert_resource(PendingTileTextures(handles));
    commands.insert_resource(catalog);
}

/// Builds the texture array once every texture has loaded.
pub fn assemble_ground_textures(
    mut commands: Commands,
    pending: Res<PendingTileTextures>,
    catalog: Res<TileCatalog>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
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
    commands.remove_resource::<PendingTileTextures>();
    commands.insert_resource(GroundTextures(images.add(array)));
}

/// Gives every chunk without ground its quad and material. The tile data border starts as
/// a copy of the chunk's edge; [`stitch_chunk_borders`] fills in loaded neighbors.
pub fn spawn_chunk_visuals(
    mut commands: Commands,
    chunks: Query<(Entity, &WorldChunk, &ChunkTiles), Without<ChunkGround>>,
    world: Res<WorldInfo>,
    catalog: Res<TileCatalog>,
    layers: Res<TileLayers>,
    config: Res<PresentationConfig>,
    textures: Res<GroundTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    for (entity, chunk, tiles) in &chunks {
        let visuals = layers.style_chunk(tiles, &world, &catalog);
        let built = tile_data(
            &Neighborhood {
                size: tiles.size,
                center: &visuals,
                neighbor: &|_| None,
            },
            config.tiles.blend.sanitized().tint_radius,
        );
        let data = images.add(tile_data_image(tiles.size, built.layers));
        let tints = images.add(tile_data_image(tiles.size, built.tints));
        let material = materials.add(TerrainMaterial {
            tiles: textures.0.clone(),
            data: data.clone(),
            tints: tints.clone(),
            params: TerrainParams::new(
                chunk_origin(chunk.pos, tiles.size),
                tiles.size,
                config.tiles.blend,
            ),
        });
        commands.entity(entity).insert(ChunkGround {
            visuals,
            data,
            tints,
        });
        commands.spawn((
            Mesh2d(meshes.add(build_chunk_mesh(tiles.size, TILE_SIZE))),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, CHUNK_Z),
            ChildOf(entity),
        ));
    }
}

/// Rebuilds the tile data of new chunks and of their loaded neighbors.
pub fn stitch_chunk_borders(
    added: Query<&WorldChunk, Added<ChunkGround>>,
    chunks: Query<(&WorldChunk, &ChunkTiles, &ChunkGround)>,
    config: Res<PresentationConfig>,
    mut images: ResMut<Assets<Image>>,
) {
    if added.is_empty() {
        return;
    }
    let tint_radius = config.tiles.blend.sanitized().tint_radius;
    let by_pos: HashMap<ChunkPos, (&ChunkTiles, &ChunkGround)> = chunks
        .iter()
        .map(|(chunk, tiles, ground)| (chunk.pos, (tiles, ground)))
        .collect();
    let mut dirty = HashSet::new();
    for chunk in &added {
        for offset in neighborhood_offsets() {
            let pos = offset_pos(chunk.pos, offset);
            if by_pos.contains_key(&pos) {
                dirty.insert(pos);
            }
        }
    }
    for pos in dirty {
        let (tiles, ground) = by_pos[&pos];
        let neighbor = |offset: IVec2| {
            by_pos
                .get(&offset_pos(pos, offset))
                .filter(|(other, _)| other.size == tiles.size)
                .map(|(_, other)| other.visuals.as_slice())
        };
        let built = tile_data(
            &Neighborhood {
                size: tiles.size,
                center: &ground.visuals,
                neighbor: &neighbor,
            },
            tint_radius,
        );
        if let Some(mut image) = images.get_mut(&ground.data) {
            image.data = Some(built.layers);
        }
        if let Some(mut image) = images.get_mut(&ground.tints) {
            image.data = Some(built.tints);
        }
    }
}

fn neighborhood_offsets() -> impl Iterator<Item = IVec2> {
    (-1..=1).flat_map(|y| (-1..=1).map(move |x| IVec2::new(x, y)))
}

fn offset_pos(pos: ChunkPos, offset: IVec2) -> ChunkPos {
    ChunkPos {
        x: pos.x + i64::from(offset.x),
        y: pos.y + i64::from(offset.y),
    }
}

/// Global tile coordinates of a chunk's minimum corner.
fn chunk_origin(pos: ChunkPos, size: u32) -> IVec2 {
    IVec2::new(
        (pos.x * i64::from(size)) as i32,
        (pos.y * i64::from(size)) as i32,
    )
}

pub fn clear_ground(mut commands: Commands) {
    commands.remove_resource::<TileCatalog>();
    commands.remove_resource::<PendingTileTextures>();
    commands.remove_resource::<GroundTextures>();
}
