use avian2d::{
    collision::collider::Collider,
    dynamics::rigid_body::{AngularVelocity, LinearVelocity, LockedAxes, RigidBody},
    physics_transform::Position,
};
use bevy::{app::App, math::Vec2, transform::components::Transform};
use bevy_ecs::{bundle::Bundle, observer::On, system::Commands};
use shared::components::{DisconnectPolicy, Health, NetworkPhysicsState, Player};

use crate::networking::{client::ClientIntiated, entity::NetworkCommandsExt};

pub fn configure(app: &mut App) {
    app.add_observer(spawn_player_on_intiated);
}

pub fn new_player_bundle() -> impl Bundle {
    let spawn = Vec2::new(0.0, 0.0);
    (
        Player,
        Health(100),
        NetworkPhysicsState::default(),
        RigidBody::Dynamic,
        Collider::circle(16.0),
        Position(spawn),
        LinearVelocity::ZERO,
        AngularVelocity::ZERO,
        LockedAxes::ROTATION_LOCKED,
        Transform::from_xyz(spawn.x, spawn.y, 0.0),
    )
}

fn spawn_player_on_intiated(event: On<ClientIntiated>, mut commands: Commands) {
    commands.network_spawn_with_ownership_and_policy(
        event.client_id,
        DisconnectPolicy::Despawn,
        new_player_bundle(),
    );
}

pub fn update_player() {}

pub fn validate_player_movement() {}
