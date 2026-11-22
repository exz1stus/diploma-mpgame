// Chunk ground. Per pixel, from the four nearest tile centers:
// - height: the highest overlay layer whose coverage (bilinear weight of its tiles and higher
//   ones, plus edge noise) exceeds 0.5 - overlay_reach wins, so higher ground overlays lower
//   ground with a crisp, irregular edge. Lower ground next to such an edge is darkened for a
//   few pixels (ambient occlusion), so steps read as height. The shadow is scaled by
//   ao_south where the higher ground lies north of the pixel (the south side of a step) and
//   by ao_north where it lies south, interpolated in between;
// - biomes: within the winning layer, the biome whose tiles cover the pixel most, plus a smooth
//   noise field of its own, wins: texture and tint change together on a crisp, irregular edge,
//   like height steps but without shading;
// - temperature: the ground is multiplied by the winning biome's tint, interpolated from its
//   tiles' tints (chunk_mesh::tile_data can average them within a layer to soften colors
//   inside a biome);
// - terrain height: the terrain height and slope, interpolated bilinearly from the tiles'
//   (chunk_mesh::TileData::relief), shade the ground: lower ground darker and higher ground
//   brighter around height_reference, and slopes facing the north-west lit, the others
//   shaded, so craters read as bowls and the light varies over the land rather than
//   repeating with the textures. Same formula as the preview's flat view (displayFactor).
// Positions are snapped to the texture pixel grid so everything stays pixel-art crisp.
//
// tools/worldgen-preview/dist/terrain.mjs implements the same algorithm; keep them in sync.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var tiles: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tile_data: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var tile_tints: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: TerrainParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var tile_relief: texture_2d<u32>;

struct TerrainParams {
    // Global tile coordinates of the chunk's minimum corner, chunk size, pixels per tile.
    chunk: vec4<f32>,
    // Unused, overlay reach, edge noise, noise scale.
    blend: vec4<f32>,
    // Height shadow strength, height shadow width in pixels, south scale, north scale.
    shade: vec4<f32>,
    // Height shade, height reference, slope shade, unused.
    relief: vec4<f32>,
};

// Slope shading: brightness per unit of slope (height per tile) toward the north-west, and its
// limit. Matches RELIEF_GAIN and RELIEF_LIMIT in the preview's protocol.mjs.
const RELIEF_GAIN: f32 = 50.0;
const RELIEF_LIMIT: f32 = 0.4;
const SLOPE_BIAS: f32 = 32768.0;
const HEIGHT_UNIT: f32 = 65535.0;

// (height, east slope, north slope) of a relief texel, in normalized height (per tile).
fn relief_at(at: vec2<i32>) -> vec3<f32> {
    let texel = vec3<f32>(textureLoad(tile_relief, at, 0).rgb);
    return vec3(texel.x, texel.y - SLOPE_BIAS, texel.z - SLOPE_BIAS) / HEIGHT_UNIT;
}

// Brightness from the terrain: height around the reference, times light on north-west slopes.
fn terrain_light(relief: vec3<f32>) -> f32 {
    let height = clamp(1.0 + params.relief.x * (relief.x - params.relief.y), 0.25, 1.75);
    // Facing the north-west: rising toward the east, falling toward the north (+Y).
    let slope = relief.y - relief.z;
    let lit = 1.0 + clamp(slope * RELIEF_GAIN * params.relief.z, -RELIEF_LIMIT, RELIEF_LIMIT);
    return height * lit;
}

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

// Two octaves in [-0.5, 0.5], continuous in global tile coordinates; `seed` and `seed + 1`.
fn noise_field(p: vec2<f32>, scale: f32, seed: u32) -> f32 {
    return value_noise(p / scale, seed) * 0.65 + value_noise(p * 2.0 / scale, seed + 1u) * 0.35 - 0.5;
}

// Height edges.
fn edge_noise(p: vec2<f32>, scale: f32) -> f32 {
    return noise_field(p, scale, 0u);
}

// Each biome's own edge field.
fn biome_noise(p: vec2<f32>, scale: f32, biome: u32) -> f32 {
    return noise_field(p, scale, 32u + 2u * biome);
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
    var biome_of: array<u32, 4>;
    var tint_of: array<vec3<f32>, 4>;
    let coverage = weights(f);
    var relief = vec3(0.0);
    for (var i = 0; i < 4; i++) {
        let at = clamp(corner + offsets[i], vec2(0), vec2(size + 1));
        relief += relief_at(at) * coverage[i];
        let data = textureLoad(tile_data, at, 0);
        layer_of[i] = data.r;
        level_of[i] = data.g;
        biome_of[i] = data.b | (data.a << 8u);
        tint_of[i] = vec3<f32>(textureLoad(tile_tints, at, 0).rgb);
    }

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
    var gap_level = -1;
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
        if threshold - covered < gap {
            gap = threshold - covered;
            gap_level = i32(level_of[i]);
        }
    }

    // Biome: within the chosen layer, the biome with the most coverage plus its noise.
    var winner = 0;
    var best = -1.0e9;
    for (var i = 0; i < 4; i++) {
        if level_of[i] != chosen {
            continue;
        }
        var first = true;
        for (var j = 0; j < i; j++) {
            if level_of[j] == chosen && biome_of[j] == biome_of[i] {
                first = false;
            }
        }
        if !first {
            continue;
        }
        var covered = 0.0;
        for (var j = 0; j < 4; j++) {
            if level_of[j] == chosen && biome_of[j] == biome_of[i] {
                covered += coverage[j];
            }
        }
        let score = covered + params.blend.z * biome_noise(p, params.blend.w, biome_of[i]);
        if score > best {
            best = score;
            winner = i;
        }
    }

    // The winning biome's texture, and its tiles' tints interpolated bilinearly.
    let color = texel_color(layer_of[winner], pixel);
    var tint_sum = vec3(0.0);
    var tint_total = 0.0;
    for (var i = 0; i < 4; i++) {
        if level_of[i] == chosen && biome_of[i] == biome_of[winner] {
            let weight = max(coverage[i], 1.0e-6);
            tint_sum += tint_of[i] * weight;
            tint_total += weight;
        }
    }
    let tint = tint_sum / tint_total;

    var light = 1.0;
    if gap_level >= 0 && params.shade.y > 0.0 {
        // Which side the higher ground is on: the gradient of its coverage. `north` is 1 when
        // it lies due north of the pixel (+Y), -1 due south.
        var c: array<f32, 4>;
        for (var i = 0; i < 4; i++) {
            c[i] = select(0.0, 1.0, i32(level_of[i]) >= gap_level);
        }
        let gradient = vec2(
            (1.0 - f.y) * (c[1] - c[0]) + f.y * (c[3] - c[2]),
            (1.0 - f.x) * (c[2] - c[0]) + f.x * (c[3] - c[1]),
        );
        let length_of = length(gradient);
        var north = 0.0;
        if length_of > 0.0 {
            north = gradient.y / length_of;
        }
        let side = mix(params.shade.w, params.shade.z, (north + 1.0) / 2.0);
        // Ambient occlusion in whole-pixel steps: one step per pixel toward the higher edge.
        let width = params.shade.y * side;
        let steps = ceil(width - gap * pixels_per_tile);
        if width > 0.0 && steps > 0.0 {
            light = 1.0 - min(1.0, params.shade.x * side) * min(width, steps) / width;
        }
    }
    light *= terrain_light(relief);
    return vec4(color * srgb_to_linear(tint / 255.0) * light, 1.0);
}
