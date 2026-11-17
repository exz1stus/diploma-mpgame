#import bevy_sprite::mesh2d_functions as mesh_functions

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var tiles: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tiles_sampler: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) texture_layer: u32,
    @location(3) tint: vec4<f32>,
    @location(4) height: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) texture_layer: u32,
    @location(2) tint: vec4<f32>,
    // Unused until a relief layer writes heights.
    @location(3) height: f32,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world_position = mesh_functions::mesh2d_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    out.clip_position = mesh_functions::mesh2d_position_world_to_clip(world_position);
    out.uv = vertex.uv;
    out.texture_layer = vertex.texture_layer;
    out.tint = vertex.tint;
    out.height = vertex.height;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(tiles, tiles_sampler, in.uv, in.texture_layer) * in.tint;
}
