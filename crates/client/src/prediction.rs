use std::collections::BTreeMap;

use bevy::{ecs::entity, input::keyboard::KeyCode::KeyA, math::VectorSpace, prelude::*, state::commands};
use shared::{
    components::Position,
    movement::{MovementInput, MovementState, TickNumber, step},
};

use crate::state::{ClientState, Owned};

#[derive(Component)]
pub struct Prediction {
    pub current_tick: TickNumber,
    pub last_ack: Option<TickNumber>,
    pub state: MovementState,
    pub input_buffer: BTreeMap<TickNumber, MovementInput>,
    pub prediction_history: BTreeMap<TickNumber, MovementState>,
    pub visual_offset: Vec2,
}
impl Prediction {
    pub fn new(initial: MovementState) -> Self {
        Self {
            current_tick: 0,
            last_ack: None,
            state: initial,
            input_buffer: BTreeMap::new(),
            prediction_history: BTreeMap::from([(0, initial)]),
            visual_offset: Vec2::ZERO,
        }
    }
    pub fn predict(&mut self, x: i8, y: i8, dt: f32) -> MovementInput {
        self.current_tick += 1;

        let input = MovementInput {
            tick: self.current_tick,
            x,
            y,
        };
        self.input_buffer.insert(input.tick, input);
        self.state = step(self.state, input, dt);
        self.prediction_history.insert(input.tick, self.state);
        input
    }
}
pub fn initialize_prediction(
    mut commands: Commands,
    players: Query<(Entity, &Position), (With<Owned>, Without<Prediction>)>,
) {
    for (entity, position) in &players {
        commands
            .entity(entity)
            .insert(Prediction::new(MovementState {
                x: position.x,
                y: position.y,
            }));
    }
}
pub fn predict_movement(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Fixed>>,
    client_state: Res<ClientState>,
    mut players: Query<&mut Prediction,With<Owned>>,
){
    if !client_state.initiated {
        return;
    }
    let x =  i8::from(keys.pressed(KeyCode::KeyD))
        - i8::from(keys.pressed(KeyCode::KeyA));
    
    let y = i8::from(keys.pressed(KeyCode::KeyW))
        - i8::from(keys.pressed(KeyCode::KeyS));

    let dt = time.timestep().as_secs_f32();
    for  mut prediction in &mut players {
        prediction.predict(x, y, dt);
    }
}
