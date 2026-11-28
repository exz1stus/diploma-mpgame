//! Object shadows and ambient occlusion (`shadow.wgsl`), in whole terrain texels.
//!
//! The sun stands where worldgen's solar model puts it (the same model temperatures come from)
//! for the world clock's time, seen from the camera's position. A sun shadow is the object's
//! silhouette sheared along the ground: its base stays where the object touches the ground, its
//! top slides away from the sun, further as the sun gets lower, and it fades out at dusk.
//! Ambient occlusion darkens the ground next to an object's lowest rows in whole-texel steps,
//! like the tiles' height shading (all around pebbles), and darkens the art's own bottom rows so
//! it sits in the ground. Silhouettes of one sheet and role share one material, so the sun moves
//! every shadow with a single uniform; meshes are shared per sprite variant.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d},
};
use shared::world::{SunPosition, TILE_SIZE, TilePos, WorldGeography};

use super::{
    atlas::{PixelRect, SpriteVariant},
    config::{OcclusionStyle, PresentationConfig, ShadowStyle},
    placement::Placement,
};
use crate::{
    render_layers,
    world::{WorldInfo, clock::WorldClock},
};

pub const SHADER_PATH: &str = "embedded://client/world/presentation/shadow.wgsl";

/// Z of the base shade over its object: just above it, below anything standing south of it.
const BASE_SHADE_Z: f32 = 2.0e-6;

/// An object's sun shadow, occlusion or base shade; a child of the object.
#[derive(Component)]
pub struct ObjectShadow;

/// What a silhouette draws; the shader's mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Silhouette {
    /// Sun shadow of standing art; follows the sun.
    Sun,
    /// Ground occlusion next to standing art's lowest rows.
    StandingOcclusion,
    /// Ground occlusion all around a ground decal.
    GroundOcclusion,
    /// Darkening of standing art's own bottom rows, drawn over it.
    BaseShade,
}

impl Silhouette {
    fn mode(self) -> f32 {
        match self {
            Self::Sun => 0.0,
            Self::StandingOcclusion => 1.0,
            Self::GroundOcclusion => 2.0,
            Self::BaseShade => 3.0,
        }
    }

    /// Texels (or rows, for the base shade) the effect reaches; 0 draws nothing.
    fn reach(self, occlusion: OcclusionStyle) -> u32 {
        match self {
            Self::Sun => 0,
            Self::StandingOcclusion => occlusion.reach,
            Self::GroundOcclusion => occlusion.decal_reach,
            Self::BaseShade => occlusion.base_rows,
        }
    }

    fn enabled(self, occlusion: OcclusionStyle) -> bool {
        self == Self::Sun || self.reach(occlusion) > 0
    }

    /// How far the mesh grows past the art, in art pixels, so no texel at its edge is cut.
    fn margin(self, occlusion: OcclusionStyle) -> f32 {
        match self {
            Self::Sun => 2.0,
            Self::StandingOcclusion | Self::GroundOcclusion => self.reach(occlusion) as f32 + 2.0,
            Self::BaseShade => 0.0,
        }
    }

    fn params(self, light: SunLight, occlusion: OcclusionStyle, texel: f32) -> ShadowParams {
        let darkness = match self {
            Self::Sun => light.alpha,
            Self::StandingOcclusion | Self::GroundOcclusion => occlusion.opacity,
            Self::BaseShade => occlusion.base_opacity,
        };
        let shear = light.shear();
        ShadowParams {
            shear: Vec4::new(shear.x, shear.y, darkness, 0.0),
            style: Vec4::new(self.mode(), self.reach(occlusion) as f32, texel, 0.0),
        }
    }

    /// Z relative to the object.
    pub fn z(self, object_z: f32) -> f32 {
        match self {
            Self::Sun => render_layers::SHADOWS - object_z,
            Self::StandingOcclusion | Self::GroundOcclusion => render_layers::OCCLUSION - object_z,
            Self::BaseShade => BASE_SHADE_Z,
        }
    }
}

/// How shadows fall right now.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SunLight {
    /// World direction shadows point in, away from the sun.
    pub direction: Vec2,
    /// Shadow length as a multiple of the object's height.
    pub length: f32,
    /// Shadow darkness; 0 while the sun is down.
    pub alpha: f32,
    /// The least a shadow spreads up or down, as a fraction of the object's height.
    pub depth: f32,
}

impl SunLight {
    pub fn new(sun: SunPosition, style: ShadowStyle) -> Self {
        let style = style.sanitized();
        let elevation = sun.elevation() as f32;
        // Worldgen's Y grows southward, so north is world -Y.
        let toward_sun = Vec2::new(sun.east as f32, -sun.north as f32);
        // Horizontal over vertical extent of the sun direction: the cotangent of its elevation.
        let length = if sun.up > 0.0 {
            (toward_sun.length() / sun.up as f32).min(style.max_length)
        } else {
            style.max_length
        };
        let fade = (elevation / style.fade_degrees.to_radians()).clamp(0.0, 1.0);
        Self {
            direction: (-toward_sun).normalize_or(Vec2::Y),
            length,
            alpha: style.opacity * fade * fade * (3.0 - 2.0 * fade),
            depth: style.depth,
        }
    }

    /// Ground offset per unit of height. Its vertical part spreads at least `depth` (upward,
    /// behind the object, when the sun stands east or west), so objects keep some depth.
    pub fn shear(self) -> Vec2 {
        let shear = self.direction * self.length;
        let spread = shear.y.abs().max(self.depth);
        Vec2::new(shear.x, if shear.y < 0.0 { -spread } else { spread })
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ShadowMaterial {
    /// The sheet the silhouettes come from (only its alpha is read).
    #[texture(0)]
    #[sampler(1)]
    pub sheet: Handle<Image>,
    #[uniform(2)]
    pub params: ShadowParams,
}

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowParams {
    /// Sun shear (xy), darkness (z).
    pub shear: Vec4,
    /// Mode (x), reach (y), terrain texel size in world units (z).
    pub style: Vec4,
}

impl Material2d for ShadowMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Silhouette materials per sheet and role, and meshes per sprite variant and margin, shared by
/// every silhouette of the current world; rebuilt with the object catalog.
#[derive(Resource, Default)]
pub struct ShadowAssets {
    materials: HashMap<(AssetId<Image>, Silhouette), Handle<ShadowMaterial>>,
    meshes: HashMap<MeshKey, Handle<Mesh>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MeshKey {
    sheet: AssetId<Image>,
    rect: PixelRect,
    pivot: (u32, u32),
    flip: bool,
    margin: u32,
    lying_height: u32,
}

/// What spawning a silhouette needs, besides the object.
pub struct ShadowSpawner<'a> {
    pub assets: &'a mut ShadowAssets,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<ShadowMaterial>,
    /// Sheets, read for where art touches the ground.
    pub images: &'a Assets<Image>,
    pub light: SunLight,
    pub occlusion: OcclusionStyle,
    /// Terrain texel size in world units.
    pub texel: f32,
}

impl ShadowSpawner<'_> {
    /// Gives `object` (at `transform`) a silhouette of `art` as placed, from `sheet`;
    /// `lying_height` as in [`silhouette_mesh`].
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        commands: &mut Commands,
        role: Silhouette,
        object: Entity,
        transform: &Transform,
        sheet: &Handle<Image>,
        art: &SpriteVariant,
        placement: &Placement,
        lying_height: f32,
    ) {
        let Some(material) = self.material(role, sheet) else {
            return;
        };
        let mesh = self.mesh(role, sheet, art, placement.flip, lying_height);
        let foot = placement.foot - transform.translation.truncate();
        let world_per_px = placement.size.x / art.rect.2 as f32;
        commands.spawn((
            ObjectShadow,
            ChildOf(object),
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_translation(foot.extend(role.z(transform.translation.z)))
                .with_scale(Vec3::new(world_per_px, world_per_px, 1.0)),
        ));
    }

    /// The shared material of `role` for silhouettes cut from `sheet`; `None` when the role
    /// is turned off.
    pub fn material(
        &mut self,
        role: Silhouette,
        sheet: &Handle<Image>,
    ) -> Option<Handle<ShadowMaterial>> {
        let occlusion = self.occlusion.sanitized();
        if !role.enabled(occlusion) {
            return None;
        }
        let params = role.params(self.light, occlusion, self.texel);
        let material = self
            .assets
            .materials
            .entry((sheet.id(), role))
            .or_insert_with(|| {
                self.materials.add(ShadowMaterial {
                    sheet: sheet.clone(),
                    params,
                })
            });
        Some(material.clone())
    }

    /// The shared mesh of `role` for `art` on `sheet`, mirrored when `flip`. Until the sheet
    /// has loaded, the art is taken to touch the ground with its bottom row and the mesh is
    /// not shared.
    pub fn mesh(
        &mut self,
        role: Silhouette,
        sheet: &Handle<Image>,
        art: &SpriteVariant,
        flip: bool,
        lying_height: f32,
    ) -> Handle<Mesh> {
        let margin = role.margin(self.occlusion.sanitized());
        let Some(image) = self.images.get(sheet) else {
            return self
                .meshes
                .add(silhouette_mesh(art, flip, margin, 0, lying_height));
        };
        let pivot = art.pivot();
        let key = MeshKey {
            sheet: sheet.id(),
            rect: art.rect,
            pivot: (pivot.x.to_bits(), pivot.y.to_bits()),
            flip,
            margin: margin.to_bits(),
            lying_height: lying_height.to_bits(),
        };
        self.assets
            .meshes
            .entry(key)
            .or_insert_with(|| {
                let contact = empty_bottom_rows(image, art.rect);
                self.meshes
                    .add(silhouette_mesh(art, flip, margin, contact, lying_height))
            })
            .clone()
    }
}

/// Transparent rows at the bottom of `rect` on `sheet`: the art touches the ground just above
/// them, so its shadow and occlusion start there.
fn empty_bottom_rows(sheet: &Image, rect: PixelRect) -> u32 {
    let opaque_row = |y: u32| {
        (rect.0..rect.0 + rect.2).any(|x| {
            sheet
                .get_color_at(x, y)
                .is_ok_and(|color| color.alpha() > 0.5)
        })
    };
    (0..rect.3)
        .take_while(|row| !opaque_row(rect.1 + rect.3 - 1 - row))
        .count() as u32
}

/// The art's quad in art pixels with its foot at the origin (+Y up), grown by `margin` on every
/// side; UVs in sheet pixels (beyond the art past its edges). Every vertex also carries the art's
/// bounds and shape: color = (mirrored ? -1 : 1, left, bottom, `contact`: transparent bottom
/// rows), normal = (width, height, `lying_height`: art pixels tall for art lying on the ground,
/// 0 for upright art).
fn silhouette_mesh(
    art: &SpriteVariant,
    flip: bool,
    margin: f32,
    contact: u32,
    lying_height: f32,
) -> Mesh {
    let rect = art.rect;
    let (width, height) = (rect.2 as f32, rect.3 as f32);
    let pivot = art.pivot();
    // Mirrored art mirrors its pivot too.
    let left = if flip { pivot.x - width } else { -pivot.x };
    let bottom = pivot.y - height;
    let mirror = if flip { -1.0 } else { 1.0 };
    let u = |x: f32| {
        if flip {
            rect.0 as f32 + (left + width - x)
        } else {
            rect.0 as f32 + (x - left)
        }
    };
    let v = |y: f32| rect.1 as f32 + (bottom + height - y);
    let (x0, x1) = (left - margin, left + width + margin);
    let (y0, y1) = (bottom - margin, bottom + height + margin);
    let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        corners.map(|[x, y]| [x, y, 0.0]).to_vec(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        corners.map(|[x, y]| [u(x), v(y)]).to_vec(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[mirror, left, bottom, contact as f32]; 4],
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![[width, height, lying_height]; 4],
    )
    .with_inserted_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]))
}

/// The sun for the clock's time at the camera's position.
pub fn update_sun(
    clock: Res<WorldClock>,
    world: Res<WorldInfo>,
    config: Res<PresentationConfig>,
    cameras: Query<&GlobalTransform, With<Camera2d>>,
    mut light: ResMut<SunLight>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let tile = (camera.translation().truncate() / TILE_SIZE).floor();
    let geography = WorldGeography::new(world.chunk_size, world.world_size_chunks);
    let place = geography.geo(TilePos {
        x: tile.x as i64,
        y: tile.y as i64,
    });
    let sun = SunPosition::at(clock.time_of_day(), place, &world.planet);
    light.set_if_neq(SunLight::new(sun, config.objects.shadow));
}

/// Moves every sun shadow with the sun: one uniform per sheet (occlusion stays put).
pub fn apply_sun_to_shadows(
    light: Res<SunLight>,
    assets: Res<ShadowAssets>,
    mut materials: ResMut<Assets<ShadowMaterial>>,
) {
    if !light.is_changed() {
        return;
    }
    let shear = light.shear();
    let sun_materials = assets
        .materials
        .iter()
        .filter(|((_, role), _)| *role == Silhouette::Sun);
    for (_, material) in sun_materials {
        if let Some(mut material) = materials.get_mut(material) {
            material.params.shear = Vec4::new(shear.x, shear.y, light.alpha, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::presentation::atlas::PixelRect;

    fn sun(east: f64, north: f64, up: f64) -> SunPosition {
        let length = (east * east + north * north + up * up).sqrt();
        SunPosition {
            east: east / length,
            north: north / length,
            up: up / length,
        }
    }

    #[test]
    fn shadows_point_away_from_the_sun_and_lengthen_as_it_sinks() {
        let style = ShadowStyle::default();
        let morning = SunLight::new(sun(1.0, 0.0, 1.0), style);
        assert!(
            morning.direction.abs_diff_eq(Vec2::NEG_X, 1e-6),
            "east sun, shadow west"
        );
        assert!(
            (morning.length - 1.0).abs() < 1e-5,
            "45° sun: shadow as long as the object"
        );
        // A northern sun (world -Y) throws shadows toward +Y.
        let north = SunLight::new(sun(0.0, 1.0, 1.0), style);
        assert!(north.direction.abs_diff_eq(Vec2::Y, 1e-6));
        let low = SunLight::new(sun(1.0, 0.0, 0.05), style);
        assert_eq!(low.length, style.max_length);
        assert!(low.alpha < morning.alpha, "shadows fade toward the horizon");
        assert_eq!(
            SunLight::new(sun(1.0, 0.0, -0.2), style).alpha,
            0.0,
            "none at night"
        );
        assert_eq!(
            SunLight::new(sun(0.0, 0.0, 1.0), style).length,
            0.0,
            "overhead: no length"
        );
    }

    #[test]
    fn side_light_keeps_the_shadow_deep_and_its_vertical_direction() {
        let style = ShadowStyle::default();
        let east = SunLight::new(sun(1.0, 0.0, 1.0), style).shear();
        assert!(
            east.abs_diff_eq(Vec2::new(-1.0, style.depth), 1e-5),
            "spread up, behind the object: {east}"
        );
        let south = SunLight::new(sun(0.0, -1.0, 1.0), style).shear();
        assert!(
            (south.y + 1.0).abs() < 1e-5,
            "a long shadow keeps its own spread"
        );
    }

    #[test]
    fn roles_carry_their_mode_reach_and_texel() {
        let occlusion = OcclusionStyle::default();
        let light = SunLight::new(sun(1.0, 0.0, 1.0), ShadowStyle::default());
        let standing = Silhouette::StandingOcclusion.params(light, occlusion, 2.0);
        assert_eq!(
            standing.style,
            Vec4::new(1.0, occlusion.reach as f32, 2.0, 0.0)
        );
        assert_eq!(standing.shear.z, occlusion.opacity);
        let base = Silhouette::BaseShade.params(light, occlusion, 2.0);
        assert_eq!((base.style.x, base.shear.z), (3.0, occlusion.base_opacity));
        assert_eq!(
            Silhouette::Sun.params(light, occlusion, 2.0).shear.z,
            light.alpha
        );
        let none = OcclusionStyle {
            decal_reach: 0,
            ..occlusion
        };
        assert!(!Silhouette::GroundOcclusion.enabled(none));
        assert!(Silhouette::StandingOcclusion.z(-5.0) < Silhouette::Sun.z(-5.0));
        assert!(Silhouette::BaseShade.z(-5.0) > 0.0, "over its object");
    }

    /// The shader as Bevy would hand it to naga, with its imports stubbed.
    fn resolved_shader() -> String {
        let imports = "
        fn get_world_from_local(instance_index: u32) -> mat4x4<f32> { return mat4x4<f32>(); }
        fn mesh2d_position_local_to_world(m: mat4x4<f32>, p: vec4<f32>) -> vec4<f32> { return m * p; }
        fn mesh2d_position_world_to_clip(p: vec4<f32>) -> vec4<f32> { return p; }";
        let source = include_str!("shadow.wgsl");
        let start = source.find("#import").unwrap();
        let end = source[start..].find('}').unwrap() + start + 1;
        format!("{}{imports}{}", &source[..start], &source[end..])
            .replace("mesh_functions::", "")
            .replace("#{MATERIAL_BIND_GROUP}", "2")
    }

    #[test]
    fn shadow_shader_is_valid_wgsl() {
        let shader = resolved_shader();
        let module = naga::front::wgsl::parse_str(&shader)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&shader)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}"));
    }

    #[test]
    fn meshes_grow_by_their_margin_and_carry_the_art_bounds() {
        let art: SpriteVariant =
            ron::from_str("(rect: (10, 20, 8, 4), pivot: Some((2.0, 4.0)))").unwrap();
        assert_eq!(art.rect, PixelRect(10, 20, 8, 4));
        let attribute = |mesh: &Mesh, id| -> Vec<Vec<f32>> {
            match mesh.attribute(id).unwrap() {
                bevy::mesh::VertexAttributeValues::Float32x2(values) => {
                    values.iter().map(|v| v.to_vec()).collect()
                }
                bevy::mesh::VertexAttributeValues::Float32x3(values) => {
                    values.iter().map(|v| v.to_vec()).collect()
                }
                bevy::mesh::VertexAttributeValues::Float32x4(values) => {
                    values.iter().map(|v| v.to_vec()).collect()
                }
                _ => unreachable!(),
            }
        };
        let plain = silhouette_mesh(&art, false, 0.0, 0, 0.0);
        assert_eq!(
            attribute(&plain, Mesh::ATTRIBUTE_POSITION)[0],
            [-2.0, 0.0, 0.0],
            "foot at origin"
        );
        assert_eq!(
            attribute(&plain, Mesh::ATTRIBUTE_UV_0)[0],
            [10.0, 24.0],
            "bottom-left texel"
        );
        assert_eq!(
            attribute(&plain, Mesh::ATTRIBUTE_COLOR)[0],
            [1.0, -2.0, 0.0, 0.0]
        );
        assert_eq!(
            attribute(&plain, Mesh::ATTRIBUTE_NORMAL)[0],
            [8.0, 4.0, 0.0]
        );
        let grown = silhouette_mesh(&art, true, 2.0, 1, 24.0);
        assert_eq!(
            attribute(&grown, Mesh::ATTRIBUTE_POSITION)[0],
            [-8.0, -2.0, 0.0]
        );
        // Mirrored: the left edge shows the art's right column; 2 px beyond it.
        assert_eq!(attribute(&grown, Mesh::ATTRIBUTE_UV_0)[0], [20.0, 26.0]);
        assert_eq!(
            attribute(&grown, Mesh::ATTRIBUTE_COLOR)[0],
            [-1.0, -6.0, 0.0, 1.0],
            "carries its transparent bottom rows"
        );
        assert_eq!(
            attribute(&grown, Mesh::ATTRIBUTE_NORMAL)[0],
            [8.0, 4.0, 24.0],
            "carries its lying height"
        );
    }

    #[test]
    fn art_touches_the_ground_above_its_transparent_bottom_rows() {
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

        // 2×4 sheet: opaque top two rows, transparent bottom two.
        let mut data = vec![255u8; 2 * 4 * 4];
        for pixel in data.chunks_mut(4).skip(4) {
            pixel[3] = 0;
        }
        let sheet = Image::new(
            Extent3d {
                width: 2,
                height: 4,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::all(),
        );
        assert_eq!(empty_bottom_rows(&sheet, PixelRect(0, 0, 2, 4)), 2);
        assert_eq!(empty_bottom_rows(&sheet, PixelRect(0, 0, 2, 2)), 0);
        assert_eq!(
            empty_bottom_rows(&sheet, PixelRect(0, 2, 2, 2)),
            2,
            "all empty"
        );
    }
}
