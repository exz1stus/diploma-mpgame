//! Attaches a visual child to every networked entity once its kind arrives. The child is
//! the kind's texture, or a tinted circle while that texture file is missing.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::{components::EntityKind, snapshots::NetworkIdentity};

use super::{
    catalog::{EntityVisual, EntityVisuals},
    motion::{InterpolationDebug, LocalVisualPosition},
};
use crate::assets::{AssetRoot, texture_file_exists, texture_path};

const ENTITY_FOLDER: &str = "entities";

/// The visual child of a networked entity.
#[derive(Component)]
pub struct Visual {
    /// Distance from the entity's position down to its foot, which sets its draw order.
    pub foot_offset: f32,
}

/// Texture and placeholder mesh per kind, loaded when the first entity of that kind arrives.
#[derive(Resource, Default)]
pub struct KindAssets(HashMap<String, KindAsset>);

#[derive(Clone)]
struct KindAsset {
    texture: Option<Handle<Image>>,
    circle: Handle<Mesh>,
}

pub fn spawn_entity_visuals(
    mut commands: Commands,
    new_entities: Query<(Entity, &NetworkIdentity, &EntityKind), Added<EntityKind>>,
    visuals: Res<EntityVisuals>,
    root: Res<AssetRoot>,
    asset_server: Res<AssetServer>,
    mut kind_assets: ResMut<KindAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, identity, kind) in &new_entities {
        let visual = visuals.get(&kind.0);
        let assets = kind_assets
            .0
            .entry(kind.0.clone())
            .or_insert_with(|| {
                let texture = visuals.texture_name(&kind.0);
                load_kind_asset(&root, &asset_server, &mut meshes, texture, visual)
            })
            .clone();
        let color = visual.tint.color(identity.id);

        commands.entity(entity).insert((
            InterpolationDebug::default(),
            LocalVisualPosition::default(),
        ));
        let mut child = commands.spawn((
            Visual {
                foot_offset: visual.size / 2.0,
            },
            ChildOf(entity),
            Transform::default(),
        ));
        match assets.texture {
            Some(image) => child.insert(Sprite {
                image,
                color,
                custom_size: Some(Vec2::splat(visual.size)),
                ..default()
            }),
            None => child.insert((
                Mesh2d(assets.circle),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
            )),
        };
    }
}

fn load_kind_asset(
    root: &AssetRoot,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    texture: &str,
    visual: &EntityVisual,
) -> KindAsset {
    KindAsset {
        texture: texture_file_exists(root, ENTITY_FOLDER, texture)
            .then(|| asset_server.load(texture_path(ENTITY_FOLDER, texture))),
        circle: meshes.add(Circle::new(visual.size / 2.0)),
    }
}
