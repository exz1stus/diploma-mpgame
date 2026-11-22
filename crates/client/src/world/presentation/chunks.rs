//! Ground: one quad per streamed chunk, styled by the tile layers and drawn with a terrain
//! material once the tile texture array is assembled. A chunk's tile data carries a border
//! from its neighbors; it is rebuilt when a neighbor arrives so blending stays seamless.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use shared::world::{ChunkPos, TILE_SIZE};

use super::{
    atlas::{PixelRect, SheetPixels, load_sheet, sheet_settled},
    catalog::TileCatalog,
    chunk_mesh::{Neighborhood, build_chunk_mesh, tile_data, tile_data_image, tile_relief_image},
    config::PresentationConfig,
    layers::{TileLayers, TileVisual},
    material::{TerrainMaterial, TerrainParams},
    textures::{recolor, stack_layers},
};
use crate::{
    render_layers,
    world::{ChunkTiles, WorldChunk, WorldInfo},
};

/// The sheet and each catalog texture's region in it, until the texture array is assembled.
#[derive(Resource)]
pub struct PendingTileTextures {
    sheet: Handle<Image>,
    regions: Vec<Option<PixelRect>>,
    /// Make the art neutral in each texture's placeholder color (`tiles.ron` `recolor`).
    recolor: bool,
}

/// The tile texture array shared by every chunk of the current world.
#[derive(Resource)]
pub struct GroundTextures(Handle<Image>);

/// The quad that draws a chunk's ground; a child of the chunk.
#[derive(Component)]
pub struct GroundMesh;

/// A chunk's styled tiles and the tile data textures its material reads.
#[derive(Component)]
pub struct ChunkGround {
    visuals: Vec<TileVisual>,
    data: Handle<Image>,
    tints: Handle<Image>,
    relief: Handle<Image>,
}

/// Resolves tile textures for a newly streamed world and starts loading the sheet.
pub fn prepare_tile_catalog(
    mut commands: Commands,
    world: Res<WorldInfo>,
    config: Res<PresentationConfig>,
    asset_server: Res<AssetServer>,
) {
    let catalog = TileCatalog::build(&world, &config.tiles, |texture| {
        config.tile_region(texture).is_some()
    });
    let regions = catalog
        .textures
        .iter()
        .map(|texture| config.tile_region(&texture.name))
        .collect();
    commands.remove_resource::<GroundTextures>();
    commands.insert_resource(PendingTileTextures {
        sheet: load_sheet(&asset_server, &config.atlas),
        regions,
        recolor: config.tiles.recolor,
    });
    commands.insert_resource(catalog);
}

/// Crops every texture's region out of the sheet into the texture array once it has loaded.
pub fn assemble_ground_textures(
    mut commands: Commands,
    pending: Res<PendingTileTextures>,
    catalog: Res<TileCatalog>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    if !sheet_settled(&pending.sheet, &asset_server) {
        return;
    }
    let sheet = images.get(&pending.sheet).and_then(SheetPixels::new);
    if sheet.is_none() {
        warn!("atlas sheet unavailable; tiles use their placeholder colors");
    }
    let crops: Vec<Option<Image>> = pending
        .regions
        .iter()
        .zip(&catalog.textures)
        .map(|(region, texture)| {
            let mut image = sheet
                .as_ref()
                .zip(*region)
                .and_then(|(sheet, rect)| sheet.crop(rect))?;
            if pending.recolor {
                recolor(&mut image, texture.placeholder);
            }
            Some(image)
        })
        .collect();
    let textures: Vec<Option<&Image>> = crops.iter().map(Option::as_ref).collect();
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
        let relief = images.add(tile_relief_image(tiles.size, built.relief));
        let material = materials.add(TerrainMaterial {
            tiles: textures.0.clone(),
            data: data.clone(),
            tints: tints.clone(),
            relief: relief.clone(),
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
            relief,
        });
        commands.spawn((
            GroundMesh,
            Mesh2d(meshes.add(build_chunk_mesh(tiles.size, TILE_SIZE))),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, render_layers::GROUND),
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
        if let Some(mut image) = images.get_mut(&ground.relief) {
            image.data = Some(built.relief);
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
