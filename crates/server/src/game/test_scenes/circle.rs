use avian2d::{
    collision::collider::Collider, dynamics::rigid_body::RigidBody, physics_transform::Position,
};
use bevy::prelude::*;
use shared::components::{NetworkPhysicsState, Player};

use crate::networking::entity::NetworkCommandsExt;

pub(super) fn spawn(mut commands: Commands) {}
