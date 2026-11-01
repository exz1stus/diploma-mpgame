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
use bevy_ecs::{
    query::With,
    system::{Query, Res},
};
use shared::{
    components::{NetworkPhysicsState, Player},
    movement::movement_acceleration,
    replication::{ReplicationSet, collect_baseline},
};

use crate::networking::{input::CurrentMovementInput, server::TickCounter};

use super::server::TickSet;

pub(crate) fn configure(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        apply_movement
            .in_set(TickSet::Simulate)
            .before(avian2d::schedule::PhysicsSystems::First),
    );

    app.add_systems(
        FixedPostUpdate,
        (
            sync_positions.in_set(TickSet::SyncPositions),
            collect_baseline::<NetworkPhysicsState>
                .run_if(is_baseline_tick)
                .in_set(ReplicationSet::Collect),
        ),
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

fn is_baseline_tick(tick_counter: Res<TickCounter>) -> bool {
    const BASELINE_INTERVAL: u64 = 3;
    tick_counter.0.is_multiple_of(BASELINE_INTERVAL)
}
