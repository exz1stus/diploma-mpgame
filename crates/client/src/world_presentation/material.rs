//! The chunk ground material. One instance per chunk: it shares the tile texture array and
//! adds the chunk's tile data and position (see `terrain.wgsl`).

use bevy::{
    prelude::*, render::render_resource::AsBindGroup, render::render_resource::ShaderType,
    shader::ShaderRef, sprite_render::Material2d,
};

use super::config::TileBlend;

pub const SHADER_PATH: &str = "embedded://client/world_presentation/terrain.wgsl";

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TerrainMaterial {
    /// Tile textures, one array layer per texture.
    #[texture(0, dimension = "2d_array")]
    pub tiles: Handle<Image>,
    /// The chunk's tile data: texture and overlay layers (see [`super::chunk_mesh::tile_data`]).
    #[texture(1, sample_type = "u_int")]
    pub data: Handle<Image>,
    /// The chunk's smoothed tints.
    #[texture(2, sample_type = "u_int")]
    pub tints: Handle<Image>,
    #[uniform(3)]
    pub params: TerrainParams,
}

#[derive(ShaderType, Clone, Copy, Debug, PartialEq)]
pub struct TerrainParams {
    /// Global tile coordinates of the chunk's minimum corner, chunk size, pixels per tile.
    pub chunk: Vec4,
    /// Smooth width, overlay reach, edge noise, noise scale.
    pub blend: Vec4,
    /// Height shadow strength, height shadow width in pixels, unused, unused.
    pub shade: Vec4,
}

impl TerrainParams {
    pub fn new(origin: IVec2, size: u32, blend: TileBlend) -> Self {
        let blend = blend.sanitized();
        Self {
            chunk: Vec4::new(
                origin.x as f32,
                origin.y as f32,
                size as f32,
                blend.pixels_per_tile as f32,
            ),
            blend: Vec4::new(
                blend.smooth_width,
                blend.overlay_reach,
                blend.edge_noise,
                blend.noise_scale,
            ),
            shade: Vec4::new(blend.ao_strength, blend.ao_width as f32, 0.0, 0.0),
        }
    }
}

impl Material2d for TerrainMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader as Bevy would hand it to naga: imports and the bind group resolved.
    fn resolved_shader() -> String {
        let vertex_output = "struct VertexOutput {
            @builtin(position) position: vec4<f32>,
            @location(0) world_position: vec4<f32>,
            @location(1) world_normal: vec3<f32>,
            @location(2) uv: vec2<f32>,
        };";
        include_str!("terrain.wgsl")
            .replace(
                "#import bevy_sprite::mesh2d_vertex_output::VertexOutput",
                vertex_output,
            )
            .replace("#{MATERIAL_BIND_GROUP}", "2")
    }

    #[test]
    fn terrain_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(&resolved_shader())
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&resolved_shader())));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}"));
    }

    #[test]
    fn params_pack_chunk_position_and_sanitized_blend() {
        let blend = TileBlend {
            overlay_reach: 9.0,
            ..default()
        };
        let params = TerrainParams::new(IVec2::new(-64, 32), 32, blend);
        assert_eq!(params.chunk, Vec4::new(-64.0, 32.0, 32.0, 16.0));
        assert_eq!(params.blend.y, 0.5);
        assert_eq!(params.shade, Vec4::new(0.35, 3.0, 0.0, 0.0));
    }
}
