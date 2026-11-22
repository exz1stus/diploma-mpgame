//! Object sprites from the atlas. Each object picks its kind's art for the surface it
//! stands on, then a variant, offset, mirroring and scale from its anchor tile
//! ([`place`]), colored by its biome's object tint. Kinds without art are drawn as a
//! placeholder rectangle over the footprint.

use bevy::{prelude::*, sprite::Anchor};
use shared::world::{TILE_SIZE, TilePos, TileSample};

use bevy::{image::ImageSampler, render::render_resource::TextureFormat};

use super::{
    atlas::{load_sheet, sheet_settled},
    catalog::{KindVisual, ObjectCatalog},
    config::{ObjectLayer, PresentationConfig},
    placement::place,
    shadows::{ShadowAssets, ShadowMaterial, ShadowSpawner, Silhouette, SunLight},
    textures::recolor_regions,
};
use crate::{
    render_layers,
    world::{ChunkTiles, InChunk, WorldChunk, WorldInfo, WorldObject},
};

/// Placeholder rectangles leave a gap so neighbouring objects stay distinguishable.
const PLACEHOLDER_SCALE: f32 = 0.9;

/// The sheets object sprites are cut from: the loaded atlas sheet, which untinted kinds draw
/// from, and the sheet tinted kinds draw from: the same one, or a copy with every sprite made
/// neutral (`objects.ron` `recolor`). Objects wait until `tinted` is ready.
#[derive(Resource)]
pub struct ObjectSheet {
    source: Handle<Image>,
    recolor: Option<Srgba>,
    tinted: Option<Handle<Image>>,
}

impl ObjectSheet {
    fn for_kind(&self, kind: &KindVisual) -> Option<Handle<Image>> {
        let tinted = self.tinted.clone()?;
        Some(if kind.style.tinted {
            tinted
        } else {
            self.source.clone()
        })
    }
}

/// Marks an object whose sprite has been attached.
#[derive(Component)]
pub struct ObjectVisual;

pub fn prepare_object_catalog(
    mut commands: Commands,
    world: Res<WorldInfo>,
    config: Res<PresentationConfig>,
    asset_server: Res<AssetServer>,
) {
    let source = load_sheet(&asset_server, &config.atlas);
    let recolor = config.objects.recolor();
    commands.insert_resource(ObjectSheet {
        tinted: recolor.is_none().then(|| source.clone()),
        source,
        recolor,
    });
    commands.insert_resource(ObjectCatalog::build(&world, &config.objects));
    commands.insert_resource(ShadowAssets::default());
}

/// Once the sheet has loaded, makes the recolored copy tinted kinds draw from.
pub fn recolor_object_sheet(
    mut sheet: ResMut<ObjectSheet>,
    config: Res<PresentationConfig>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(color) = sheet.recolor.filter(|_| sheet.tinted.is_none()) else {
        return;
    };
    if !sheet_settled(&sheet.source, &asset_server) {
        return;
    }
    let recolored = images
        .get(&sheet.source)
        .and_then(|source| source.convert(TextureFormat::Rgba8UnormSrgb))
        .map(|mut copy| {
            recolor_regions(&mut copy, &config.atlas.sprite_rects(), color);
            copy.sampler = ImageSampler::nearest();
            images.add(copy)
        });
    sheet.tinted = Some(recolored.unwrap_or_else(|| sheet.source.clone()));
}

/// The sprite goes on the object entity itself: objects stream in by the thousand, and a
/// child entity per object would double spawn and transform-propagation cost.
pub fn spawn_object_visuals(
    mut commands: Commands,
    mut objects: Query<(Entity, &WorldObject, &InChunk, &mut Transform), Without<ObjectVisual>>,
    chunks: Query<(&WorldChunk, &ChunkTiles)>,
    catalog: Res<ObjectCatalog>,
    sheet: Res<ObjectSheet>,
    config: Res<PresentationConfig>,
    light: Res<SunLight>,
    mut shadow_assets: ResMut<ShadowAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shadow_materials: ResMut<Assets<ShadowMaterial>>,
) {
    if sheet.tinted.is_none() {
        return;
    }
    let world_per_px = TILE_SIZE / config.tiles.blend.sanitized().pixels_per_tile as f32;
    let mut shadows = ShadowSpawner {
        assets: &mut shadow_assets,
        meshes: &mut meshes,
        materials: &mut shadow_materials,
        light: *light,
        occlusion: config.objects.occlusion,
        // Art pixels and terrain texels share one size: pixels_per_tile per tile.
        texel: world_per_px,
    };
    for (entity, object, in_chunk, mut transform) in &mut objects {
        let Some((kind, image)) = catalog
            .kind(object.kind)
            .and_then(|kind| Some((kind, sheet.for_kind(kind)?)))
        else {
            continue;
        };
        let sample = chunks
            .get(in_chunk.0)
            .ok()
            .and_then(|(chunk, tiles)| tile_at(chunk, tiles, object.anchor));
        let surface = sample.and_then(|sample| catalog.surface(sample));
        let object_tint = sample
            .filter(|_| kind.style.tinted)
            .and_then(|sample| catalog.tint(sample));
        let color = object_color(kind, object_tint);
        let (sprite, anchor) = match config.atlas.sprite(kind.sprite(surface)) {
            Some(variants) => {
                let placement = place(
                    object.anchor,
                    object.footprint,
                    &kind.style,
                    variants,
                    kind.salt,
                    world_per_px,
                    // Snap to the terrain pixel grid, the same size as art pixels.
                    world_per_px,
                );
                let art = variants[placement.variant];
                transform.translation.z = object_z(kind, placement.foot.y);
                let sprite = Sprite {
                    image: image.clone(),
                    rect: Some(art.rect.to_rect()),
                    custom_size: Some(placement.size),
                    flip_x: placement.flip,
                    color,
                    ..default()
                };
                for role in silhouettes(kind) {
                    shadows.spawn(
                        &mut commands,
                        role,
                        entity,
                        &transform,
                        &image,
                        &art,
                        &placement,
                    );
                }
                (sprite, Anchor(placement.anchor))
            }
            None => {
                let size = Vec2::new(
                    object.footprint.width as f32,
                    object.footprint.height as f32,
                ) * TILE_SIZE;
                let south = transform.translation.y - size.y / 2.0;
                transform.translation.z = object_z(kind, south);
                (
                    Sprite::from_color(kind.placeholder, size * PLACEHOLDER_SCALE),
                    Anchor::CENTER,
                )
            }
        };
        commands
            .entity(entity)
            .insert((ObjectVisual, sprite, anchor));
    }
}

/// The kind's own tint, times the object tint of the biome under it (tinted kinds only); both
/// sRGB, multiplied per channel like the terrain shader multiplies tiles.
fn object_color(kind: &KindVisual, object_tint: Option<[u8; 3]>) -> Color {
    let tint = kind.style.tint().to_srgba();
    let [r, g, b] = object_tint.map_or([1.0; 3], |rgb| rgb.map(|value| f32::from(value) / 255.0));
    Srgba::new(tint.red * r, tint.green * g, tint.blue * b, tint.alpha).into()
}

/// The silhouettes a kind gets: ground occlusion, and if it stands a sun shadow and the shade
/// on its own bottom rows.
fn silhouettes(kind: &KindVisual) -> Vec<Silhouette> {
    let standing = kind.style.layer == ObjectLayer::Sorted;
    let occlusion = kind.style.occlusion;
    let mut roles = Vec::new();
    if occlusion {
        roles.push(if standing {
            Silhouette::StandingOcclusion
        } else {
            Silhouette::GroundOcclusion
        });
    }
    if standing && kind.style.shadow {
        roles.push(Silhouette::Sun);
    }
    if standing && occlusion {
        roles.push(Silhouette::BaseShade);
    }
    roles
}

fn object_z(kind: &KindVisual, foot_y: f32) -> f32 {
    match kind.style.layer {
        ObjectLayer::Sorted => render_layers::y_sorted(foot_y),
        ObjectLayer::Ground => render_layers::DECALS,
    }
}

/// The tile at a world tile position, if it lies in this chunk.
fn tile_at(chunk: &WorldChunk, tiles: &ChunkTiles, pos: TilePos) -> Option<TileSample> {
    let size = i64::from(tiles.size);
    let (x, y) = (pos.x - chunk.pos.x * size, pos.y - chunk.pos.y * size);
    if !(0..size).contains(&x) || !(0..size).contains(&y) {
        return None;
    }
    tiles.tiles.get((y * size + x) as usize).copied()
}

pub fn clear_objects(mut commands: Commands) {
    commands.remove_resource::<ObjectCatalog>();
    commands.remove_resource::<ObjectSheet>();
}
