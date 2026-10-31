use std::collections::HashMap;

use avian2d::dynamics::rigid_body::LinearVelocity;
use bevy::{
    app::{App, FixedUpdate},
    math::Vec2,
    prelude::IntoScheduleConfigs,
    time::{Fixed, Time},
};
use bevy_ecs::{
    component::Component,
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
    app.add_systems(FixedUpdate, consume_input.in_set(TickSet::ProcessInputs));
    app.add_observer(clear_disconnected_input);
}

fn clear_disconnected_input(event: On<ClientLeft>, mut input_buffer: ResMut<InputBuffer>) {
    input_buffer.pending.remove(&event.client_id);
    input_buffer.last_inputs.remove(&event.client_id);
}

#[derive(Debug, Component, Default)]
pub(crate) struct CurrentMovementInput {
    pub movement: Vec2,
}

fn consume_input(
    mut input_buffer: ResMut<InputBuffer>,
    mut players: Query<(&Ownership, &mut CurrentMovementInput), With<Player>>,
) {
    for (ownership, mut current_movement) in &mut players {
        let newest_input = input_buffer.pending.remove(&ownership.client_id);
        if let Some(input) = newest_input
            && input_buffer
                .last_inputs
                .get(&ownership.client_id)
                .is_none_or(|last| input.tick > last.tick)
        {
            input_buffer.last_inputs.insert(ownership.client_id, input);
        }

        if let Some(input) = input_buffer.last_inputs.get(&ownership.client_id).copied() {
            current_movement.movement = Vec2::new(input.x as f32, input.y as f32);
        }
    }
}
