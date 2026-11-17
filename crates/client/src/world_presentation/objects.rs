//! Object sprites: `textures/objects/<texture>.png` sized to the footprint, or a
//! placeholder rectangle while the texture file is missing.

use bevy::prelude::*;
use shared::world::TILE_SIZE;

use super::{
    catalog::ObjectCatalog,
    config::PresentationConfig,
    textures::{AssetRoot, OBJECT_FOLDER, request_textures},
};
use crate::game::world::{WorldInfo, WorldObject};

/// Draw order of objects: above the ground, below players.
const OBJECT_Z: f32 = -9.0;
/// Placeholder rectangles leave a gap so neighbouring objects stay distinguishable.
const PLACEHOLDER_SCALE: f32 = 0.9;

/// Texture handle per object catalog entry; `None` where no file exists.
#[derive(Resource)]
pub struct ObjectTextures(Vec<Option<Handle<Image>>>);

/// Marks an object whose sprite has been attached.
#[derive(Component)]
pub struct ObjectVisual;

pub fn prepare_object_catalog(
    mut commands: Commands,
    world: Res<WorldInfo>,
    config: Res<PresentationConfig>,
    root: Res<AssetRoot>,
    asset_server: Res<AssetServer>,
) {
    let catalog = ObjectCatalog::build(&world, &config.objects);
    let handles = request_textures(&root, OBJECT_FOLDER, &catalog.textures, &asset_server);
    commands.insert_resource(ObjectTextures(handles));
    commands.insert_resource(catalog);
}

/// The sprite goes on the object entity itself: objects stream in by the thousand, and a
/// child entity per object would double spawn and transform-propagation cost.
pub fn spawn_object_visuals(
    mut commands: Commands,
    mut objects: Query<(Entity, &WorldObject, &mut Transform), Without<ObjectVisual>>,
    catalog: Res<ObjectCatalog>,
    textures: Res<ObjectTextures>,
) {
    for (entity, object, mut transform) in &mut objects {
        let size = Vec2::new(
            object.footprint.width as f32,
            object.footprint.height as f32,
        ) * TILE_SIZE;
        let sprite = object_sprite(&catalog, &textures, object, size);
        transform.translation.z = OBJECT_Z;
        commands.entity(entity).insert((ObjectVisual, sprite));
    }
}

fn object_sprite(
    catalog: &ObjectCatalog,
    textures: &ObjectTextures,
    object: &WorldObject,
    size: Vec2,
) -> Sprite {
    let index = catalog.texture_of.get(usize::from(object.kind.0));
    let texture = index.and_then(|&index| textures.0.get(index as usize)?.clone());
    match (texture, catalog.texture(object.kind)) {
        (Some(image), _) => Sprite {
            image,
            custom_size: Some(size),
            ..default()
        },
        (None, Some(entry)) => Sprite::from_color(entry.placeholder, size * PLACEHOLDER_SCALE),
        (None, None) => Sprite::from_color(Srgba::rgb(1.0, 0.0, 1.0), size * PLACEHOLDER_SCALE),
    }
}

pub fn clear_objects(mut commands: Commands) {
    commands.remove_resource::<ObjectCatalog>();
    commands.remove_resource::<ObjectTextures>();
}
