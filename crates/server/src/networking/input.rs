use std::collections::HashMap;

use bevy::{
    app::{App, FixedUpdate},
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::{
    component::Component,
    observer::On,
    query::With,
    resource::Resource,
    system::{Query, ResMut},
};
use bevy_renet::renet::ClientId;
use shared::{
    components::{Ownership, Player},
    movement::MovementInput,
};

use super::client::ClientLeft;
use super::server::TickSet;

#[derive(Component, Clone, Copy)]
pub(crate) struct CurrentMovementInput {
    pub(crate) movement: MovementInput,
}

impl Default for CurrentMovementInput {
    fn default() -> Self {
        Self {
            movement: MovementInput {
                tick: 0,
                x: 0,
                y: 0,
            },
        }
    }
}

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
    app.add_systems(FixedUpdate, select_input.in_set(TickSet::ProcessInputs));
    app.add_observer(clear_disconnected_input);
}

fn clear_disconnected_input(event: On<ClientLeft>, mut input_buffer: ResMut<InputBuffer>) {
    input_buffer.pending.remove(&event.client_id);
    input_buffer.last_inputs.remove(&event.client_id);
}

fn select_input(
    mut input_buffer: ResMut<InputBuffer>,
    mut players: Query<(&Ownership, &mut CurrentMovementInput), With<Player>>,
) {
    for (ownership, mut current) in &mut players {
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
            current.movement = input;
        }
    }
}
