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
    /// Collect inputs AFTER the authoritative tick,
    /// through  the last completed local tick.
    /// Returns None if  required input is missing.
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
    /// Call only AFTER a snapshot has been successfully handled.
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_requires_every_input_after_the_snapshot() {
        let mut history = PredictionHistory::new(NetworkPhysicsState {
            tick: 100,
            ..Default::default()
        });

        for tick in [101, 103] {
            history.record_input(MovementInput { tick, x: 1, y: 0 });
        }

        // Tick 102 is missing: replay must fail explicitly.
        assert!(history.replay_inputs(100, 103).is_none());

        history.record_input(MovementInput {
            tick: 102,
            x: 0,
            y: 1,
        });

        let inputs = history.replay_inputs(100, 103).unwrap();

        let ticks: Vec<_> = inputs.iter().map(|input| input.tick).collect();

        assert_eq!(ticks, vec![101, 102, 103]);

        history.accept_authoritative_tick(102);

        // Tick 103 is still needed after accepting state 102.
        assert_eq!(history.replay_inputs(102, 103).unwrap()[0].tick, 103,);
    }
}
