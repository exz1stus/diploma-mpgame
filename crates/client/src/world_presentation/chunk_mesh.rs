//! Chunk geometry and per-chunk shader data.
//!
//! A chunk is one quad whose UVs are chunk-local tile coordinates. The tiles' visuals go to
//! the shader as two small textures with a one-tile border taken from the neighboring
//! chunks, so blending across chunk edges is seamless: tile data (texture and overlay
//! layer) and smoothed tints.
//!
//! `tools/worldgen-preview/dist/terrain.mjs` (`buildTileData`) builds the same data.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::layers::TileVisual;

/// One quad covering `size` × `size` tiles, minimum corner at the origin. UVs are
/// chunk-local tile coordinates (+Y up), so the shader knows each pixel's tile position.
pub fn build_chunk_mesh(size: u32, tile_size: f32) -> Mesh {
    let side = size as f32;
    let corners = [[0.0, 0.0], [side, 0.0], [side, side], [0.0, side]];
    let positions: Vec<_> = corners
        .iter()
        .map(|[x, y]| [x * tile_size, y * tile_size, 0.0])
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, corners.to_vec())
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// Visuals of a chunk and of the chunks around it, for [`tile_data`].
pub struct Neighborhood<'a> {
    pub size: u32,
    /// The chunk's own visuals, `y * size + x`.
    pub center: &'a [TileVisual],
    /// Visuals of the chunk at a chunk offset (each axis in -1..=1), if loaded.
    pub neighbor: &'a dyn Fn(IVec2) -> Option<&'a [TileVisual]>,
}

impl Neighborhood<'_> {
    /// The visual of a chunk-local tile at most one chunk away. Tiles of missing neighbors
    /// repeat the chunk's nearest own tile.
    fn visual(&self, x: i32, y: i32) -> TileVisual {
        let size = self.size as i32;
        let offset = IVec2::new(x.div_euclid(size), y.div_euclid(size));
        let own = |x: i32, y: i32| {
            let (x, y) = (x.clamp(0, size - 1), y.clamp(0, size - 1));
            self.center[(y * size + x) as usize]
        };
        if offset == IVec2::ZERO {
            return own(x, y);
        }
        match (self.neighbor)(offset) {
            Some(tiles) if tiles.len() == self.center.len() => {
                let (lx, ly) = (x.rem_euclid(size), y.rem_euclid(size));
                tiles[(ly * size + lx) as usize]
            }
            _ => own(x, y),
        }
    }
}

/// Side of a chunk's tile data textures: the chunk plus a one-tile border.
pub fn tile_data_side(size: u32) -> u32 {
    size + 2
}

/// A chunk's shader data, RGBA8 texels row-major from the bottom row; texel (x + 1, y + 1)
/// holds tile (x, y).
pub struct TileData {
    /// (texture layer, overlay layer, 0, 0).
    pub layers: Vec<u8>,
    /// (r, g, b, 255): the tint averaged over the (2r + 1)² box of tiles around the tile,
    /// rounded to the nearest integer.
    pub tints: Vec<u8>,
}

pub fn tile_data(neighborhood: &Neighborhood, tint_radius: u32) -> TileData {
    let size = neighborhood.size as i32;
    let side = tile_data_side(neighborhood.size) as i32;
    let radius = (tint_radius as i32).min(size - 1).max(0);
    // Exact integer box sums: horizontal over the rows the vertical pass needs, then vertical.
    let span = side + 2 * radius;
    let mut rows = vec![[0u32; 3]; (span * side) as usize];
    for ry in 0..span {
        let y = ry - 1 - radius;
        for cx in 0..side {
            let sum = &mut rows[(ry * side + cx) as usize];
            for dx in -radius..=radius {
                let tint = neighborhood.visual(cx - 1 + dx, y).tint;
                for c in 0..3 {
                    sum[c] += u32::from(tint[c]);
                }
            }
        }
    }
    let count = ((2 * radius + 1) * (2 * radius + 1)) as u32;
    let mut layers = Vec::with_capacity((side * side * 4) as usize);
    let mut tints = Vec::with_capacity((side * side * 4) as usize);
    for cy in 0..side {
        for cx in 0..side {
            let visual = neighborhood.visual(cx - 1, cy - 1);
            layers.extend([byte(visual.texture_layer), byte(visual.overlay), 0, 0]);
            let mut sum = [0u32; 3];
            for dy in 0..=2 * radius {
                let row = rows[((cy + dy) * side + cx) as usize];
                for c in 0..3 {
                    sum[c] += row[c];
                }
            }
            tints.extend(sum.map(|value| ((value + count / 2) / count) as u8));
            tints.push(255);
        }
    }
    TileData { layers, tints }
}

fn byte(value: u32) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

/// An integer texture of a chunk's tile data, read with `textureLoad`.
pub fn tile_data_image(size: u32, data: Vec<u8>) -> Image {
    let side = tile_data_side(size);
    Image::new(
        Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Uint,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    #[test]
    fn one_quad_spans_the_chunk_with_tile_uvs() {
        let mesh = build_chunk_mesh(4, 10.0);
        assert_eq!(mesh.count_vertices(), 4);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing positions");
        };
        assert_eq!(positions[2], [40.0, 40.0, 0.0]);
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("missing uvs");
        };
        assert_eq!(uvs[2], [4.0, 4.0]);
    }

    fn visuals(texture: u32, tint: [u8; 3]) -> Vec<TileVisual> {
        (0..16)
            .map(|index| TileVisual {
                texture_layer: texture,
                overlay: index,
                tint,
            })
            .collect()
    }

    fn texel(data: &[u8], side: usize, x: i32, y: i32) -> &[u8] {
        let index = (y + 1) as usize * side + (x + 1) as usize;
        &data[index * 4..index * 4 + 4]
    }

    #[test]
    fn border_texels_come_from_neighbors_or_repeat_the_edge() {
        let center = visuals(1, [255; 3]);
        let east = visuals(2, [255; 3]);
        let neighbor = |offset: IVec2| (offset == IVec2::X).then_some(east.as_slice());
        let data = tile_data(
            &Neighborhood {
                size: 4,
                center: &center,
                neighbor: &neighbor,
            },
            0,
        );
        let side = tile_data_side(4) as usize;
        assert_eq!(data.layers.len(), side * side * 4);
        assert_eq!(&texel(&data.layers, side, 3, 0)[..2], [1, 3], "own tile");
        assert_eq!(
            &texel(&data.layers, side, 4, 1)[..2],
            [2, 4],
            "east tile (0, 1)"
        );
        assert_eq!(
            &texel(&data.layers, side, -1, 1)[..2],
            [1, 4],
            "edge repeats"
        );
        assert_eq!(&texel(&data.layers, side, -1, -1)[..2], [1, 0]);
    }

    #[test]
    fn tints_are_box_averaged_across_chunk_borders() {
        let center = visuals(0, [0, 0, 0]);
        let east = visuals(0, [255, 90, 9]);
        let neighbor = |offset: IVec2| (offset == IVec2::X).then_some(east.as_slice());
        let neighborhood = Neighborhood {
            size: 4,
            center: &center,
            neighbor: &neighbor,
        };
        let side = tile_data_side(4) as usize;
        let sharp = tile_data(&neighborhood, 0);
        assert_eq!(texel(&sharp.tints, side, 3, 0), [0, 0, 0, 255]);
        assert_eq!(texel(&sharp.tints, side, 4, 0), [255, 90, 9, 255]);
        let smooth = tile_data(&neighborhood, 1);
        // Tile (3, 1): one of the three columns of its 3 × 3 box lies east of the border.
        assert_eq!(texel(&smooth.tints, side, 3, 1), [85, 30, 3, 255]);
        // Tile (4, 1): two of three columns.
        assert_eq!(texel(&smooth.tints, side, 4, 1), [170, 60, 6, 255]);
        assert_eq!(texel(&smooth.tints, side, 1, 2), [0, 0, 0, 255]);
    }
}
