// Chunk ground. Per pixel, from the four nearest tile centers:
// - height: the highest overlay layer whose coverage (bilinear weight of its tiles and higher
//   ones, plus edge noise) exceeds 0.5 - overlay_reach wins, so higher ground overlays lower
//   ground with a crisp, irregular edge. Lower ground next to such an edge is darkened for a
//   few pixels (ambient occlusion), so steps read as height;
// - textures of the winning layer blend bilinearly (smooth_width wide);
// - temperature: the ground is multiplied by a tint. Each tile's tint is its biome's color
//   averaged over neighboring tiles (chunk_mesh::tile_data), and pixels interpolate those
//   bilinearly, so biome colors change in very smooth gradients, as in Minecraft.
// Positions are snapped to the texture pixel grid so everything stays pixel-art crisp.
//
// tools/worldgen-preview/dist/terrain.mjs implements the same algorithm; keep them in sync.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var tiles: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tile_data: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var tile_tints: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: TerrainParams;

struct TerrainParams {
    // Global tile coordinates of the chunk's minimum corner, chunk size, pixels per tile.
    chunk: vec4<f32>,
    // Smooth width, overlay reach, edge noise, noise scale.
    blend: vec4<f32>,
    // Height shadow strength, height shadow width in pixels, unused, unused.
    shade: vec4<f32>,
};

fn lattice(x: i32, y: i32, seed: u32) -> f32 {
    var h = (bitcast<u32>(x) * 0x27d4eb2du) ^ (bitcast<u32>(y) * 0x165667b1u)
        ^ ((seed + 1u) * 0x9e3779b1u);
    h = (h ^ (h >> 15u)) * 0x85ebca6bu;
    h = (h ^ (h >> 13u)) * 0xc2b2ae35u;
    h = h ^ (h >> 16u);
    return f32(h) * (1.0 / 4294967296.0);
}

fn value_noise(p: vec2<f32>, seed: u32) -> f32 {
    let cell = floor(p);
    let f = p - cell;
    let u = f * f * (3.0 - 2.0 * f);
    let x = i32(cell.x);
    let y = i32(cell.y);
    let bottom = mix(lattice(x, y, seed), lattice(x + 1, y, seed), u.x);
    let top = mix(lattice(x, y + 1, seed), lattice(x + 1, y + 1, seed), u.x);
    return mix(bottom, top, u.y);
}

// In [-0.5, 0.5], continuous in global tile coordinates.
fn edge_noise(p: vec2<f32>, scale: f32) -> f32 {
    return value_noise(p / scale, 0u) * 0.65 + value_noise(p * 2.0 / scale, 1u) * 0.35 - 0.5;
}

fn smooth_axis(f: f32, width: f32) -> f32 {
    if width <= 0.0 {
        return step(0.5, f);
    }
    return clamp((f - 0.5) / width + 0.5, 0.0, 1.0);
}

fn weights(f: vec2<f32>) -> vec4<f32> {
    return vec4<f32>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

fn texel_color(layer: u32, texel: vec2<i32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(tiles));
    let x = ((texel.x % size.x) + size.x) % size.x;
    // Image rows run top-down; world +Y is up.
    let y = size.y - 1 - (((texel.y % size.y) + size.y) % size.y);
    return textureLoad(tiles, vec2<i32>(x, y), i32(layer), 0).rgb;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let origin = params.chunk.xy;
    let size = i32(params.chunk.z);
    let pixels_per_tile = params.chunk.w;
    let texel = floor((origin + in.uv) * pixels_per_tile);
    let pixel = vec2<i32>(texel);
    let p = (texel + 0.5) / pixels_per_tile;
    let q = p - 0.5;
    let base = floor(q);
    let f = q - base;
    // Tile data texel of the bottom-left corner tile (the data has a one-tile border).
    let corner = vec2<i32>(base - origin) + 1;
    let offsets = array<vec2<i32>, 4>(vec2(0, 0), vec2(1, 0), vec2(0, 1), vec2(1, 1));
    var layer_of: array<u32, 4>;
    var level_of: array<u32, 4>;
    var tint = vec3(0.0);
    let coverage = weights(f);
    for (var i = 0; i < 4; i++) {
        let at = clamp(corner + offsets[i], vec2(0), vec2(size + 1));
        let data = textureLoad(tile_data, at, 0);
        layer_of[i] = data.r;
        level_of[i] = data.g;
        tint += vec3<f32>(textureLoad(tile_tints, at, 0).rgb) * coverage[i];
    }
    let fade = weights(vec2(
        smooth_axis(f.x, params.blend.x),
        smooth_axis(f.y, params.blend.x),
    ));

    // Height: the highest covering layer, and how close the nearest higher edge is.
    let threshold = 0.5 - params.blend.y - params.blend.z * edge_noise(p, params.blend.w);
    var chosen = min(min(level_of[0], level_of[1]), min(level_of[2], level_of[3]));
    for (var i = 0; i < 4; i++) {
        let level = level_of[i];
        if level <= chosen {
            continue;
        }
        var covered = 0.0;
        for (var j = 0; j < 4; j++) {
            if level_of[j] >= level {
                covered += coverage[j];
            }
        }
        if covered > threshold {
            chosen = level;
        }
    }
    var gap = 1.0e9;
    for (var i = 0; i < 4; i++) {
        if level_of[i] <= chosen {
            continue;
        }
        var covered = 0.0;
        for (var j = 0; j < 4; j++) {
            if level_of[j] >= level_of[i] {
                covered += coverage[j];
            }
        }
        gap = min(gap, threshold - covered);
    }

    // Textures of the chosen layer, blended bilinearly.
    var color = vec3(0.0);
    var total = 0.0;
    var fallback = 0;
    var strongest = -1.0;
    for (var i = 0; i < 4; i++) {
        if level_of[i] != chosen {
            continue;
        }
        if coverage[i] > strongest {
            strongest = coverage[i];
            fallback = i;
        }
        if fade[i] > 0.0 {
            color += texel_color(layer_of[i], pixel) * fade[i];
            total += fade[i];
        }
    }
    if total <= 0.0 {
        color = texel_color(layer_of[fallback], pixel);
    } else {
        color /= total;
    }

    // Ambient occlusion in whole-pixel steps: one step per pixel toward the higher edge.
    var light = 1.0;
    let ao_width = params.shade.y;
    if gap < 1.0e8 && ao_width > 0.0 {
        let steps = ceil(ao_width - gap * pixels_per_tile);
        if steps > 0.0 {
            light = 1.0 - params.shade.x * min(ao_width, steps) / ao_width;
        }
    }
    return vec4(color * srgb_to_linear(tint / 255.0) * light, 1.0);
}
