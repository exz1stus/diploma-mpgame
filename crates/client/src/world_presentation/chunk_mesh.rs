//! Chunk geometry: one textured quad per tile, carrying its [`TileVisual`] as vertex data.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, MeshVertexAttribute, PrimitiveTopology, VertexFormat},
    prelude::*,
};

use super::layers::TileVisual;

/// Texture-array layer of the tile (flat across the quad).
pub const ATTRIBUTE_TEXTURE_LAYER: MeshVertexAttribute =
    MeshVertexAttribute::new("TileTextureLayer", 0x7469_6c65_0001, VertexFormat::Uint32);
/// Relief input of the tile.
pub const ATTRIBUTE_HEIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("TileHeight", 0x7469_6c65_0002, VertexFormat::Float32);

/// Quad corners (x, y) in tiles, and texture UVs (image rows run top-down).
const CORNERS: [([f32; 2], [f32; 2]); 4] = [
    ([0.0, 0.0], [0.0, 1.0]),
    ([1.0, 0.0], [1.0, 1.0]),
    ([1.0, 1.0], [1.0, 0.0]),
    ([0.0, 1.0], [0.0, 0.0]),
];

/// `visuals` are in chunk tile order (`y * size + x`, +Y up). Positions are chunk-local
/// world units with the chunk's minimum corner at the origin.
pub fn build_chunk_mesh(size: u32, tile_size: f32, visuals: &[TileVisual]) -> Mesh {
    let vertex_count = visuals.len() * CORNERS.len();
    let mut positions = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut layers = Vec::with_capacity(vertex_count);
    let mut tints = Vec::with_capacity(vertex_count);
    let mut heights = Vec::with_capacity(vertex_count);
    let mut indices = Vec::with_capacity(visuals.len() * 6);
    for (index, visual) in visuals.iter().enumerate() {
        let tile = Vec2::new((index as u32 % size) as f32, (index as u32 / size) as f32);
        let first = positions.len() as u32;
        for (corner, uv) in CORNERS {
            let position = (tile + Vec2::from(corner)) * tile_size;
            positions.push([position.x, position.y, 0.0]);
            uvs.push(uv);
            layers.push(visual.texture_layer);
            tints.push(visual.tint.to_f32_array());
            heights.push(visual.height);
        }
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, tints)
    .with_inserted_attribute(ATTRIBUTE_TEXTURE_LAYER, layers)
    .with_inserted_attribute(ATTRIBUTE_HEIGHT, heights)
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    fn positions(mesh: &Mesh) -> &[[f32; 3]] {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(values)) => values,
            other => panic!("unexpected positions {other:?}"),
        }
    }

    #[test]
    fn one_quad_per_tile_with_row_zero_at_the_bottom() {
        let visuals: Vec<_> = (0..4)
            .map(|layer| TileVisual {
                texture_layer: layer,
                ..default()
            })
            .collect();
        let mesh = build_chunk_mesh(2, 10.0, &visuals);
        assert_eq!(mesh.count_vertices(), 16);
        assert_eq!(mesh.indices().unwrap().len(), 24);
        let positions = positions(&mesh);
        // Tile 2 is (x 0, y 1): its quad starts one tile up.
        assert_eq!(positions[8], [0.0, 10.0, 0.0]);
        assert_eq!(positions[10], [10.0, 20.0, 0.0]);
        let Some(VertexAttributeValues::Uint32(layers)) = mesh.attribute(ATTRIBUTE_TEXTURE_LAYER)
        else {
            panic!("missing texture layers");
        };
        assert_eq!(&layers[8..12], &[2, 2, 2, 2]);
    }
}
