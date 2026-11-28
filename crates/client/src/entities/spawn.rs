//! Attaches a visual child to every networked entity once its kind arrives. The child is
//! the kind's animated sheet or texture, or a tinted circle while those files are missing.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::{components::EntityKind, snapshots::NetworkIdentity};

use super::{
    animation::{EntitySheet, SheetAnimator},
    catalog::{EntityVisual, EntityVisuals},
    motion::{InterpolationDebug, LocalVisualPosition},
};
use crate::{
    assets::{AssetRoot, load_pixel_art, texture_file_exists, texture_path},
    world::presentation::ShadowCaster,
};

const ENTITY_FOLDER: &str = "entities";

/// The visual child of a networked entity.
#[derive(Component)]
pub struct Visual {
    /// Distance from the entity's position down to its foot, which sets its draw order.
    pub foot_offset: f32,
}

/// What a kind is drawn with, loaded when the first entity of that kind arrives.
#[derive(Resource, Default)]
pub struct KindAssets(HashMap<String, KindAsset>);

#[derive(Clone)]
enum KindAsset {
    Sheet(Handle<Image>, EntitySheet),
    Texture(Handle<Image>),
    Circle(Handle<Mesh>),
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
        let asset = kind_assets
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
        let drawn_size = asset.drawn_size(visual);
        let mut child = commands.spawn((
            Visual {
                foot_offset: drawn_size.y / 2.0,
            },
            ChildOf(entity),
            Transform::default(),
        ));
        let size = Some(drawn_size);
        match asset {
            KindAsset::Sheet(image, sheet) => {
                let animator = SheetAnimator::new(sheet);
                child.insert((
                    Sprite {
                        image,
                        color,
                        custom_size: size,
                        rect: animator.first_rect(),
                        ..default()
                    },
                    animator,
                    ShadowCaster,
                ))
            }
            KindAsset::Texture(image) => child.insert(Sprite {
                image,
                color,
                custom_size: size,
                ..default()
            }),
            KindAsset::Circle(circle) => child.insert((
                Mesh2d(circle),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
            )),
        };
    }
}

impl KindAsset {
    /// World size of the visual: a sheet frame at its own pixel size, else the kind's `size`.
    fn drawn_size(&self, visual: &EntityVisual) -> Vec2 {
        match self {
            Self::Sheet(_, sheet) => sheet.frame_world_size(),
            Self::Texture(_) | Self::Circle(_) => Vec2::splat(visual.size),
        }
    }
}

/// The kind's sheet, else its texture, else a circle of its size.
fn load_kind_asset(
    root: &AssetRoot,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    texture: &str,
    visual: &EntityVisual,
) -> KindAsset {
    if let Some(sheet) = visual
        .sheet
        .as_ref()
        .filter(|sheet| root.join(&sheet.image).is_file())
    {
        return KindAsset::Sheet(load_pixel_art(asset_server, &sheet.image), sheet.clone());
    }
    if texture_file_exists(root, ENTITY_FOLDER, texture) {
        return KindAsset::Texture(asset_server.load(texture_path(ENTITY_FOLDER, texture)));
    }
    KindAsset::Circle(meshes.add(Circle::new(visual.size / 2.0)))
}
