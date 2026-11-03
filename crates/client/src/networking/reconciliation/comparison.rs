use std::collections::VecDeque;

use bevy::prelude::*;
use shared::components::NetworkPhysicsState;

use super::{ReconciliationAction, ReconciliationStatus};
use crate::networking::{
    prediction_history::PredictionHistory,
    simulation_clock::{ClientSimulationClock, MAX_CATCHUP_TICKS},
    state::{Owned, RemoteWorld},
};

const POSITION_ERROR_THRESHOLD: f32 = 0.05;
const VELOCITY_ERROR_THRESHOLD: f32 = 0.10;

pub fn compare_authoritative_snapshots(
    clock: Res<ClientSimulationClock>,
    mut remote_world: ResMut<RemoteWorld>,
    mut status: ResMut<ReconciliationStatus>,
    mut histories: Query<&mut PredictionHistory, With<Owned>>,
) {
    let Some(completed_tick) = clock.completed_tick() else {
        return;
    };

    let snapshots = std::mem::take(&mut remote_world.authoritative_snapshots);
    let mut deferred = VecDeque::new();

    for (entity_id, authoritative) in snapshots {
        let Some(replica) = remote_world.entities.get(&entity_id) else {
            continue;
        };

        let Ok(mut history) = histories.get_mut(replica.entity) else {
            deferred.push_back((entity_id, authoritative));
            continue;
        };

        if authoritative.tick <= history.last_authoritative_tick {
            status.stale_snapshots += 1;
            continue;
        }

        if authoritative.tick > completed_tick {
            if gap_requires_resync(completed_tick, authoritative.tick) {
                status.large_gap_resyncs += 1;
                keep_newest_action(
                    &mut status.pending_action,
                    ReconciliationAction::HardResync {
                        entity_id,
                        authoritative,
                    },
                );
                continue;
            }

            deferred.push_back((entity_id, authoritative));
            status.deferred_snapshots += 1;
            continue;
        }

        let Some(predicted) = history.state_at(authoritative.tick) else {
            status.missing_history_snapshots += 1;
            keep_newest_action(
                &mut status.pending_action,
                ReconciliationAction::HardResync {
                    entity_id,
                    authoritative,
                },
            );
            continue;
        };

        let (position_error, velocity_error) = physics_errors(predicted, authoritative);
        record_comparison(
            &mut status,
            authoritative.tick,
            position_error,
            velocity_error,
        );

        if correction_required(position_error, velocity_error) {
            status.mismatching_snapshots += 1;
            let action = if external_interaction_likely(position_error, velocity_error) {
                ReconciliationAction::HardResync {
                    entity_id,
                    authoritative,
                }
            } else {
                ReconciliationAction::Rollback {
                    entity_id,
                    authoritative,
                }
            };
            keep_newest_action(&mut status.pending_action, action);
            continue;
        }

        status.matching_snapshots += 1;
        clear_obsolete_action(&mut status, authoritative.tick);
        history.accept_authoritative_tick(authoritative.tick);
    }

    remote_world.authoritative_snapshots = deferred;
}

fn physics_errors(
    predicted: NetworkPhysicsState,
    authoritative: NetworkPhysicsState,
) -> (f32, f32) {
    (
        predicted.position.distance(authoritative.position),
        predicted.velocity.distance(authoritative.velocity),
    )
}

fn correction_required(position_error: f32, velocity_error: f32) -> bool {
    position_error > POSITION_ERROR_THRESHOLD || velocity_error > VELOCITY_ERROR_THRESHOLD
}

fn external_interaction_likely(position_error: f32, velocity_error: f32) -> bool {
    const LARGE_POSITION_ERROR: f32 = 2.0;
    const LARGE_VELOCITY_ERROR: f32 = 20.0;

    position_error > LARGE_POSITION_ERROR || velocity_error > LARGE_VELOCITY_ERROR
}

fn gap_requires_resync(completed_tick: u64, authoritative_tick: u64) -> bool {
    authoritative_tick.saturating_sub(completed_tick) > MAX_CATCHUP_TICKS
}

fn record_comparison(
    status: &mut ReconciliationStatus,
    tick: u64,
    position_error: f32,
    velocity_error: f32,
) {
    status.last_compared_tick = Some(tick);
    status.last_position_error = position_error;
    status.last_velocity_error = velocity_error;
}

fn clear_obsolete_action(status: &mut ReconciliationStatus, authoritative_tick: u64) {
    if status
        .pending_action
        .is_some_and(|action| action.tick() <= authoritative_tick)
    {
        status.pending_action = None;
    }
}

fn keep_newest_action(current: &mut Option<ReconciliationAction>, candidate: ReconciliationAction) {
    if current.is_none_or(|existing| candidate.tick() > existing.tick()) {
        *current = Some(candidate);
    }
}
