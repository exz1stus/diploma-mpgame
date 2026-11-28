//! Shadows of moving sprites (players and other entities), drawn like a standing object's:
//! sun shadow, ground occlusion and base shade. Their art changes with every animation frame
//! and their draw order with every step, so the silhouettes follow the sprite each frame
//! instead of being placed once.

use bevy::prelude::*;

use super::{
    atlas::{PixelRect, SpriteVariant},
    config::PresentationConfig,
    shadows::{ShadowAssets, ShadowMaterial, ShadowSpawner, Silhouette, SunLight},
};
use shared::world::TILE_SIZE;

/// A sprite, centered on its entity and cut from a sheet by `Sprite::rect`, that casts
/// silhouettes like a standing object. Its parent's transform plus its own place it.
#[derive(Component)]
pub struct ShadowCaster;

/// One silhouette of a [`ShadowCaster`]; a child of it.
#[derive(Component)]
pub struct CasterShadow(Silhouette);

/// The silhouettes of a standing object that has both a shadow and occlusion.
const ROLES: [Silhouette; 3] = [
    Silhouette::StandingOcclusion,
    Silhouette::Sun,
    Silhouette::BaseShade,
];

/// Entity sprites stand upright: no lying height.
const UPRIGHT: f32 = 0.0;

/// The caster's current frame and how it is drawn this frame.
struct CasterPose {
    sheet: Handle<Image>,
    art: SpriteVariant,
    flip: bool,
    /// World units per art pixel.
    scale: f32,
    /// Foot point relative to the caster.
    foot: Vec2,
    /// The caster's world Z.
    z: f32,
}

impl CasterPose {
    fn new(sprite: &Sprite, world_z: f32) -> Option<Self> {
        let rect = sprite.rect?;
        let art = SpriteVariant::bottom_centered(pixel_rect(rect)?);
        let size = sprite.custom_size.unwrap_or(rect.size());
        Some(Self {
            sheet: sprite.image.clone(),
            art,
            flip: sprite.flip_x,
            scale: size.x / art.rect.2 as f32,
            foot: Vec2::new(0.0, -size.y / 2.0),
            z: world_z,
        })
    }

    fn transform(&self, role: Silhouette) -> Transform {
        Transform::from_translation(self.foot.extend(role.z(self.z)))
            .with_scale(Vec3::new(self.scale, self.scale, 1.0))
    }
}

fn pixel_rect(rect: Rect) -> Option<PixelRect> {
    let size = rect.size();
    (rect.min.min_element() >= 0.0 && size.min_element() >= 1.0).then_some(PixelRect(
        rect.min.x as u32,
        rect.min.y as u32,
        size.x as u32,
        size.y as u32,
    ))
}

#[allow(clippy::type_complexity)]
pub fn follow_shadow_casters(
    mut commands: Commands,
    casters: Query<(Entity, &ChildOf, &Transform, &Sprite, Option<&Children>), With<ShadowCaster>>,
    roots: Query<&Transform, (Without<ShadowCaster>, Without<CasterShadow>)>,
    mut shadows: Query<
        (
            &CasterShadow,
            &mut Transform,
            &mut Mesh2d,
            &mut MeshMaterial2d<ShadowMaterial>,
        ),
        Without<ShadowCaster>,
    >,
    config: Res<PresentationConfig>,
    light: Res<SunLight>,
    mut shadow_assets: ResMut<ShadowAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shadow_materials: ResMut<Assets<ShadowMaterial>>,
    images: Res<Assets<Image>>,
) {
    let mut spawner = ShadowSpawner {
        assets: &mut shadow_assets,
        meshes: &mut meshes,
        materials: &mut shadow_materials,
        images: &images,
        light: *light,
        occlusion: config.objects.occlusion,
        texel: TILE_SIZE / config.tiles.blend.sanitized().pixels_per_tile as f32,
    };
    for (caster, child_of, transform, sprite, children) in &casters {
        let root_z = roots
            .get(child_of.parent())
            .map_or(0.0, |root| root.translation.z);
        let Some(pose) = CasterPose::new(sprite, root_z + transform.translation.z) else {
            continue;
        };
        // Silhouettes start where the art touches the ground, read from the loaded sheet.
        if !images.contains(&pose.sheet) {
            continue;
        }
        let mut has_shadows = false;
        for child in children.into_iter().flatten() {
            if let Ok((shadow, transform, mesh, material)) = shadows.get_mut(*child) {
                has_shadows = true;
                update_shadow(&mut spawner, &pose, shadow.0, transform, mesh, material);
            }
        }
        if !has_shadows {
            spawn_shadows(&mut commands, &mut spawner, &pose, caster);
        }
    }
}

fn spawn_shadows(
    commands: &mut Commands,
    spawner: &mut ShadowSpawner,
    pose: &CasterPose,
    caster: Entity,
) {
    for role in ROLES {
        let Some(material) = spawner.material(role, &pose.sheet) else {
            continue;
        };
        commands.spawn((
            CasterShadow(role),
            ChildOf(caster),
            Mesh2d(spawner.mesh(role, &pose.sheet, &pose.art, pose.flip, UPRIGHT)),
            MeshMaterial2d(material),
            pose.transform(role),
        ));
    }
}

/// Moves a silhouette to the caster's current frame and draw order. Handles are looked up
/// again each frame, so silhouettes pick up materials rebuilt with a new world.
fn update_shadow(
    spawner: &mut ShadowSpawner,
    pose: &CasterPose,
    role: Silhouette,
    mut transform: Mut<Transform>,
    mut mesh: Mut<Mesh2d>,
    mut material: Mut<MeshMaterial2d<ShadowMaterial>>,
) {
    transform.set_if_neq(pose.transform(role));
    mesh.set_if_neq(Mesh2d(spawner.mesh(
        role,
        &pose.sheet,
        &pose.art,
        pose.flip,
        UPRIGHT,
    )));
    if let Some(current) = spawner.material(role, &pose.sheet) {
        material.set_if_neq(MeshMaterial2d(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_become_bottom_centered_art_scaled_to_the_drawn_size() {
        let sprite = Sprite {
            rect: Some(Rect::new(32.0, 64.0, 64.0, 96.0)),
            custom_size: Some(Vec2::splat(64.0)),
            ..default()
        };
        let pose = CasterPose::new(&sprite, -5.0).unwrap();
        assert_eq!(pose.art.rect, PixelRect(32, 64, 32, 32));
        assert_eq!(pose.art.pivot(), Vec2::new(16.0, 32.0));
        assert_eq!((pose.scale, pose.foot), (2.0, Vec2::new(0.0, -32.0)));
        assert!(
            CasterPose::new(&Sprite::default(), 0.0).is_none(),
            "no frame, no shadow"
        );
    }
}
