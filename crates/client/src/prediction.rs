use std::collections::BTreeMap;

use bevy::{math::Vec2, prelude::*};
use shared::{
    client_messages::ClientMessages,
    components::NetworkPhysicsState,
    movement::{MovementInput, TickNumber, movement_acceleration},
};

use crate::network::{NetworkSimulation, queue_outgoing};
use crate::state::{ClientState, Owned};

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
    pub current_tick: TickNumber,
    pub last_ack: Option<TickNumber>,
    pub state: NetworkPhysicsState,
    pub input_buffer: BTreeMap<TickNumber, MovementInput>,
    pub prediction_history: BTreeMap<TickNumber, NetworkPhysicsState>,
    pub visual_offset: Vec2,
}

impl Prediction {
    pub fn new(initial: NetworkPhysicsState) -> Self {
        Self {
            current_tick: 0,
            last_ack: None,
            state: initial,
            input_buffer: BTreeMap::new(),
            prediction_history: BTreeMap::from([(0, initial)]),
            visual_offset: Vec2::ZERO,
        }
    }

    fn next_input(&mut self, x: i8, y: i8) -> MovementInput {
        self.current_tick += 1;
        MovementInput {
            tick: self.current_tick,
            x,
            y,
        }
    }

    fn apply(&mut self, input: MovementInput, dt: f32) {
        self.input_buffer.insert(input.tick, input);
        self.state.velocity += movement_acceleration(self.state.velocity, input) * dt;
        self.state.position += self.state.velocity * dt;
        self.prediction_history
            .insert(input.tick, self.state.clone());
    }

    fn reset_to(&mut self, state: &NetworkPhysicsState) {
        self.state = state.clone();
        self.input_buffer.clear();
        self.prediction_history.clear();
        self.prediction_history
            .insert(self.current_tick, self.state);
    }
}

pub fn initialize_prediction(
    mut commands: Commands,
    players: Query<(Entity, &NetworkPhysicsState), (With<Owned>, Without<Prediction>)>,
) {
    for (entity, position) in &players {
        commands
            .entity(entity)
            .insert(Prediction::new(position.clone()));
    }
}

pub fn predict_movement(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Fixed>>,
    real_time: Res<Time<Real>>,
    settings: Res<PredictionSettings>,
    mut client_state: ResMut<ClientState>,
    mut simulation: ResMut<NetworkSimulation>,
    mut players: Query<(&NetworkPhysicsState, &mut Prediction), With<Owned>>,
    mut previous_enabled: Local<Option<bool>>,
) {
    if !client_state.initiated {
        return;
    }
    let x = i8::from(keys.pressed(KeyCode::KeyD)) - i8::from(keys.pressed(KeyCode::KeyA));

    let y = i8::from(keys.pressed(KeyCode::KeyW)) - i8::from(keys.pressed(KeyCode::KeyS));

    let dt = time.timestep().as_secs_f32();
    let resumed = *previous_enabled == Some(false) && settings.enabled;
    *previous_enabled = Some(settings.enabled);
    for (position, mut prediction) in &mut players {
        if resumed {
            prediction.reset_to(position);
        }
        let input = prediction.next_input(x, y);
        if settings.enabled {
            prediction.apply(input, dt);
        }
        queue_outgoing(
            &mut simulation,
            &mut client_state,
            ClientMessages::MovementInputs {
                inputs: vec![input],
            },
            real_time.elapsed_secs_f64(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasd_predicts_and_queues_the_same_input() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        app.insert_resource(Time::<Real>::default());
        app.insert_resource(ClientState {
            initiated: true,
            ..Default::default()
        });
        app.init_resource::<NetworkSimulation>();
        app.init_resource::<PredictionSettings>();
        app.add_systems(Update, predict_movement);

        let entity = app
            .world_mut()
            .spawn((
                Owned,
                NetworkPhysicsState::default(),
                Prediction::new(NetworkPhysicsState::default()),
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyD);

        app.update();

        let prediction = app.world().get::<Prediction>(entity).unwrap();
        assert!(prediction.state.position.x > 0.0);
        let queued = &app.world().resource::<NetworkSimulation>().outgoing[0].message;
        assert!(matches!(
            &queued.payload,
            ClientMessages::MovementInputs { inputs }
                if inputs.len() == 1 && inputs[0].x == 1 && inputs[0].y == 0
        ));
    }

    #[test]
    fn disabling_prediction_still_sends_input_and_resumes_from_server_position() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        app.insert_resource(Time::<Real>::default());
        app.insert_resource(ClientState {
            initiated: true,
            ..Default::default()
        });
        app.init_resource::<NetworkSimulation>();
        app.insert_resource(PredictionSettings { enabled: false });
        app.add_systems(Update, predict_movement);

        let entity = app
            .world_mut()
            .spawn((
                Owned,
                NetworkPhysicsState::default(),
                Prediction::new(NetworkPhysicsState::default()),
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyD);
        app.update();

        assert_eq!(
            app.world()
                .get::<Prediction>(entity)
                .unwrap()
                .state
                .position
                .x,
            0.0
        );
        assert_eq!(
            app.world().resource::<NetworkSimulation>().outgoing.len(),
            1
        );

        app.world_mut()
            .get_mut::<NetworkPhysicsState>(entity)
            .unwrap()
            .position
            .x = 50.0;
        app.world_mut().resource_mut::<PredictionSettings>().enabled = true;
        app.update();

        assert!(
            app.world()
                .get::<Prediction>(entity)
                .unwrap()
                .state
                .position
                .x
                > 50.0
        );
    }
}
