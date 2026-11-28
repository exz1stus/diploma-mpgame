// Object silhouettes on the ground: sun shadows, ambient occlusion, and the shade on an object's
// own bottom rows. Ground darkening is decided per terrain texel: every fragment snaps to the
// center of its texel on the world's tile pixel grid (texel size style.z, origin at world 0),
// maps that point into the object's art, and darkens the whole texel or none of it, so shadows
// and occlusion are made of the same square pixels as the ground.
//
// Meshes are a quad in art pixels with the object's foot at the origin (+Y up), grown by a
// margin so edge texels are never cut. Per vertex: uv = sheet pixel at the vertex, color =
// (mirrored ? -1 : 1, art left, art bottom, contact), normal = (art width, art height, lying);
// the art spans [left, left + width] × [bottom, bottom + height] in local pixels. `contact` is
// the art's transparent bottom rows: it touches the ground at y = bottom + contact (`base`).
//
// Modes (style.x):
// 0 sun shadow. Upright art (lying = 0): a ground point g shows the art pixel p with
//   g = (p.x + shear.x * h, base + shear.y * h), h = p.y - base: the base stays on the ground,
//   the top slides away from the sun. Art lying on the ground, `lying` pixels tall: g is in
//   shadow when the outline covers g - shear * t for some t in [0, lying], the outline pushed
//   away from the sun, so the shadow always touches the object.
// 1 occlusion under standing art: texels near the art's lowest `reach` rows darken in
//   whole-texel steps, darkest next to it, fading out over `reach` texels (like the terrain's
//   height shading).
// 2 occlusion around a ground decal: the same steps around all of its pixels.
// 3 base shade: the art's own lowest `reach` rows darken toward the bottom row, on the art's
//   pixel grid; drawn over the object so it sits in the ground.

#import bevy_sprite::{
    mesh2d_functions as mesh_functions,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var sheet: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var sheet_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> params: ShadowParams;

struct ShadowParams {
    // xy: sun shear (ground offset per unit of height); z: darkness; w: unused.
    shear: vec4<f32>,
    // x: mode; y: reach (texels for occlusion, rows for base shade); z: terrain texel size in
    // world units; w: unused.
    style: vec4<f32>,
};

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) art_size: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) art: vec4<f32>,
};

struct Fragment {
    @builtin(position) position: vec4<f32>,
    // The fragment on the ground: world position, and local art pixels (after the shear).
    @location(0) world: vec2<f32>,
    @location(1) ground: vec2<f32>,
    // The unsheared local point and its sheet pixel, to map other local points to the sheet.
    @location(2) local: vec2<f32>,
    @location(3) uv: vec2<f32>,
    // Mirroring, art left, art bottom, contact; art width, height, lying; world units per
    // local pixel.
    @location(4) art: vec4<f32>,
    @location(5) art_size: vec3<f32>,
    @location(6) scale: f32,
};

fn mode() -> i32 {
    return i32(params.style.x + 0.5);
}

// Local y where the art touches the ground: above its transparent bottom rows.
fn base_of(art: vec4<f32>) -> f32 {
    return art.z + art.w;
}

// Where the sun shadow puts local vertex `p`.
fn sun_vertex(p: vec2<f32>, art: vec4<f32>, art_size: vec3<f32>) -> vec2<f32> {
    let shear = params.shear.xy;
    let lying = art_size.z;
    if lying > 0.0 {
        // Stretch the quad over the outline and the outline pushed by the full height.
        let push = shear * lying;
        let center = art.yz + art_size.xy * 0.5;
        let outward = select(min(push, vec2<f32>(0.0)), max(push, vec2<f32>(0.0)), p > center);
        return p + outward;
    }
    let base = base_of(art);
    let h = p.y - base;
    return vec2<f32>(p.x + shear.x * h, base + shear.y * h);
}

@vertex
fn vertex(vertex: Vertex) -> Fragment {
    let p = vertex.position.xy;
    var ground = p;
    if mode() == 0 {
        ground = sun_vertex(p, vertex.art, vertex.art_size);
    }
    var out: Fragment;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world = mesh_functions::mesh2d_position_local_to_world(
        world_from_local,
        vec4<f32>(ground, vertex.position.z, 1.0),
    );
    out.position = mesh_functions::mesh2d_position_world_to_clip(world);
    out.world = world.xy;
    out.ground = ground;
    out.local = p;
    out.uv = vertex.uv;
    out.art = vertex.art;
    out.art_size = vertex.art_size;
    out.scale = length(world_from_local[0].xyz);
    return out;
}

// Coverage of the art pixel containing local point `p`; 0 outside the art.
fn art_alpha(in: Fragment, p: vec2<f32>) -> f32 {
    let low = in.art.yz;
    let high = low + in.art_size.xy;
    if any(p < low) || any(p >= high) {
        return 0.0;
    }
    let uv = in.uv + vec2<f32>((p.x - in.local.x) * in.art.x, in.local.y - p.y);
    return textureLoad(sheet, vec2<i32>(floor(uv)), 0).a;
}

fn opaque(in: Fragment, p: vec2<f32>) -> bool {
    return art_alpha(in, p) > 0.5;
}

// Center of the terrain texel containing world point `w`.
fn texel_center(w: vec2<f32>) -> vec2<f32> {
    let texel = params.style.z;
    return (floor(w / texel) + 0.5) * texel;
}

// The ground point (local art pixels, after the shear) at world point `w`.
fn ground_at(in: Fragment, w: vec2<f32>) -> vec2<f32> {
    return in.ground + (w - in.world) / in.scale;
}

// Whether the art casts a sun shadow on the texel at world point `center`.
fn sun_coverage(in: Fragment, center: vec2<f32>) -> f32 {
    let g = ground_at(in, center);
    let shear = params.shear.xy;
    let lying = in.art_size.z;
    if lying > 0.0 {
        // Some height of the lying outline, pushed away from the sun, covers the texel. One
        // step per art pixel of height moves the outline by at most one shadow length.
        let steps = i32(ceil(lying));
        for (var i = 0; i <= steps; i++) {
            if opaque(in, g - shear * (lying * f32(i) / f32(steps))) {
                return 1.0;
            }
        }
        return 0.0;
    }
    // Undo the shear about the art's base to find the pixel casting the texel.
    let base = base_of(in.art);
    let h = (g.y - base) / shear.y;
    return select(0.0, 1.0, opaque(in, vec2<f32>(g.x - shear.x * h, base + h)));
}

// Whole-texel steps: 1 one texel from opaque art, fading over `reach` texels; only art rows up
// to `rows` above the art's base count (all rows when `rows` is negative).
fn occlusion(in: Fragment, center: vec2<f32>, rows: f32) -> f32 {
    if opaque(in, ground_at(in, center)) {
        return 0.0;
    }
    let reach = i32(params.style.y + 0.5);
    let texel = params.style.z;
    let lowest = base_of(in.art) + rows;
    var nearest = reach + 1;
    for (var dy = -reach; dy <= reach; dy++) {
        for (var dx = -reach; dx <= reach; dx++) {
            let p = ground_at(in, center + vec2<f32>(f32(dx), f32(dy)) * texel);
            if rows >= 0.0 && p.y > lowest {
                continue;
            }
            if opaque(in, p) {
                nearest = min(nearest, max(abs(dx), abs(dy)));
            }
        }
    }
    if nearest > reach {
        return 0.0;
    }
    return f32(reach - nearest + 1) / f32(reach);
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    let center = texel_center(in.world);
    var coverage = 0.0;
    switch mode() {
        case 0: {
            coverage = sun_coverage(in, center);
        }
        case 1: {
            coverage = occlusion(in, center, params.style.y);
        }
        case 2: {
            coverage = occlusion(in, center, -1.0);
        }
        default: {
            // On the art's own pixels: darkest on its lowest opaque row, one step lighter per
            // row up.
            let corner = in.art.yz;
            let pixel = floor(in.local - corner) + 0.5 + corner;
            let row = floor(pixel.y - base_of(in.art));
            let rows = params.style.y;
            if row < rows && opaque(in, pixel) {
                coverage = (rows - row) / rows;
            }
        }
    }
    return vec4<f32>(0.0, 0.0, 0.0, coverage * params.shear.z);
}
