//! Live look changes. Whatever changes [`PresentationConfig`] (the F5 dev reload today, the
//! settings UI later) re-styles the loaded world in place: ground and object visuals are
//! dropped and the regular catalog and spawn systems rebuild them, so no setting needs
//! code of its own.

use bevy::prelude::*;

use super::{
    chunks::{ChunkGround, GroundMesh},
    config::PresentationConfig,
    objects::ObjectVisual,
    shadows::ObjectShadow,
    trails::ObjectTrail,
};
use crate::{assets::AssetRoot, world::WorldInfo};

const RELOAD_KEY: KeyCode = KeyCode::F5;

/// Re-reads `atlas.ron`, `tiles.ron` and `objects.ron` when F5 is pressed.
pub fn reload_config_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    root: Res<AssetRoot>,
    mut config: ResMut<PresentationConfig>,
) {
    if keys.just_pressed(RELOAD_KEY) {
        info!("reloading world presentation config");
        *config = PresentationConfig::load(&root);
    }
}

/// Runs when the config changed after startup: drops ground and object visuals (with the
/// objects' shadows and trails) and marks
/// the world changed, which re-runs the catalog systems; the spawn systems then rebuild
/// every visual from the new config.
pub fn restyle_world(
    mut commands: Commands,
    world: Option<ResMut<WorldInfo>>,
    ground_meshes: Query<Entity, With<GroundMesh>>,
    grounds: Query<Entity, With<ChunkGround>>,
    objects: Query<Entity, With<ObjectVisual>>,
    shadows: Query<Entity, With<ObjectShadow>>,
    trails: Query<Entity, With<ObjectTrail>>,
) {
    let Some(mut world) = world else {
        return;
    };
    for entity in ground_meshes.iter().chain(&shadows).chain(&trails) {
        commands.entity(entity).despawn();
    }
    for entity in &grounds {
        commands.entity(entity).remove::<ChunkGround>();
    }
    for entity in &objects {
        commands.entity(entity).remove::<ObjectVisual>();
    }
    world.set_changed();
}
