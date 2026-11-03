use avian2d::{
    dynamics::rigid_body::forces::{Forces, ReadRigidBodyForces, WriteRigidBodyForces},
    prelude::{
        AngularVelocity, Collider, Gravity, LinearVelocity, LockedAxes, Physics, PhysicsPlugins,
        PhysicsSystems, Position as PhysicsPosition, RigidBody, Substeps,
    },
};
use bevy::{ecs::schedule::ScheduleLabel, prelude::*};
use shared::{
    client_messages::ClientMessages,
    components::NetworkPhysicsState,
    movement::{MovementInput, movement_acceleration},
};
use std::time::Duration;

use super::{
    network::{NetworkSimulation, queue_outgoing},
    prediction::Prediction,
    prediction_history::PredictionHistory,
    simulation_clock::{ClientSimulationClock, MAX_CATCHUP_TICKS},
    state::{ClientState, Owned},
};

const MAX_STEPS_PER_FRAME: usize = 8;
const INPUT_REDUNDANCY: usize = 32;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClientPhysicsTick;
#[derive(Component)]
pub struct PredictedBody;
#[derive(Component)]
pub struct CurrentMovementInput {
    pub movement: MovementInput,
}

#[derive(Resource, Default)]
struct PredictionStepTime(Time);

pub fn configure(app: &mut App) {
    app.init_schedule(ClientPhysicsTick);
    app.add_plugins(PhysicsPlugins::new(ClientPhysicsTick));

    app.insert_resource(Gravity(Vec2::ZERO));
    app.init_resource::<PredictionStepTime>();

    app.add_systems(
        ClientPhysicsTick,
        apply_movement.before(PhysicsSystems::First),
    );
}

/// Runs after initialize_prediction has established the baseline.
pub fn initialize_physics_body(
    mut commands: Commands,
    players: Query<
        (Entity, &Prediction),
        (With<Owned>, With<PredictionHistory>, Without<PredictedBody>),
    >,
) {
    for (entity, prediction) in &players {
        let initial = prediction.state;

        commands.entity(entity).insert((
            PredictedBody,
            CurrentMovementInput {
                movement: MovementInput {
                    tick: initial.tick,
                    x: 0,
                    y: 0,
                },
            },
            RigidBody::Dynamic,
            Collider::circle(16.0),
            PhysicsPosition(initial.position),
            LinearVelocity(initial.velocity),
            AngularVelocity::ZERO,
            LockedAxes::ROTATION_LOCKED,
            Transform::from_xyz(initial.position.x, initial.position.y, 0.0),
        ));
    }
}

/// Uses the same forcesbased controller as the server
fn apply_movement(
    mut players: Query<(&CurrentMovementInput, Forces), (With<Owned>, With<PredictedBody>)>,
) {
    for (input, mut forces) in &mut players {
        let acceleration = movement_acceleration(
            forces.linear_velocity(),
            Vec2::new(input.movement.x as f32, input.movement.y as f32),
        );

        forces.apply_linear_acceleration(acceleration);
    }
}

/// Runs live prediction toward the simulation clocks target
pub fn run_prediction(world: &mut World) {
    if !world.resource::<ClientState>().initiated {
        return;
    }

    let Some(player) = find_local_player(world) else {
        return;
    };

    let now = world.resource::<Time<Real>>().elapsed_secs_f64();

    let timestep = world.resource::<Time<Fixed>>().timestep();

    let (completed, target, estimated_server_tick) = {
        let clock = world.resource::<ClientSimulationClock>();

        let (Some(completed), Some(target), Some(estimated_server)) = (
            clock.completed_tick(),
            clock.target_tick(now),
            clock.estimated_server_tick(now),
        ) else {
            return;
        };

        (completed, target, estimated_server.floor() as u64)
    };

    if target.saturating_sub(completed) > MAX_CATCHUP_TICKS {
        warn!(
            completed,
            target, "Prediction needs a fresh authoritative baseline"
        );
        return;
    }

    let (keyboard_x, keyboard_y) = read_movement_input(world);

    for _ in 0..MAX_STEPS_PER_FRAME {
        let tick = {
            let mut clock = world.resource_mut::<ClientSimulationClock>();

            let Some(tick) = clock.begin_tick(now) else {
                break;
            };

            tick
        };

        let (x, y) = if tick <= estimated_server_tick {
            (0, 0)
        } else {
            (keyboard_x, keyboard_y)
        };

        let input = MovementInput { tick, x, y };

        world
            .entity_mut(player)
            .get_mut::<PredictionHistory>()
            .expect("Predicted body must have history")
            .record_input(input);

        let state = simulate_prediction_tick(world, player, input, timestep);

        {
            let mut entity = world.entity_mut(player);

            entity
                .get_mut::<PredictionHistory>()
                .expect("Predicted body must have history")
                .record_state(state);

            let mut prediction = entity
                .get_mut::<Prediction>()
                .expect("Predicted body must have prediction");

            prediction.state = state;
        }

        world
            .resource_mut::<ClientSimulationClock>()
            .finish_tick(tick);

        if tick > estimated_server_tick {
            queue_recent_inputs(world, player, now);
        }
    }
}

pub fn simulate_prediction_tick(
    world: &mut World,
    player: Entity,
    input: MovementInput,
    timestep: Duration,
) -> NetworkPhysicsState {
    world
        .entity_mut(player)
        .get_mut::<CurrentMovementInput>()
        .expect("Predicted body must have current input")
        .movement = input;

    let previous_time = world.resource::<Time>().clone();

    let step_time = {
        let mut step_clock = world.resource_mut::<PredictionStepTime>();

        step_clock.0.advance_by(timestep);
        step_clock.0.clone()
    };

    *world.resource_mut::<Time>() = step_time;

    world.run_schedule(ClientPhysicsTick);

    *world.resource_mut::<Time>() = previous_time;

    let entity = world.entity(player);

    NetworkPhysicsState {
        tick: input.tick,
        position: entity
            .get::<PhysicsPosition>()
            .expect("Predicted body must have position")
            .0,
        velocity: entity
            .get::<LinearVelocity>()
            .expect("Predicted body must have velocity")
            .0,
    }
}

pub fn replay_prediction_ticks(
    world: &mut World,
    player: Entity,
    inputs: &[MovementInput],
    timestep: Duration,
) -> Vec<NetworkPhysicsState> {
    let saved_step_time = world.resource::<PredictionStepTime>().0.clone();
    let saved_physics_time = world.resource::<Time<Physics>>().clone();
    let saved_substep_time = world.resource::<Time<Substeps>>().clone();

    let states = inputs
        .iter()
        .copied()
        .map(|input| simulate_prediction_tick(world, player, input, timestep))
        .collect();

    world.resource_mut::<PredictionStepTime>().0 = saved_step_time;
    *world.resource_mut::<Time<Physics>>() = saved_physics_time;
    *world.resource_mut::<Time<Substeps>>() = saved_substep_time;

    states
}

fn find_local_player(world: &mut World) -> Option<Entity> {
    let mut query = world.query_filtered::<Entity, (
        With<Owned>,
        With<PredictedBody>,
        With<Prediction>,
        With<PredictionHistory>,
    )>();

    let mut players = query.iter(world);
    let player = players.next()?;

    if players.next().is_some() {
        warn!("Expected exactly one locally predicted player");
        return None;
    }

    Some(player)
}

fn read_movement_input(world: &World) -> (i8, i8) {
    let keys = world.resource::<ButtonInput<KeyCode>>();

    let x = i8::from(keys.pressed(KeyCode::KeyD)) - i8::from(keys.pressed(KeyCode::KeyA));

    let y = i8::from(keys.pressed(KeyCode::KeyW)) - i8::from(keys.pressed(KeyCode::KeyS));

    (x, y)
}

fn queue_recent_inputs(world: &mut World, player: Entity, now: f64) {
    let inputs = world
        .entity(player)
        .get::<PredictionHistory>()
        .expect("Predicted body must have history")
        .recent_inputs(INPUT_REDUNDANCY);

    world.resource_scope(|world, mut simulation: Mut<NetworkSimulation>| {
        let mut client_state = world.resource_mut::<ClientState>();

        queue_outgoing(
            &mut simulation,
            &mut client_state,
            ClientMessages::MovementInputs { inputs },
            now,
        );
    });
}

pub fn log_prediction_status(
    time: Res<Time<Real>>,
    clock: Res<ClientSimulationClock>,
    players: Query<(&Prediction, &PredictionHistory), (With<Owned>, With<PredictedBody>)>,
    mut next_log_at: Local<f64>,
) {
    let now = time.elapsed_secs_f64();

    if now < *next_log_at {
        return;
    }

    *next_log_at = now + 1.0;

    for (prediction, history) in &players {
        let completed = clock.completed_tick();

        let history_exists = completed.and_then(|tick| history.state_at(tick)).is_some();

        info!(
            completed = ?completed,
            target = ?clock.target_tick(now),
            predicted_tick = prediction.state.tick,
            history_exists,
            position = ?prediction.state.position,
            velocity = ?prediction.state.velocity,
            "Prediction status"
        );
    }
}
