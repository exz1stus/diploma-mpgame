use avian2d::{
    dynamics::{integrator::Gravity, rigid_body::LinearVelocity},
    physics_transform::Position as PhysicsPosition,
};
use bevy::{
    app::{App, FixedPostUpdate},
    math::Vec2,
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::system::Query;
use shared::components::NetworkPhysicsState;

use super::server::TickSet;

pub(crate) fn configure(app: &mut App) {
    app.add_systems(
        FixedPostUpdate,
        sync_positions.in_set(TickSet::SyncPositions),
    );

    app.insert_resource(Gravity(Vec2::ZERO));
}

fn sync_positions(
    mut positions: Query<(&PhysicsPosition, &LinearVelocity, &mut NetworkPhysicsState)>,
) {
    for (physics, velocity, mut network) in &mut positions {
        if network.position != physics.0 || network.velocity != velocity.0 {
            network.position = physics.0;
            network.velocity = velocity.0;
        }
    }
}
