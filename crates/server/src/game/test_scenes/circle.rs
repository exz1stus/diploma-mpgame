use avian2d::{
    collision::collider::Collider,
    dynamics::rigid_body::{LinearVelocity, RigidBody},
    physics_transform::Position,
};
use bevy::prelude::*;
use shared::components::{Health, Ownership};

use crate::{game::player::new_player_bundle, networking::entity::NetworkCommandsExt};

pub(super) fn spawn(mut commands: Commands) {}
