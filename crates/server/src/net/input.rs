use std::collections::{BTreeMap, HashMap};

use bevy::{
    app::{App, FixedUpdate},
    math::Vec2,
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
use super::server::{TickCounter, TickSet};

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<InputBuffer>();
    app.add_systems(FixedUpdate, consume_input.in_set(TickSet::ProcessInputs));
    app.add_observer(clear_disconnected_input);
}

#[derive(Resource, Default)]
pub struct InputBuffer {
    pending: HashMap<ClientId, BTreeMap<u64, MovementInput>>,
    last_input: HashMap<ClientId, MovementInput>,
}

impl InputBuffer {
    pub(crate) fn record(&mut self, client_id: ClientId, input: MovementInput, server_tick: u64) {
        const MAX_FUTURE_TICKS: u64 = 20;
        if input.tick < server_tick || input.tick > server_tick.saturating_add(MAX_FUTURE_TICKS) {
            return;
        }

        self.pending
            .entry(client_id)
            .or_default()
            .insert(input.tick, input);
    }

    fn take_for_tick(&mut self, client_id: ClientId, server_tick: u64) -> Option<MovementInput> {
        let pending = self.pending.get_mut(&client_id)?;
        let newest_ready = pending
            .range(..=server_tick)
            .next_back()
            .map(|(_, input)| *input);
        pending.retain(|tick, _| *tick > server_tick);
        if pending.is_empty() {
            self.pending.remove(&client_id);
        }
        newest_ready
    }
}

fn clear_disconnected_input(event: On<ClientLeft>, mut input_buffer: ResMut<InputBuffer>) {
    input_buffer.pending.remove(&event.client_id);
    input_buffer.last_input.remove(&event.client_id);
}

#[derive(Debug, Component, Default)]
pub(crate) struct CurrentMovementInput {
    pub movement: Vec2,
}

fn consume_input(
    mut input_buffer: ResMut<InputBuffer>,
    tick: bevy_ecs::system::Res<TickCounter>,
    mut players: Query<(&Ownership, &mut CurrentMovementInput), With<Player>>,
) {
    for (ownership, mut current_movement) in &mut players {
        let newest_input = input_buffer.take_for_tick(ownership.client_id, tick.0);
        if let Some(input) = newest_input
            && input_buffer
                .last_input
                .get(&ownership.client_id)
                .is_none_or(|last| input.tick > last.tick)
        {
            input_buffer.last_input.insert(ownership.client_id, input);
        }

        if let Some(input) = input_buffer.last_input.get(&ownership.client_id).copied() {
            current_movement.movement = Vec2::new(input.x as f32, input.y as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_input_waits_for_its_tick_and_missing_tick_holds_previous_input() {
        let mut buffer = InputBuffer::default();
        let client_id = 7;
        buffer.record(
            client_id,
            MovementInput {
                tick: 102,
                x: 1,
                y: 0,
            },
            100,
        );
        assert!(buffer.take_for_tick(client_id, 100).is_none());
        assert!(buffer.take_for_tick(client_id, 101).is_none());
        assert_eq!(buffer.take_for_tick(client_id, 102).unwrap().x, 1);
        assert!(buffer.take_for_tick(client_id, 103).is_none());
    }

    #[test]
    fn late_input_cannot_replace_current_movement() {
        let mut buffer = InputBuffer::default();
        let client_id = 7;
        buffer.record(
            client_id,
            MovementInput {
                tick: 99,
                x: -1,
                y: 0,
            },
            100,
        );
        assert!(buffer.take_for_tick(client_id, 100).is_none());
    }
}
