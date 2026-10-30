use std::collections::HashMap;

use avian2d::dynamics::rigid_body::LinearVelocity;
use bevy::{
    app::{App, FixedUpdate},
    math::Vec2,
    prelude::IntoScheduleConfigs,
    time::{Fixed, Time},
};
use bevy_ecs::{
    observer::On,
    query::With,
    resource::Resource,
    system::{Query, Res, ResMut},
};
use bevy_renet::renet::ClientId;
use shared::{
    components::{Ownership, Player},
    movement::{MovementInput, movement_acceleration},
};

use super::client::ClientLeft;
use super::server::TickSet;

#[derive(Resource, Default)]
pub struct InputBuffer {
    pending: HashMap<ClientId, MovementInput>,
    last_inputs: HashMap<ClientId, MovementInput>,
}

impl InputBuffer {
    pub(crate) fn record(&mut self, client_id: ClientId, input: MovementInput) {
        if self
            .last_inputs
            .get(&client_id)
            .is_some_and(|last| input.tick <= last.tick)
        {
            return;
        }
        if self
            .pending
            .get(&client_id)
            .is_none_or(|pending| input.tick > pending.tick)
        {
            self.pending.insert(client_id, input);
        }
    }
}

pub(crate) fn configure(app: &mut App) {
    app.init_resource::<InputBuffer>();
    app.add_systems(FixedUpdate, apply_input.in_set(TickSet::ProcessInputs));
    app.add_observer(clear_disconnected_input);
}

fn clear_disconnected_input(event: On<ClientLeft>, mut input_buffer: ResMut<InputBuffer>) {
    input_buffer.pending.remove(&event.client_id);
    input_buffer.last_inputs.remove(&event.client_id);
}

fn apply_input(
    time: Res<Time<Fixed>>,
    mut input_buffer: ResMut<InputBuffer>,
    mut players: Query<(&Ownership, &mut LinearVelocity), With<Player>>,
) {
    for (ownership, mut velocity) in &mut players {
        let newest_input = input_buffer.pending.remove(&ownership.client_id);
        if let Some(input) = newest_input {
            if input_buffer
                .last_inputs
                .get(&ownership.client_id)
                .is_none_or(|last| input.tick > last.tick)
            {
                input_buffer.last_inputs.insert(ownership.client_id, input);
            }
        }

        if let Some(input) = input_buffer.last_inputs.get(&ownership.client_id).copied() {
            velocity.0 = next_velocity(velocity.0, input, time.delta_secs());
        }
    }
}

fn next_velocity(velocity: Vec2, input: MovementInput, dt: f32) -> Vec2 {
    velocity + movement_acceleration(velocity, input) * dt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_input_accumulates_velocity_over_fixed_ticks() {
        let input = MovementInput {
            tick: 1,
            x: 1,
            y: 0,
        };
        let mut velocity = Vec2::ZERO;
        let dt = 1.0 / 60.0;

        for _ in 0..60 {
            velocity = next_velocity(velocity, input, dt);
        }

        assert!(velocity.x > 90.0, "velocity after one second: {velocity:?}");
        assert_eq!(velocity.y, 0.0);
    }
}
