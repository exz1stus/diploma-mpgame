use std::collections::BTreeMap;

use bevy::ecs::component::Component;
use shared::{
    components::NetworkPhysicsState,
    movement::{MovementInput, TickNumber},
};

const MAX_HISTORY_ENTRIES: usize = 256;

#[derive(Component)]
pub struct PredictionHistory {
    inputs: BTreeMap<TickNumber, MovementInput>,
    states: BTreeMap<TickNumber, NetworkPhysicsState>,

    pub last_authoritative_tick: TickNumber,
}
impl PredictionHistory {
    pub fn new(initial: NetworkPhysicsState) -> Self {
        Self {
            inputs: BTreeMap::new(),
            states: BTreeMap::from([(initial.tick, initial)]),
            last_authoritative_tick: initial.tick,
        }
    }
    pub fn record_input(&mut self, input: MovementInput) {
        self.inputs.insert(input.tick, input);

        while self.inputs.len() > MAX_HISTORY_ENTRIES {
            self.inputs.pop_first();
        }
    }
    pub fn record_state(&mut self, state: NetworkPhysicsState) {
        self.states.insert(state.tick, state);

        while self.states.len() > MAX_HISTORY_ENTRIES {
            self.states.pop_first();
        }
    }
    pub fn state_at(&self, tick: TickNumber) -> Option<NetworkPhysicsState> {
        self.states.get(&tick).copied()
    }
    /// Collect inputs AFTER the authoritative tick
    pub fn replay_inputs(
        &self,
        authoritative_tick: TickNumber,
        completed_tick: TickNumber,
    ) -> Option<Vec<MovementInput>> {
        if authoritative_tick > completed_tick {
            return None;
        }

        if authoritative_tick == completed_tick {
            return Some(Vec::new());
        }

        ((authoritative_tick + 1)..=completed_tick)
            .map(|tick| self.inputs.get(&tick).copied())
            .collect()
    }
    /// Call only AFTER a snapshot has been successfully handled
    pub fn accept_authoritative_tick(&mut self, tick: TickNumber) {
        if tick <= self.last_authoritative_tick {
            return;
        }

        self.last_authoritative_tick = tick;

        self.inputs.retain(|input_tick, _| *input_tick > tick);

        self.states.retain(|state_tick, _| *state_tick >= tick);
    }
    pub fn recent_inputs(&self, count: usize) -> Vec<MovementInput> {
        let mut inputs: Vec<_> = self.inputs.values().rev().take(count).copied().collect();

        inputs.reverse();
        inputs
    }
}
