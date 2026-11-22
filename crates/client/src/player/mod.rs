//! What is unique to the local player: the camera that follows them. Their visual is drawn
//! like every other entity's ([`crate::entities`]).

pub(crate) mod camera;

use bevy::prelude::*;

use crate::schedule::ClientSet;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, camera::spawn_camera).add_systems(
            Update,
            (camera::follow_player_camera, camera::debug_zoom).in_set(ClientSet::Camera),
        );
    }
}
