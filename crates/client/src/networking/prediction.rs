use bevy::{math::Vec2, prelude::*};
use shared::components::NetworkPhysicsState;

use crate::networking::state::Owned;
use crate::networking::{
    prediction_history::PredictionHistory, simulation_clock::ClientSimulationClock,
};

#[derive(Resource)]
pub struct PredictionSettings {
    pub enabled: bool,
}

impl Default for PredictionSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Component)]
pub struct Prediction {
    pub state: NetworkPhysicsState,
    pub visual_offset: Vec2,
}

impl Prediction {
    pub fn new(initial: NetworkPhysicsState) -> Self {
        Self {
            state: initial,
            visual_offset: Vec2::ZERO,
        }
    }
}

pub fn initialize_prediction(
    mut commands: Commands,
    fixed_time: Res<Time<Fixed>>,
    mut clock: ResMut<ClientSimulationClock>,
    players: Query<(Entity, &NetworkPhysicsState), (With<Owned>, Without<Prediction>)>,
) {
    let mut players = players.iter();

    let Some((entity, authoritative)) = players.next() else {
        return;
    };

    if players.next().is_some() {
        warn!("Prediction initialization expects one owned player");
        return;
    }

    let baseline = *authoritative;

    clock.initialize(baseline.tick, fixed_time.timestep().as_secs_f64());

    commands
        .entity(entity)
        .insert((Prediction::new(baseline), PredictionHistory::new(baseline)));
}
