use avian2d::prelude::{LinearVelocity, Position as PhysicsPosition};
use bevy::prelude::*;
use shared::{components::NetworkPhysicsState, movement::MovementInput};

use super::{ReconciliationAction, ReconciliationStatus};
use crate::{
    game::rendering::{LocalVisualPosition, current_visual_position},
    networking::{
        physics::{CurrentMovementInput, PredictedBody, replay_prediction_ticks},
        prediction::Prediction,
        prediction_history::PredictionHistory,
        simulation_clock::ClientSimulationClock,
        state::{Owned, RemoteWorld},
    },
};

pub fn apply_reconciliation(world: &mut World) {
    let Some(action) = take_pending_action(world) else {
        return;
    };

    let Some(player) = find_owned_player(world, action) else {
        warn!(
            entity_id = action.entity_id(),
            "Cannot reconcile missing owned entity"
        );
        return;
    };

    if !prediction_body_is_ready(world, player) {
        world.resource_mut::<ReconciliationStatus>().pending_action = Some(action);
        return;
    }

    match action {
        ReconciliationAction::Rollback { authoritative, .. } => {
            apply_rollback_or_resync(world, player, authoritative);
        }
        ReconciliationAction::HardResync { authoritative, .. } => {
            hard_resync(world, player, authoritative);
        }
    }
}

fn take_pending_action(world: &mut World) -> Option<ReconciliationAction> {
    world
        .resource_mut::<ReconciliationStatus>()
        .pending_action
        .take()
}

fn find_owned_player(world: &World, action: ReconciliationAction) -> Option<Entity> {
    world
        .resource::<RemoteWorld>()
        .entities
        .get(&action.entity_id())
        .filter(|replica| replica.owned)
        .map(|replica| replica.entity)
}

fn prediction_body_is_ready(world: &World, player: Entity) -> bool {
    let Ok(entity) = world.get_entity(player) else {
        return false;
    };

    entity.contains::<Owned>()
        && entity.contains::<PredictedBody>()
        && entity.contains::<Prediction>()
        && entity.contains::<PredictionHistory>()
        && entity.contains::<CurrentMovementInput>()
        && entity.contains::<PhysicsPosition>()
        && entity.contains::<LinearVelocity>()
}

fn apply_rollback_or_resync(world: &mut World, player: Entity, authoritative: NetworkPhysicsState) {
    if rollback_and_replay(world, player, authoritative) {
        world
            .resource_mut::<ReconciliationStatus>()
            .applied_rollbacks += 1;
        return;
    }

    warn!(
        tick = authoritative.tick,
        "Rollback history is incomplete; using hard resynchronization"
    );
    hard_resync(world, player, authoritative);
}

fn rollback_and_replay(
    world: &mut World,
    player: Entity,
    authoritative: NetworkPhysicsState,
) -> bool {
    let visual_position_before_correction = current_visual_position(world, player);
    let Some(completed_tick) = world.resource::<ClientSimulationClock>().completed_tick() else {
        return false;
    };

    let Some(replay_inputs) =
        collect_replay_inputs(world, player, authoritative.tick, completed_tick)
    else {
        return false;
    };

    restore_authoritative_state(world, player, authoritative);
    record_state(world, player, authoritative);

    let timestep = world.resource::<Time<Fixed>>().timestep();
    let corrected_states = replay_prediction_ticks(world, player, &replay_inputs, timestep);

    for state in corrected_states.iter().copied() {
        record_state(world, player, state);
    }

    let corrected_state = corrected_states.last().copied().unwrap_or(authoritative);
    finish_rollback(
        world,
        player,
        authoritative.tick,
        corrected_state,
        visual_position_before_correction,
    );

    true
}

fn collect_replay_inputs(
    world: &World,
    player: Entity,
    authoritative_tick: u64,
    completed_tick: u64,
) -> Option<Vec<MovementInput>> {
    world
        .entity(player)
        .get::<PredictionHistory>()
        .expect("Reconciliation requires prediction history")
        .replay_inputs(authoritative_tick, completed_tick)
}

fn record_state(world: &mut World, player: Entity, state: NetworkPhysicsState) {
    world
        .entity_mut(player)
        .get_mut::<PredictionHistory>()
        .expect("Reconciliation requires prediction history")
        .record_state(state);
}

fn finish_rollback(
    world: &mut World,
    player: Entity,
    authoritative_tick: u64,
    corrected_state: NetworkPhysicsState,
    old_visual_position: Vec2,
) {
    const MAX_SMOOTH_DISTANCE: f32 = 64.0;
    let mut entity = world.entity_mut(player);

    entity
        .get_mut::<PredictionHistory>()
        .expect("Reconciliation requires prediction history")
        .accept_authoritative_tick(authoritative_tick);

    let mut prediction = entity
        .get_mut::<Prediction>()
        .expect("Reconciliation requires prediction state");
    prediction.state = corrected_state;

    let new_offset = old_visual_position - corrected_state.position;

    prediction.visual_offset = if new_offset.length() <= MAX_SMOOTH_DISTANCE {
        new_offset
    } else {
        Vec2::ZERO
    };
}

fn hard_resync(world: &mut World, player: Entity, authoritative: NetworkPhysicsState) {
    restore_authoritative_state(world, player, authoritative);
    if let Some(mut visual) = world.entity_mut(player).get_mut::<LocalVisualPosition>() {
        *visual = LocalVisualPosition::default();
    }
    reset_player_prediction(world, player, authoritative);
    reset_simulation_clock(world, authoritative.tick);

    world.resource_mut::<ReconciliationStatus>().hard_resyncs += 1;
}

fn reset_player_prediction(world: &mut World, player: Entity, authoritative: NetworkPhysicsState) {
    let mut entity = world.entity_mut(player);

    let mut prediction = entity.get_mut::<Prediction>().expect("Hard resync requires prediction");
    prediction.state = authoritative;
    prediction.visual_offset = Vec2::ZERO;

    *entity
        .get_mut::<PredictionHistory>()
        .expect("Hard resync requires prediction history") = PredictionHistory::new(authoritative);

    entity
        .get_mut::<CurrentMovementInput>()
        .expect("Hard resync requires current input")
        .movement = MovementInput {
        tick: authoritative.tick,
        x: 0,
        y: 0,
    };
}

fn reset_simulation_clock(world: &mut World, authoritative_tick: u64) {
    let timestep_secs = world.resource::<Time<Fixed>>().timestep().as_secs_f64();

    world
        .resource_mut::<ClientSimulationClock>()
        .initialize(authoritative_tick, timestep_secs);
}

fn restore_authoritative_state(
    world: &mut World,
    player: Entity,
    authoritative: NetworkPhysicsState,
) {
    let mut entity = world.entity_mut(player);

    entity
        .get_mut::<PhysicsPosition>()
        .expect("Reconciliation requires an Avian position")
        .0 = authoritative.position;

    entity
        .get_mut::<LinearVelocity>()
        .expect("Reconciliation requires an Avian velocity")
        .0 = authoritative.velocity;

    if let Some(mut transform) = entity.get_mut::<Transform>() {
        transform.translation.x = authoritative.position.x;
        transform.translation.y = authoritative.position.y;
    }

    entity
        .get_mut::<Prediction>()
        .expect("Reconciliation requires prediction state")
        .state = authoritative;
}
