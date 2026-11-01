use avian2d::{
    dynamics::{
        integrator::Gravity,
        rigid_body::{
            LinearVelocity,
            forces::{Forces, ReadRigidBodyForces, WriteRigidBodyForces},
        },
    },
    physics_transform::Position as PhysicsPosition,
};
use bevy::{
    app::{App, FixedPostUpdate, FixedUpdate},
    math::Vec2,
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::{query::With, system::Query};
use shared::{
    components::{NetworkPhysicsState, Player},
    movement::movement_acceleration,
};

use super::{input::CurrentMovementInput, server::TickSet};

pub(crate) fn configure(app: &mut App) {
    app.add_systems(FixedUpdate, apply_movement.in_set(TickSet::Simulate));

    app.add_systems(
        FixedPostUpdate,
        sync_positions.in_set(TickSet::SyncPositions),
    );

    app.insert_resource(Gravity(Vec2::ZERO));
}

/// Applies the same acceleration controller used by client prediction.
/// Avian integrates the acceleration over the configured fixed timestep.
fn apply_movement(mut players: Query<(&CurrentMovementInput, Forces), With<Player>>) {
    for (input, mut forces) in &mut players {
        let acceleration = movement_acceleration(forces.linear_velocity(), input.movement);

        forces.apply_linear_acceleration(acceleration);
    }
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
